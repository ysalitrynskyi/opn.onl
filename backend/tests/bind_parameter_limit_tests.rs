//! Queries must not bind one SQL parameter per row.
//!
//! Postgres (through sqlx) refuses a statement with more than 65,535 bind
//! parameters, so a handler that loads ids and passes them back as `IN (...)`
//! fails once a tag, an organization or a blocked domain covers more rows than
//! that. Each test but the last seeds 65,536 rows (one more than the limit) with
//! a single `generate_series` insert and removes them again before it asserts,
//! so the shared database stays small. They are slow by design: keep one large
//! seed per code path.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_email};
use opn_onl_backend::entity::{
    blocked_domains, blocked_email_domains, click_events, link_tags, links, organizations, tags,
    users,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    DbBackend, EntityTrait, PaginatorTrait, QueryFilter, Statement,
};
use serde_json::{Value, json};

/// One more than the 65,535 parameters a single statement may bind.
const OVER_THE_LIMIT: u32 = 65_536;

// ---------------------------------------------------------------- helpers

async fn register_verified(
    server: &axum_test::TestServer,
    db: &DatabaseConnection,
) -> (String, i32) {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    let user_id = body["user_id"].as_i64().unwrap() as i32;
    mark_email_verified(db, user_id).await;
    (body["token"].as_str().unwrap().to_string(), user_id)
}

async fn register_admin(server: &axum_test::TestServer, db: &DatabaseConnection) -> (String, i32) {
    let (token, user_id) = register_verified(server, db).await;
    sql(
        db,
        format!("UPDATE users SET is_admin = TRUE WHERE id = {user_id}"),
    )
    .await;
    (token, user_id)
}

async fn create_org(server: &axum_test::TestServer, token: &str) -> i32 {
    let res = server
        .post("/orgs")
        .authorization_bearer(token)
        .json(&json!({
            "name": "Bind Limit Org",
            "slug": format!("bind-{}", uuid::Uuid::new_v4().simple()),
        }))
        .await;
    assert_eq!(res.status_code(), 201, "create org: {}", res.text());
    res.json::<Value>()["id"].as_i64().unwrap() as i32
}

async fn create_tag(server: &axum_test::TestServer, token: &str, name: &str) -> i32 {
    let res = server
        .post("/tags")
        .authorization_bearer(token)
        .json(&json!({ "name": name }))
        .await;
    assert_eq!(res.status_code(), 201, "create tag: {}", res.text());
    res.json::<Value>()["id"].as_i64().unwrap() as i32
}

async fn sql(db: &DatabaseConnection, stmt: String) -> u64 {
    db.execute(Statement::from_string(DbBackend::Postgres, stmt))
        .await
        .expect("raw sql")
        .rows_affected()
}

/// ONE statement that inserts `n` links for `user_id` (in `org_id`, if any)
/// pointing at `https://{host}/bulk/{n}`. Codes are random; every other NOT NULL
/// column has a default.
async fn insert_bulk_links(
    db: &DatabaseConnection,
    user_id: i32,
    org_id: Option<i32>,
    host: &str,
    n: u32,
) {
    let org = org_id.map_or("NULL".to_string(), |o| o.to_string());
    let inserted = sql(
        db,
        format!(
            "INSERT INTO links (code, original_url, user_id, org_id) \
             SELECT 'bulk' || substr(md5(gen_random_uuid()::text), 1, 20), \
                    'https://{host}/bulk/' || g, {user_id}, {org} \
             FROM generate_series(1, {n}) AS g"
        ),
    )
    .await;
    assert_eq!(inserted, n as u64, "bulk insert size");
}

