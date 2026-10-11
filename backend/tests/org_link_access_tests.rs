//! Who may act on an organization's links.
//!
//! An organization link belongs to the organization: what a caller may do is
//! decided by their CURRENT membership and role, never by having created the
//! link. Personal links stay with their creator. Real router, real Postgres.
//!
//! | action | personal link | organization link |
//! |--------|---------------|-------------------|
//! | read (the link itself, stats, realtime, QR, rules, lists, export, sparklines, `/ws`, `/sse`) | creator | any member |
//! | write (update, pin, rules, tags, clone, bulk update) | creator | editor and above |
//! | delete (single and bulk) | creator | admin and above, or the creator while still an editor |

mod common;

use common::{
    mark_email_verified, spawn_real_app, spawn_real_app_ws_with_interval, unique_code, unique_email,
};
use opn_onl_backend::entity::{links, org_members, routing_rules, users};
use opn_onl_backend::handlers::websocket::{ClickEvent, WsState};
use opn_onl_backend::utils::create_jwt;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter,
};
use serde_json::{Value, json};
use std::time::Duration;

// ---------------------------------------------------------------- fixtures

/// A fresh app for one request. Every app has its own rate limiters, and the
/// mock transport puts all callers in one per-second bucket, so a test that
/// makes dozens of calls takes a new app for each rather than tripping it.
async fn app() -> axum_test::TestServer {
    spawn_real_app().await.0
}

struct Person {
    id: i32,
    token: String,
}