async fn insert_link(db: &DatabaseConnection, user_id: i32, org_id: Option<i32>, url: &str) -> i32 {
    links::ActiveModel {
        code: Set(format!("lk{}", uuid::Uuid::new_v4().simple())),
        original_url: Set(url.to_string()),
        user_id: Set(Some(user_id)),
        org_id: Set(org_id),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert link")
    .id
}

/// A personal user with `OVER_THE_LIMIT` live links that all carry one personal
/// tag. Returns (token, user_id, tag_id).
async fn user_with_tag_on_every_link(
    server: &axum_test::TestServer,
    db: &DatabaseConnection,
) -> (String, i32, i32) {
    let (token, user_id) = register_verified(server, db).await;
    let tag_id = create_tag(server, &token, "bind-limit-tag").await;
    insert_bulk_links(db, user_id, None, "iana.org", OVER_THE_LIMIT).await;
    let tagged = sql(
        db,
        format!(
            "INSERT INTO link_tags (link_id, tag_id) \
             SELECT id, {tag_id} FROM links WHERE user_id = {user_id}"
        ),
    )
    .await;
    assert_eq!(tagged, OVER_THE_LIMIT as u64);
    (token, user_id, tag_id)
}

// ---------------------------------------------------------- organizations

#[tokio::test]
async fn deleting_an_org_with_more_links_than_one_statement_can_bind() {
    let (server, db) = spawn_real_app().await;
    let (token, owner_id) = register_verified(&server, &db).await;
    let org_id = create_org(&server, &token).await;
    let other_org_id = create_org(&server, &token).await;

    insert_bulk_links(&db, owner_id, Some(org_id), "iana.org", OVER_THE_LIMIT).await;
    let survivor = insert_link(&db, owner_id, Some(other_org_id), "https://iana.org/keep").await;

    // The purge must still take click history and tag assignments with the links.
    let doomed: Vec<i32> = links::Entity::find()
        .filter(links::Column::OrgId.eq(org_id))
        .paginate(&db, 3)
        .fetch_page(0)
        .await
        .unwrap()
        .into_iter()
        .map(|l| l.id)
        .collect();
    let org_tag = tags::ActiveModel {
        name: Set("bind-limit-org-tag".to_string()),
        org_id: Set(Some(org_id)),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    for link_id in &doomed {
        click_events::ActiveModel {
            link_id: Set(*link_id),
            created_at: Set(chrono::Utc::now().naive_utc()),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        link_tags::ActiveModel {
            link_id: Set(*link_id),
            tag_id: Set(org_tag.id),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
    }

    let res = server
        .delete(&format!("/orgs/{org_id}"))
        .authorization_bearer(&token)
        .await;
    let status = res.status_code();
    let body = res.text();

    // Clean up before asserting, so a failure leaves the shared database small.
    let leftover_links = links::Entity::find()
        .filter(links::Column::OrgId.eq(org_id))
        .count(&db)
        .await
        .unwrap();
    let leftover_clicks = click_events::Entity::find()
        .filter(click_events::Column::LinkId.is_in(doomed.clone()))
        .count(&db)
        .await
        .unwrap();
    let leftover_assignments = link_tags::Entity::find()
        .filter(link_tags::Column::LinkId.is_in(doomed))
        .count(&db)
        .await
        .unwrap();
    let org_row = organizations::Entity::find_by_id(org_id)
        .one(&db)
        .await
        .unwrap();
    let survivor_row = links::Entity::find_by_id(survivor).one(&db).await.unwrap();
    sql(&db, format!("DELETE FROM links WHERE user_id = {owner_id}")).await;

    assert_eq!(status, 204, "DELETE /orgs/{{id}} failed: {body}");
    assert_eq!(leftover_links, 0, "every org link must be purged");
    assert_eq!(leftover_clicks, 0, "click events leave with their links");
    assert_eq!(
        leftover_assignments, 0,
        "tag assignments leave with their links"
    );
    assert!(org_row.is_none(), "the organization row must be gone");
    assert!(
        survivor_row.is_some(),
        "another organization's link must not be touched"
    );
}

// ------------------------------------------------------------------- tags

#[tokio::test]
async fn tag_link_count_for_a_tag_on_more_links_than_one_statement_can_bind() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id, tag_id) = user_with_tag_on_every_link(&server, &db).await;
    // A soft-deleted tagged link must still be left out of the count.
    sql(
        &db,
        format!(
            "UPDATE links SET deleted_at = now() \
             WHERE id = (SELECT min(id) FROM links WHERE user_id = {user_id})"
        ),
    )
    .await;

    let res = server
        .get(&format!("/tags/{tag_id}"))
        .authorization_bearer(&token)
        .await;
    let status = res.status_code();
    let body: Value = res.json();
    sql(&db, format!("DELETE FROM links WHERE user_id = {user_id}")).await;

    assert_eq!(status, 200, "GET /tags/{{id}}: {body}");
    assert_eq!(
        body["link_count"],
        i64::from(OVER_THE_LIMIT) - 1,
        "one of the {OVER_THE_LIMIT} tagged links is deleted: {body}"
    );
}

#[tokio::test]
async fn listing_the_links_of_a_tag_on_more_links_than_one_statement_can_bind() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id, tag_id) = user_with_tag_on_every_link(&server, &db).await;
    // CONTROL: a live link without the tag must stay out of the listing.
    let untagged = insert_link(&db, user_id, None, "https://iana.org/untagged").await;

    let res = server
        .get(&format!("/tags/{tag_id}/links"))
        .authorization_bearer(&token)
        .await;
    let status = res.status_code();
    let listed = res.json::<Value>();
    let count = listed.as_array().map(|a| a.len());
    let has_untagged = listed
        .as_array()
        .is_some_and(|a| a.iter().any(|l| l["id"].as_i64() == Some(untagged as i64)));
    sql(&db, format!("DELETE FROM links WHERE user_id = {user_id}")).await;

    assert_eq!(status, 200, "GET /tags/{{id}}/links failed");
    assert_eq!(count, Some(OVER_THE_LIMIT as usize));
    assert!(
        !has_untagged,
        "an untagged link leaked into the tag listing"
    );
}