/// A verified user and a valid JWT, inserted directly (no HTTP, no bcrypt).
async fn person(db: &DatabaseConnection) -> Person {
    let email = unique_email();
    let user = users::ActiveModel {
        email: Set(email.clone()),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(chrono::Utc::now().naive_utc()),
        email_verified: Set(true),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert user");
    let token = create_jwt(user.id, &email, user.token_version).expect("mint token");
    Person { id: user.id, token }
}

async fn add_member(db: &DatabaseConnection, org_id: i32, user_id: i32, role: &str) -> i32 {
    org_members::ActiveModel {
        org_id: Set(org_id),
        user_id: Set(user_id),
        role: Set(role.to_string()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("add org member")
    .id
}

async fn insert_link(db: &DatabaseConnection, creator: i32, org_id: Option<i32>) -> links::Model {
    links::ActiveModel {
        code: Set(unique_code()),
        original_url: Set("https://iana.org/original".to_string()),
        user_id: Set(Some(creator)),
        org_id: Set(org_id),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert link")
}

async fn row(db: &DatabaseConnection, id: i32) -> links::Model {
    links::Entity::find_by_id(id)
        .one(db)
        .await
        .unwrap()
        .expect("link row exists")
}

/// The roles that matter, one person each, plus an organization link created
/// by `editor` and a personal link of theirs.
struct Team {
    db: DatabaseConnection,
    org_id: i32,
    owner: Person,
    admin: Person,
    editor: Person,
    other_editor: Person,
    viewer: Person,
    outsider: Person,
    editor_member_row: i32,
    /// Organization link created by `editor`.
    link: links::Model,
    /// Personal link of `editor`.
    personal: links::Model,
}

impl Team {
    async fn new() -> Team {
        let (setup, db) = spawn_real_app().await;
        let owner = person(&db).await;
        let admin = person(&db).await;
        let editor = person(&db).await;
        let other_editor = person(&db).await;
        let viewer = person(&db).await;
        let outsider = person(&db).await;

        let res = setup
            .post("/orgs")
            .authorization_bearer(&owner.token)
            .json(&json!({
                "name": "Link Access Org",
                "slug": format!("access-{}", uuid::Uuid::new_v4().simple()),
            }))
            .await;
        assert_eq!(res.status_code(), 201, "create org: {}", res.text());
        let org_id = res.json::<Value>()["id"].as_i64().unwrap() as i32;

        add_member(&db, org_id, admin.id, "admin").await;
        let editor_member_row = add_member(&db, org_id, editor.id, "editor").await;
        add_member(&db, org_id, other_editor.id, "editor").await;
        add_member(&db, org_id, viewer.id, "viewer").await;

        let link = insert_link(&db, editor.id, Some(org_id)).await;
        let personal = insert_link(&db, editor.id, None).await;
        Team {
            db,
            org_id,
            owner,
            admin,
            editor,
            other_editor,
            viewer,
            outsider,
            editor_member_row,
            link,
            personal,
        }
    }

    /// Another organization link created by `editor`.
    async fn another_link(&self) -> links::Model {
        insert_link(&self.db, self.editor.id, Some(self.org_id)).await
    }

    /// The owner removes `editor` through the real endpoint.
    async fn remove_editor(&self) {
        let res = app()
            .await
            .delete(&format!(
                "/orgs/{}/members/{}",
                self.org_id, self.editor_member_row
            ))
            .authorization_bearer(&self.owner.token)
            .await;
        assert_eq!(res.status_code(), 204, "remove member: {}", res.text());
    }

    /// The owner demotes `editor` to viewer through the real endpoint.
    async fn demote_editor(&self) {
        let res = app()
            .await
            .put(&format!(
                "/orgs/{}/members/{}",
                self.org_id, self.editor_member_row
            ))
            .authorization_bearer(&self.owner.token)
            .json(&json!({ "role": "viewer" }))
            .await;
        assert_eq!(res.status_code(), 200, "demote member: {}", res.text());
    }
}

// ----------------------------------------------------------------- requests

async fn get(token: &str, path: &str) -> axum_test::TestResponse {
    app().await.get(path).authorization_bearer(token).await
}

async fn get_status(token: &str, path: &str) -> u16 {
    get(token, path).await.status_code().as_u16()
}

async fn send(
    method: &str,
    token: &str,
    path: &str,
    body: Option<Value>,
) -> axum_test::TestResponse {
    let server = app().await;
    let request = match method {
        "PUT" => server.put(path),
        "DELETE" => server.delete(path),
        _ => server.post(path),
    }
    .authorization_bearer(token);
    match body {
        Some(body) => request.json(&body).await,
        None => request.await,
    }
}

async fn status(method: &str, token: &str, path: &str, body: Option<Value>) -> u16 {
    send(method, token, path, body).await.status_code().as_u16()
}

async fn listed_ids(token: &str, query: &[(&str, String)]) -> Vec<i64> {
    let server = app().await;
    let mut request = server.get("/links").authorization_bearer(token);
    for (key, value) in query {
        request = request.add_query_param(key, value);
    }
    let res = request.await;
    assert_eq!(res.status_code(), 200, "list: {}", res.text());
    res.json::<Value>()
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|l| l["id"].as_i64())
        .collect()
}

async fn org_list(token: &str, org_id: i32) -> Vec<i64> {
    listed_ids(token, &[("org_id", org_id.to_string())]).await
}

async fn own_list(token: &str) -> Vec<i64> {
    listed_ids(token, &[]).await
}

async fn export_csv(token: &str, org_id: Option<i32>) -> String {
    let server = app().await;
    let mut request = server.get("/links/export").authorization_bearer(token);
    if let Some(org_id) = org_id {
        request = request.add_query_param("org_id", org_id);
    }
    let res = request.await;
    assert_eq!(res.status_code(), 200, "export: {}", res.text());
    res.text()
}

async fn sparkline_ids(token: &str, ids: &[i32]) -> Vec<i64> {
    let joined = ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let res = app()
        .await
        .get("/links/sparklines")
        .add_query_param("ids", joined)
        .authorization_bearer(token)
        .await;
    assert_eq!(res.status_code(), 200, "sparklines: {}", res.text());
    res.json::<Value>()["sparklines"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["link_id"].as_i64())
        .collect()
}

async fn dashboard_total_links(token: &str) -> i64 {
    let res = get(token, "/analytics/dashboard").await;
    assert_eq!(res.status_code(), 200, "dashboard: {}", res.text());
    res.json::<Value>()["total_links"].as_i64().unwrap()
}

async fn bulk_update(token: &str, ids: &[i32]) -> i64 {
    let res = send(
        "POST",
        token,
        "/links/bulk/update",
        Some(json!({ "ids": ids, "expires_at": "2099-01-01T00:00:00Z" })),
    )
    .await;
    assert_eq!(res.status_code(), 200, "bulk update: {}", res.text());
    res.json::<Value>()["updated"].as_i64().unwrap()
}

async fn bulk_delete(token: &str, ids: &[i32]) -> i64 {
    let res = send(
        "POST",
        token,
        "/links/bulk/delete",
        Some(json!({ "ids": ids })),
    )
    .await;
    assert_eq!(res.status_code(), 200, "bulk delete: {}", res.text());
    res.json::<Value>()["deleted"].as_i64().unwrap()
}

/// Everything a caller who may only READ an organization link can still do.
async fn assert_can_read(who: &str, token: &str, team: &Team) {
    let id = team.link.id;
    for path in [
        format!("/links/{id}"),
        format!("/links/{id}/stats"),
        format!("/links/{id}/clicks/realtime"),
        format!("/links/{id}/qr"),
        format!("/links/{id}/rules"),
    ] {
        assert_eq!(get_status(token, &path).await, 200, "{who}: GET {path}");
    }
    assert!(
        org_list(token, team.org_id).await.contains(&(id as i64)),
        "{who}: GET /links?org_id= lists the org link"
    );
    assert!(
        sparkline_ids(token, &[id]).await.contains(&(id as i64)),
        "{who}: sparklines include the org link"
    );
    assert!(
        export_csv(token, Some(team.org_id))
            .await
            .contains(&team.link.code),
        "{who}: the org export includes the org link"
    );
}

/// Nothing readable: every read is refused, or comes back without the link.
async fn assert_cannot_read(who: &str, token: &str, team: &Team) {
    let id = team.link.id;
    for path in [
        format!("/links/{id}"),
        format!("/links/{id}/stats"),
        format!("/links/{id}/clicks/realtime"),
        format!("/links/{id}/qr"),
        format!("/links/{id}/rules"),
    ] {
        assert_eq!(get_status(token, &path).await, 403, "{who}: GET {path}");
    }
    assert!(
        !org_list(token, team.org_id).await.contains(&(id as i64)),
        "{who}: GET /links?org_id= must not list the org link"
    );
    assert!(
        !own_list(token).await.contains(&(id as i64)),
        "{who}: GET /links must not list the org link"
    );
    assert!(
        !sparkline_ids(token, &[id]).await.contains(&(id as i64)),
        "{who}: sparklines must omit the org link"
    );
    assert!(
        !export_csv(token, Some(team.org_id))
            .await
            .contains(&team.link.code),
        "{who}: the org export must omit the org link"
    );
    assert!(
        !export_csv(token, None).await.contains(&team.link.code),
        "{who}: the personal export must omit the org link"
    );
}

/// Every write refused, and the row, its rules and the org's link count intact.
async fn assert_cannot_write(who: &str, token: &str, team: &Team) {
    let id = team.link.id;
    let before = row(&team.db, id).await;
    let links_in_org = || async {
        links::Entity::find()
            .filter(links::Column::OrgId.eq(team.org_id))
            .count(&team.db)
            .await
            .unwrap()
    };
    let count_before = links_in_org().await;

    let tag = json!({ "tag_ids": [1] });
    let rules = json!({ "rules": [{ "destination_url": "https://iana.org/evil" }] });
    for (method, path, body) in [
        (
            "PUT",
            format!("/links/{id}"),
            Some(json!({ "original_url": "https://iana.org/hijacked", "title": "hijacked" })),
        ),
        ("POST", format!("/links/{id}/pin"), None),
        ("PUT", format!("/links/{id}/rules"), Some(rules)),
        ("POST", format!("/links/{id}/tags"), Some(tag.clone())),
        ("DELETE", format!("/links/{id}/tags"), Some(tag)),
        ("POST", format!("/links/{id}/clone"), None),
    ] {
        assert_eq!(
            status(method, token, &path, body).await,
            403,
            "{who}: {method} {path}"
        );
    }
    assert_eq!(bulk_update(token, &[id]).await, 0, "{who}: bulk update");

    let after = row(&team.db, id).await;
    assert_eq!(after, before, "{who}: the link row must be untouched");
    assert_eq!(
        routing_rules::Entity::find()
            .filter(routing_rules::Column::LinkId.eq(id))
            .count(&team.db)
            .await
            .unwrap(),
        0,
        "{who}: no routing rule may be written"
    );
    assert_eq!(
        links_in_org().await,
        count_before,
        "{who}: no link may be cloned into the org"
    );
}

async fn assert_cannot_delete(who: &str, token: &str, team: &Team) {
    let id = team.link.id;
    assert_eq!(
        status("DELETE", token, &format!("/links/{id}"), None).await,
        403,
        "{who}: DELETE /links/{id}"
    );
    assert_eq!(bulk_delete(token, &[id]).await, 0, "{who}: bulk delete");
    assert!(
        row(&team.db, id).await.deleted_at.is_none(),
        "{who}: the link must still be live"
    );
}

// -------------------------------------------------------------------- roles

#[tokio::test]
async fn viewer_reads_org_link_but_cannot_change_it() {
    let team = Team::new().await;
    assert_can_read("viewer", &team.viewer.token, &team).await;
    assert_cannot_write("viewer", &team.viewer.token, &team).await;
    assert_cannot_delete("viewer", &team.viewer.token, &team).await;
}

#[tokio::test]
async fn editor_can_edit_a_link_another_member_created_but_not_delete_it() {
    let team = Team::new().await;
    let who = &team.other_editor;
    let id = team.link.id;
    assert_can_read("editor", &who.token, &team).await;

    let res = send(
        "PUT",
        &who.token,
        &format!("/links/{id}"),
        Some(json!({ "title": "edited by a colleague" })),
    )
    .await;
    assert_eq!(res.status_code(), 200, "update: {}", res.text());
    assert_eq!(
        row(&team.db, id).await.title.as_deref(),
        Some("edited by a colleague")
    );

    assert_eq!(
        status("POST", &who.token, &format!("/links/{id}/pin"), None).await,
        200
    );
    assert!(row(&team.db, id).await.is_pinned);

    let rules = json!({ "rules": [{ "destination_url": "https://iana.org/routed" }] });
    assert_eq!(
        status(
            "PUT",
            &who.token,
            &format!("/links/{id}/rules"),
            Some(rules)
        )
        .await,
        200
    );

    let tag = send(
        "POST",
        &who.token,
        "/tags",
        Some(json!({ "name": "shared", "org_id": team.org_id })),
    )
    .await;
    assert_eq!(tag.status_code(), 201, "create tag: {}", tag.text());
    let tag_id = tag.json::<Value>()["id"].as_i64().unwrap();
    assert_eq!(
        status(
            "POST",
            &who.token,
            &format!("/links/{id}/tags"),
            Some(json!({ "tag_ids": [tag_id] }))
        )
        .await,
        200
    );

    assert_eq!(bulk_update(&who.token, &[id]).await, 1);
    assert!(row(&team.db, id).await.expires_at.is_some());

    // The clone lands in the same organization, owned by whoever cloned it.
    let clone = send("POST", &who.token, &format!("/links/{id}/clone"), None).await;
    assert_eq!(clone.status_code(), 201, "clone: {}", clone.text());
    let cloned = row(
        &team.db,
        clone.json::<Value>()["id"].as_i64().unwrap() as i32,
    )
    .await;
    assert_eq!(cloned.org_id, Some(team.org_id));
    assert_eq!(cloned.user_id, Some(who.id));

    assert_cannot_delete("editor", &who.token, &team).await;
}

#[tokio::test]
async fn admin_and_owner_can_delete_a_link_another_member_created() {
    let team = Team::new().await;

    let by_admin = team.link.id;
    assert_eq!(
        status(
            "DELETE",
            &team.admin.token,
            &format!("/links/{by_admin}"),
            None
        )
        .await,
        200
    );
    assert!(row(&team.db, by_admin).await.deleted_at.is_some());

    let by_owner = team.another_link().await.id;
    assert_eq!(
        status(
            "DELETE",
            &team.owner.token,
            &format!("/links/{by_owner}"),
            None
        )
        .await,
        200
    );
    assert!(row(&team.db, by_owner).await.deleted_at.is_some());

    let bulk_a = team.another_link().await.id;
    let bulk_b = team.another_link().await.id;
    assert_eq!(bulk_delete(&team.admin.token, &[bulk_a, bulk_b]).await, 2);
    assert!(row(&team.db, bulk_a).await.deleted_at.is_some());
    assert!(row(&team.db, bulk_b).await.deleted_at.is_some());
}

#[tokio::test]
async fn creator_who_is_still_an_editor_can_delete_their_own_org_link() {
    let team = Team::new().await;
    let single = team.link.id;
    assert_eq!(
        status(
            "DELETE",
            &team.editor.token,
            &format!("/links/{single}"),
            None
        )
        .await,
        200
    );
    assert!(row(&team.db, single).await.deleted_at.is_some());

    let bulk = team.another_link().await.id;
    assert_eq!(bulk_delete(&team.editor.token, &[bulk]).await, 1);
    assert!(row(&team.db, bulk).await.deleted_at.is_some());
}

#[tokio::test]
async fn demoted_creator_keeps_read_access_and_loses_write_and_delete() {
    let team = Team::new().await;
    team.demote_editor().await;
    assert_can_read("demoted creator", &team.editor.token, &team).await;
    assert_cannot_write("demoted creator", &team.editor.token, &team).await;
    assert_cannot_delete("demoted creator", &team.editor.token, &team).await;
}

// -------------------------------------------------------- former and non-members

#[tokio::test]
async fn removed_member_loses_every_kind_of_access_to_links_they_created() {
    let team = Team::new().await;

    // Control: before removal the creator lists the org link, with their own
    // personal link, and it counts in their dashboard.
    let own = own_list(&team.editor.token).await;
    assert!(own.contains(&(team.link.id as i64)));
    assert!(own.contains(&(team.personal.id as i64)));
    assert_eq!(dashboard_total_links(&team.editor.token).await, 2);

    team.remove_editor().await;

    let token = &team.editor.token;
    assert_cannot_read("removed member", token, &team).await;
    assert_cannot_write("removed member", token, &team).await;
    assert_cannot_delete("removed member", token, &team).await;

    // Their own account view keeps only what is still theirs.
    let own = own_list(token).await;
    assert!(own.contains(&(team.personal.id as i64)));
    assert!(!own.contains(&(team.link.id as i64)));
    assert_eq!(dashboard_total_links(token).await, 1);
    assert!(
        export_csv(token, None).await.contains(&team.personal.code),
        "the personal link is still exported"
    );
    assert_eq!(
        status(
            "PUT",
            token,
            &format!("/links/{}", team.personal.id),
            Some(json!({ "title": "still mine" }))
        )
        .await,
        200,
        "a personal link stays fully editable"
    );

    // The organization is unaffected: its owner still manages the link.
    assert_eq!(
        status(
            "PUT",
            &team.owner.token,
            &format!("/links/{}", team.link.id),
            Some(json!({ "title": "owner keeps control" }))
        )
        .await,
        200
    );
}

#[tokio::test]
async fn someone_who_never_belonged_to_the_org_gets_nothing() {
    let team = Team::new().await;
    let token = &team.outsider.token;
    assert_cannot_read("outsider", token, &team).await;
    assert_cannot_write("outsider", token, &team).await;
    assert_cannot_delete("outsider", token, &team).await;
    assert!(
        org_list(token, team.org_id).await.is_empty(),
        "a non-member listing an org gets an empty list, not an error that reveals the org"
    );
}

#[tokio::test]
async fn personal_links_belong_to_their_creator_alone() {
    let team = Team::new().await;
    let id = team.personal.id;

    // Organization standing says nothing about a link that is not the org's.
    for (who, token) in [
        ("org owner", &team.owner.token),
        ("org admin", &team.admin.token),
        ("fellow editor", &team.other_editor.token),
        ("outsider", &team.outsider.token),
    ] {
        for path in [
            format!("/links/{id}"),
            format!("/links/{id}/stats"),
            format!("/links/{id}/clicks/realtime"),
            format!("/links/{id}/qr"),
            format!("/links/{id}/rules"),
        ] {
            assert_eq!(get_status(token, &path).await, 403, "{who}: GET {path}");
        }
        assert_eq!(
            status(
                "PUT",
                token,
                &format!("/links/{id}"),
                Some(json!({ "title": "x" }))
            )
            .await,
            403,
            "{who}: update"
        );
        assert_eq!(
            status("POST", token, &format!("/links/{id}/pin"), None).await,
            403,
            "{who}: pin"
        );
        assert_eq!(
            status("POST", token, &format!("/links/{id}/clone"), None).await,
            403,
            "{who}: clone"
        );
        assert_eq!(
            status("DELETE", token, &format!("/links/{id}"), None).await,
            403,
            "{who}: delete"
        );
        assert_eq!(bulk_update(token, &[id]).await, 0, "{who}: bulk update");
        assert_eq!(bulk_delete(token, &[id]).await, 0, "{who}: bulk delete");
        assert!(
            !sparkline_ids(token, &[id]).await.contains(&(id as i64)),
            "{who}: sparklines"
        );
    }
    let after = row(&team.db, id).await;
    assert_eq!(
        after, team.personal,
        "nobody else touched the personal link"
    );

    // Control: the creator can do all of it.
    let token = &team.editor.token;
    assert_eq!(get_status(token, &format!("/links/{id}")).await, 200);
    assert_eq!(get_status(token, &format!("/links/{id}/stats")).await, 200);
    assert_eq!(
        status(
            "PUT",
            token,
            &format!("/links/{id}"),
            Some(json!({ "title": "mine" }))
        )
        .await,
        200
    );
    assert_eq!(
        status("POST", token, &format!("/links/{id}/clone"), None).await,
        201
    );
    assert_eq!(
        status("DELETE", token, &format!("/links/{id}"), None).await,
        200
    );
}

// ------------------------------------------------------------------- listings

#[tokio::test]
async fn members_list_and_export_every_link_of_the_org_but_own_view_stays_personal() {
    let team = Team::new().await;
    let by_admin = insert_link(&team.db, team.admin.id, Some(team.org_id)).await;
    let ids = [team.link.id as i64, by_admin.id as i64];

    for (who, token) in [
        ("owner", &team.owner.token),
        ("viewer", &team.viewer.token),
        ("fellow editor", &team.other_editor.token),
    ] {
        let listed = org_list(token, team.org_id).await;
        for id in ids {
            assert!(listed.contains(&id), "{who}: GET /links?org_id= lists {id}");
        }
        let csv = export_csv(token, Some(team.org_id)).await;
        assert!(csv.contains(&team.link.code), "{who}: export");
        assert!(csv.contains(&by_admin.code), "{who}: export");
        assert_eq!(
            sparkline_ids(token, &[team.link.id, by_admin.id])
                .await
                .len(),
            2,
            "{who}: sparklines"
        );
    }

    // Without org_id the list is still the caller's own links.
    let owner_own = own_list(&team.owner.token).await;
    for id in ids {
        assert!(!owner_own.contains(&id), "owner created neither link");
    }
    assert!(
        !export_csv(&team.owner.token, None)
            .await
            .contains(&team.link.code)
    );
    let admin_own = own_list(&team.admin.token).await;
    assert!(admin_own.contains(&(by_admin.id as i64)));
    assert!(!admin_own.contains(&(team.link.id as i64)));

    // Narrowing by org still excludes personal links.
    let listed = org_list(&team.editor.token, team.org_id).await;
    assert!(!listed.contains(&(team.personal.id as i64)));
}

#[tokio::test]
async fn deleted_org_links_drop_out_of_the_org_list() {
    let team = Team::new().await;
    assert_eq!(
        status(
            "DELETE",
            &team.admin.token,
            &format!("/links/{}", team.link.id),
            None
        )
        .await,
        200
    );
    assert!(
        !org_list(&team.owner.token, team.org_id)
            .await
            .contains(&(team.link.id as i64))
    );
}

// ----------------------------------------------------------------------- bulk

#[tokio::test]
async fn bulk_update_changes_and_counts_only_the_links_the_caller_may_edit() {
    let team = Team::new().await;
    let theirs = team.another_link().await;

    // A second organization where the caller is only a viewer.
    let other_org = {
        let res = app()
            .await
            .post("/orgs")
            .authorization_bearer(&team.outsider.token)
            .json(&json!({
                "name": "Elsewhere",
                "slug": format!("elsewhere-{}", uuid::Uuid::new_v4().simple()),
            }))
            .await;
        assert_eq!(res.status_code(), 201, "create org: {}", res.text());
        res.json::<Value>()["id"].as_i64().unwrap() as i32
    };
    add_member(&team.db, other_org, team.other_editor.id, "viewer").await;
    let read_only = insert_link(&team.db, team.outsider.id, Some(other_org)).await;
    let strangers_personal = insert_link(&team.db, team.outsider.id, None).await;
    let own_personal = insert_link(&team.db, team.other_editor.id, None).await;

    let ids = [
        team.link.id,          // org link, caller is an editor: changed
        theirs.id,             // org link, caller is an editor: changed
        theirs.id,             // the same id twice: counted once
        read_only.id,          // org link, caller is a viewer: skipped
        strangers_personal.id, // someone else's personal link: skipped
        own_personal.id,       // caller's personal link: changed
        i32::MAX,              // does not exist: skipped
    ];
    let updated = bulk_update(&team.other_editor.token, &ids).await;
    assert_eq!(updated, 3, "count equals the rows actually changed");

    for id in [team.link.id, theirs.id, own_personal.id] {
        assert!(row(&team.db, id).await.expires_at.is_some(), "{id} changed");
    }
    for id in [read_only.id, strangers_personal.id] {
        assert!(
            row(&team.db, id).await.expires_at.is_none(),
            "{id} untouched"
        );
    }
}

#[tokio::test]
async fn bulk_delete_removes_and_counts_only_the_links_the_caller_may_delete() {
    let team = Team::new().await;
    // `editor` may delete what they created, but not what a colleague did.
    let mine = team.link.id;
    let colleagues = insert_link(&team.db, team.admin.id, Some(team.org_id)).await;
    let own_personal = team.personal.id;
    let strangers = insert_link(&team.db, team.outsider.id, None).await;

    let ids = [
        mine,
        mine,
        colleagues.id,
        own_personal,
        strangers.id,
        i32::MAX,
    ];
    let deleted = bulk_delete(&team.editor.token, &ids).await;
    assert_eq!(deleted, 2, "count equals the rows actually deleted");

    assert!(row(&team.db, mine).await.deleted_at.is_some());
    assert!(row(&team.db, own_personal).await.deleted_at.is_some());
    assert!(row(&team.db, colleagues.id).await.deleted_at.is_none());
    assert!(row(&team.db, strangers.id).await.deleted_at.is_none());

    // An org admin may delete the colleague's link and the editor's alike, but
    // still not someone else's personal link.
    let editors = team.another_link().await;
    let editors_personal = insert_link(&team.db, team.editor.id, None).await;
    let ids = [colleagues.id, editors.id, editors_personal.id, strangers.id];
    assert_eq!(bulk_delete(&team.admin.token, &ids).await, 2);
    assert!(row(&team.db, colleagues.id).await.deleted_at.is_some());
    assert!(row(&team.db, editors.id).await.deleted_at.is_some());
    assert!(
        row(&team.db, editors_personal.id)
            .await
            .deleted_at
            .is_none()
    );
    assert!(row(&team.db, strangers.id).await.deleted_at.is_none());
}

// -------------------------------------------------------------------- orphans

/// The creator deletes their account. The organization's link stays live (it
/// is the organization's, not theirs), so the organization must be able to take
/// it down; before this, only an instance admin could.
#[tokio::test]
async fn org_can_take_down_a_link_after_its_creator_deletes_their_account() {
    common::set_env("ENABLE_ACCOUNT_DELETION", "true");
    let team = Team::new().await;

    // The creator needs a real password to delete the account.
    let (setup, db) = spawn_real_app().await;
    let email = unique_email();
    let res = setup
        .post("/auth/register")
        .json(&json!({ "email": email, "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    let creator_id = body["user_id"].as_i64().unwrap() as i32;
    let creator_token = body["token"].as_str().unwrap().to_string();
    mark_email_verified(&db, creator_id).await;
    add_member(&db, team.org_id, creator_id, "editor").await;
    let orphan = insert_link(&db, creator_id, Some(team.org_id)).await;

    let res = app()
        .await
        .post("/auth/delete-account")
        .authorization_bearer(&creator_token)
        .json(&json!({ "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 200, "self-delete: {}", res.text());
    assert!(
        row(&db, orphan.id).await.deleted_at.is_none(),
        "the org link survives its creator"
    );

    assert!(
        org_list(&team.owner.token, team.org_id)
            .await
            .contains(&(orphan.id as i64)),
        "the orphaned link is still listed for the org"
    );
    assert_eq!(
        status(
            "DELETE",
            &team.owner.token,
            &format!("/links/{}", orphan.id),
            None
        )
        .await,
        200
    );
    assert!(row(&db, orphan.id).await.deleted_at.is_some());
}

// -------------------------------------------------------------------- realtime

async fn wait_for_subscribers(ws: &WsState, count: usize) {
    for _ in 0..200 {
        if ws.click_tx.receiver_count() >= count {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("expected {count} subscribers on the click channel");
}

fn click(user_id: i32, org_id: Option<i32>, code: &str) -> ClickEvent {
    ClickEvent {
        link_id: 1,
        link_code: code.to_string(),
        user_id: Some(user_id),
        org_id,
        click_count: 1,
        country: None,
        city: None,
        device: None,
        browser: None,
        timestamp: "2026-10-10T00:00:00Z".to_string(),
    }
}

/// An org with a creator (`editor`) and another member (`owner`), over a real
/// HTTP transport with a 50 ms membership refresh.
struct RealtimeTeam {
    server: axum_test::TestServer,
    db: DatabaseConnection,
    ws: std::sync::Arc<WsState>,
    org_id: i32,
    owner: Person,
    editor: Person,
    editor_member_row: i32,
}

impl RealtimeTeam {
    async fn new() -> RealtimeTeam {
        let (server, db, ws) = spawn_real_app_ws_with_interval(Duration::from_millis(50)).await;
        let owner = person(&db).await;
        let editor = person(&db).await;
        let res = server
            .post("/orgs")
            .authorization_bearer(&owner.token)
            .json(&json!({
                "name": "Realtime Org",
                "slug": format!("realtime-{}", uuid::Uuid::new_v4().simple()),
            }))
            .await;
        assert_eq!(res.status_code(), 201, "create org: {}", res.text());
        let org_id = res.json::<Value>()["id"].as_i64().unwrap() as i32;
        let editor_member_row = add_member(&db, org_id, editor.id, "editor").await;
        RealtimeTeam {
            server,
            db,
            ws,
            org_id,
            owner,
            editor,
            editor_member_row,
        }
    }

    /// Broadcast a click on a link the editor created in the org.
    fn org_click(&self, code: &str) {
        self.ws
            .broadcast_click(click(self.editor.id, Some(self.org_id), code));
    }

    /// Broadcast a click on a personal link of the editor.
    fn personal_click(&self, code: &str) {
        self.ws.broadcast_click(click(self.editor.id, None, code));
    }
}

/// One subscriber to the click stream, over `/ws` or over `/sse`.
enum Subscriber {
    Ws(axum_test::TestWebSocket),
    Sse {
        response: reqwest::Response,
        buffer: String,
    },
}

#[derive(Clone, Copy, Debug)]
enum Transport {
    Ws,
    Sse,
}

impl Subscriber {
    async fn open(transport: Transport, team: &RealtimeTeam, token: &str) -> Subscriber {
        match transport {
            Transport::Ws => Subscriber::Ws(
                team.server
                    .get_websocket("/ws")
                    .add_query_param("token", token)
                    .await
                    .into_websocket()
                    .await,
            ),
            Transport::Sse => {
                let url = team
                    .server
                    .server_address()
                    .expect("HTTP transport address")
                    .join("sse")
                    .unwrap();
                let response = reqwest::Client::new()
                    .get(url)
                    .query(&[("token", token)])
                    .send()
                    .await
                    .expect("open SSE stream");
                assert_eq!(response.status(), reqwest::StatusCode::OK);
                Subscriber::Sse {
                    response,
                    buffer: String::new(),
                }
            }
        }
    }

    /// The link code of the next click event.
    async fn next_code(&mut self) -> String {
        loop {
            let payload = match self {
                Subscriber::Ws(socket) => match socket.receive_message().await {
                    axum_test::WsMessage::Text(text) => text,
                    axum_test::WsMessage::Close(_) => panic!("socket closed unexpectedly"),
                    _ => continue,
                },
                Subscriber::Sse { response, buffer } => loop {
                    if let Some(end) = buffer.find("\n\n") {
                        let event: String = buffer.drain(..end + 2).collect();
                        match event.lines().find_map(|line| line.strip_prefix("data:")) {
                            Some(data) => break data.trim().to_string(),
                            None => continue,
                        }
                    }
                    let chunk = response
                        .chunk()
                        .await
                        .expect("read SSE body")
                        .expect("SSE stream ended unexpectedly");
                    buffer.push_str(&String::from_utf8_lossy(&chunk));
                },
            };
            let value: Value = serde_json::from_str(&payload).unwrap();
            if let Some(code) = value["link_code"].as_str() {
                return code.to_string();
            }
        }
    }

    /// Every link code received up to and including `until`. Events reach a
    /// subscriber in broadcast order, so whatever was addressed to it and
    /// broadcast before `until` is in the result; there is no timing window.
    async fn codes_until(&mut self, until: &str) -> Vec<String> {
        tokio::time::timeout(Duration::from_secs(10), async {
            let mut codes = Vec::new();
            loop {
                let code = self.next_code().await;
                let done = code == until;
                codes.push(code);
                if done {
                    return codes;
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("never received {until}"))
    }
}

/// A click on an org link goes to every current member, not just its creator,
/// and a member who is removed stops receiving it on the already open stream.
async fn org_clicks_follow_membership(transport: Transport) {
    let team = RealtimeTeam::new().await;
    let mut owner = Subscriber::open(transport, &team, &team.owner.token).await;
    let mut editor = Subscriber::open(transport, &team, &team.editor.token).await;
    wait_for_subscribers(&team.ws, 2).await;

    // A click on a link the editor created reaches the owner as well as the
    // editor; a click on a stranger's personal link reaches neither.
    team.org_click("ORG-1");
    team.ws
        .broadcast_click(click(team.editor.id + 100_000, None, "STRANGER"));
    team.personal_click("PERSONAL-1");
    team.org_click("SENTINEL-1");
    assert_eq!(
        owner.codes_until("SENTINEL-1").await,
        ["ORG-1", "SENTINEL-1"],
        "{transport:?}: the owner is a member, not the creator"
    );
    assert_eq!(
        editor.codes_until("SENTINEL-1").await,
        ["ORG-1", "PERSONAL-1", "SENTINEL-1"],
        "{transport:?}"
    );

    // The editor is removed. Probe until the open stream has refreshed its
    // memberships: an org click is followed by a personal one that always
    // arrives, so the org click is either in what came before it or it is not.
    org_members::Entity::delete_by_id(team.editor_member_row)
        .exec(&team.db)
        .await
        .unwrap();
    let mut settled = false;
    for _ in 0..100 {
        team.org_click("PROBE");
        team.personal_click("PROBE-PERSONAL");
        if !editor
            .codes_until("PROBE-PERSONAL")
            .await
            .contains(&"PROBE".to_string())
        {
            settled = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        settled,
        "{transport:?}: the stream kept sending a removed member's org clicks"
    );

    team.org_click("ORG-2");
    team.personal_click("PERSONAL-2");
    assert_eq!(
        editor.codes_until("PERSONAL-2").await,
        ["PERSONAL-2"],
        "{transport:?}: a removed creator must stop receiving the org's clicks"
    );
    let seen = owner.codes_until("ORG-2").await;
    assert_eq!(
        seen.last().map(String::as_str),
        Some("ORG-2"),
        "{transport:?}: the remaining member still receives them"
    );
}

#[tokio::test]
async fn ws_delivers_org_clicks_to_every_member_and_stops_for_a_removed_one() {
    org_clicks_follow_membership(Transport::Ws).await;
}

#[tokio::test]
async fn sse_delivers_org_clicks_to_every_member_and_stops_for_a_removed_one() {
    org_clicks_follow_membership(Transport::Sse).await;
}

/// The event a real visit produces carries the link's organization, so the
/// routing above works on live traffic and not only on hand-built events.
#[tokio::test]
async fn a_visit_to_an_org_link_reaches_the_org_members() {
    let team = RealtimeTeam::new().await;
    let link = insert_link(&team.db, team.editor.id, Some(team.org_id)).await;
    let mut owner = Subscriber::open(Transport::Ws, &team, &team.owner.token).await;
    wait_for_subscribers(&team.ws, 1).await;

    let hit = team.server.get(&format!("/{}", link.code)).await;
    assert!(
        hit.status_code().is_redirection(),
        "visit: {}",
        hit.status_code()
    );

    let codes = owner.codes_until(&link.code).await;
    assert_eq!(codes, [link.code.as_str()]);
}

// ------------------------------------------------------------------ bio page

#[tokio::test]
async fn only_the_creator_decides_whether_an_org_link_is_on_their_bio_page() {
    let team = Team::new().await;
    let path = format!("/links/{}", team.link.id);

    // Another editor may edit the link, but not put it on its creator's page.
    let res = send(
        "PUT",
        &team.other_editor.token,
        &path,
        Some(json!({ "bio_visible": true })),
    )
    .await;
    assert_eq!(
        res.status_code(),
        403,
        "colleague sets bio_visible: {}",
        res.text()
    );
    assert!(!row(&team.db, team.link.id).await.bio_visible);

    // Echoing the current value (as the dashboard does on every save) is fine.
    assert_eq!(
        status(
            "PUT",
            &team.other_editor.token,
            &path,
            Some(json!({ "title": "Renamed", "bio_visible": false })),
        )
        .await,
        200
    );

    // The creator may.
    assert_eq!(
        status(
            "PUT",
            &team.editor.token,
            &path,
            Some(json!({ "bio_visible": true }))
        )
        .await,
        200
    );
    assert!(row(&team.db, team.link.id).await.bio_visible);
}

#[tokio::test]
async fn a_removed_creator_bio_page_drops_the_org_links() {
    let team = Team::new().await;
    let username = format!("bio{}", unique_code().to_lowercase());
    let mut editor: users::ActiveModel = users::Entity::find_by_id(team.editor.id)
        .one(&team.db)
        .await
        .unwrap()
        .unwrap()
        .into();
    editor.bio_username = Set(Some(username.clone()));
    editor.bio_enabled = Set(true);
    editor.update(&team.db).await.expect("enable bio");
    for id in [team.link.id, team.personal.id] {
        let mut link: links::ActiveModel = row(&team.db, id).await.into();
        link.bio_visible = Set(true);
        link.update(&team.db).await.expect("mark bio-visible");
    }

    let bio_codes = || async {
        let res = app().await.get(&format!("/api/bio/{username}")).await;
        assert_eq!(res.status_code(), 200, "bio page: {}", res.text());
        res.json::<Value>()["links"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|l| l["code"].as_str().map(str::to_string))
            .collect::<Vec<_>>()
    };

    let before = bio_codes().await;
    assert!(
        before.contains(&team.link.code),
        "org link listed: {before:?}"
    );
    assert!(before.contains(&team.personal.code));

    team.remove_editor().await;

    let after = bio_codes().await;
    assert!(
        !after.contains(&team.link.code),
        "a former member's bio must not keep listing the org's links: {after:?}"
    );
    assert!(
        after.contains(&team.personal.code),
        "personal links stay: {after:?}"
    );
}