#[tokio::test]
async fn filtering_links_by_a_tag_on_more_links_than_one_statement_can_bind() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id, tag_id) = user_with_tag_on_every_link(&server, &db).await;
    // CONTROL: the newest link is untagged, so it would head an unfiltered list.
    let untagged = insert_link(&db, user_id, None, "https://iana.org/untagged").await;

    let res = server
        .get(&format!("/links?tag_id={tag_id}&limit=5"))
        .authorization_bearer(&token)
        .await;
    let status = res.status_code();
    let body: Value = res.json();
    sql(&db, format!("DELETE FROM links WHERE user_id = {user_id}")).await;

    assert_eq!(status, 200);
    let listed = body.as_array().expect("array of links");
    assert_eq!(listed.len(), 5, "a full page of tagged links, got {body}");
    for link in listed {
        assert_ne!(link["id"].as_i64(), Some(untagged as i64));
        assert!(
            link["tags"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t["id"].as_i64() == Some(tag_id as i64)),
            "every listed link carries the tag: {link}"
        );
    }
}

// ------------------------------------------------------------------ admin

#[tokio::test]
async fn blocking_a_domain_that_covers_more_links_than_one_statement_can_bind() {
    let (server, db) = spawn_real_app().await;
    let (admin_token, _) = register_admin(&server, &db).await;
    let (_, owner_id) = register_verified(&server, &db).await;
    let domain = format!("bind-{}.org", uuid::Uuid::new_v4().simple());

    insert_bulk_links(&db, owner_id, None, &domain, OVER_THE_LIMIT).await;
    // The domain's subdomains go with it, lookalike hosts do not.
    let subdomain = insert_link(&db, owner_id, None, &format!("https://www.{domain}/a")).await;
    let lookalike = insert_link(&db, owner_id, None, &format!("https://not{domain}/b")).await;
    let parent_of_blocked =
        insert_link(&db, owner_id, None, &format!("https://{domain}.test.org/c")).await;
    let unrelated = insert_link(&db, owner_id, None, "https://iana.org/unrelated").await;

    let res = server
        .post("/admin/blocked/domains")
        .authorization_bearer(&admin_token)
        .json(&json!({ "domain": domain }))
        .await;
    let status = res.status_code();
    let body: Value = res.json();

    let live = |id: i32| {
        let db = db.clone();
        async move {
            links::Entity::find_by_id(id)
                .one(&db)
                .await
                .unwrap()
                .expect("link row")
                .deleted_at
                .is_none()
        }
    };
    let still_live = links::Entity::find()
        .filter(links::Column::UserId.eq(owner_id))
        .filter(links::Column::DeletedAt.is_null())
        .count(&db)
        .await
        .unwrap();
    let subdomain_live = live(subdomain).await;
    let lookalike_live = live(lookalike).await;
    let parent_live = live(parent_of_blocked).await;
    let unrelated_live = live(unrelated).await;
    let block_row = blocked_domains::Entity::find()
        .filter(blocked_domains::Column::Domain.eq(domain.clone()))
        .one(&db)
        .await
        .unwrap();

    sql(&db, format!("DELETE FROM links WHERE user_id = {owner_id}")).await;
    sql(
        &db,
        format!("DELETE FROM blocked_domains WHERE domain = '{domain}'"),
    )
    .await;

    assert_eq!(status, 201, "POST /admin/blocked/domains failed: {body}");
    assert_eq!(
        body["affected_links"],
        u64::from(OVER_THE_LIMIT) + 1,
        "every link on the domain plus the subdomain link: {body}"
    );
    assert!(block_row.is_some(), "the block itself must be stored");
    assert!(!subdomain_live, "a subdomain link is taken down");
    assert!(lookalike_live && parent_live && unrelated_live);
    assert_eq!(still_live, 3, "only the three unrelated links stay live");
}

/// Applying an email-domain block reads and updates users in batches. This seeds
/// a few batches' worth of users, with non-matching addresses interleaved so a
/// batch boundary falls inside the matches. (65,536 matching users also works,
/// but deleting that many afterwards takes minutes: `users.disabled_by` has no
/// index for the foreign key's `ON DELETE SET NULL`.)
#[tokio::test]
async fn blocking_an_email_domain_disables_matching_users_across_batches() {
    let (server, db) = spawn_real_app().await;
    let (admin_token, _) = register_admin(&server, &db).await;
    let domain = format!("mail-{}.org", uuid::Uuid::new_v4().simple());
    let seeded: u64 = 2_500;
    let lookalikes = seeded / 10;

    sql(
        &db,
        format!(
            "INSERT INTO users (email, password_hash) \
             SELECT CASE WHEN g % 10 = 0 THEN 'lookalike' || g || '@not{domain}' \
                         ELSE 'bulk' || g || '@{domain}' END, 'x' \
             FROM generate_series(1, {seeded}) AS g"
        ),
    )
    .await;
    // Controls: a subdomain address goes; an admin on the domain stays.
    sql(
        &db,
        format!(
            "INSERT INTO users (email, password_hash, is_admin) VALUES \
             ('sub@www.{domain}', 'x', FALSE), \
             ('boss@{domain}', 'x', TRUE), \
             ('other@{domain}.test.org', 'x', FALSE)"
        ),
    )
    .await;

    let res = server
        .post("/admin/blocked/email-domains")
        .authorization_bearer(&admin_token)
        .json(&json!({ "domain": domain }))
        .await;
    let status = res.status_code();
    let body: Value = res.json();

    let disabled: Vec<String> = users::Entity::find()
        .filter(users::Column::Email.like(format!("%{domain}%")))
        .filter(users::Column::DisabledAt.is_not_null())
        .all(&db)
        .await
        .unwrap()
        .into_iter()
        .map(|u| u.email)
        .collect();
    let mut enabled: Vec<String> = users::Entity::find()
        .filter(users::Column::Email.like(format!("%{domain}%")))
        .filter(users::Column::DisabledAt.is_null())
        .all(&db)
        .await
        .unwrap()
        .into_iter()
        .map(|u| u.email)
        .collect();
    let block_row = blocked_email_domains::Entity::find()
        .filter(blocked_email_domains::Column::Domain.eq(domain.clone()))
        .one(&db)
        .await
        .unwrap();

    sql(
        &db,
        format!("DELETE FROM users WHERE email LIKE '%{domain}%'"),
    )
    .await;
    sql(
        &db,
        format!("DELETE FROM blocked_email_domains WHERE domain = '{domain}'"),
    )
    .await;

    assert_eq!(status, 201, "POST /admin/blocked/email-domains: {body}");
    let expected = (seeded - lookalikes) + 1; // plus the subdomain address
    assert_eq!(body["affected_users"], expected, "{body}");
    assert!(block_row.is_some(), "the block itself must be stored");
    assert_eq!(disabled.len() as u64, expected);
    assert!(disabled.contains(&format!("sub@www.{domain}")));
    enabled.retain(|email| !email.starts_with("lookalike"));
    enabled.sort();
    assert_eq!(
        enabled,
        vec![format!("boss@{domain}"), format!("other@{domain}.test.org")],
        "the admin and the unrelated host stay enabled"
    );
}
