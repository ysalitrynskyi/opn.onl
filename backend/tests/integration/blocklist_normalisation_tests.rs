//! The blocklists compare canonical spellings, not raw strings.
//!
//! An exact-URL block has to hold against a different spelling of the same page
//! (query, fragment, host case, default port, trailing slash) at create, update,
//! bulk create and the redirect-time re-check; a block of an internationalised
//! domain has to hold whether the domain is typed as Unicode, punycode or in
//! upper case, on both the admin and the sign-up side; and block targets that
//! would take down far more than one site are refused. Real router + real
//! Postgres via `common::spawn_real_app`.

use crate::common;

use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::{blocked_domains, blocked_email_domains, blocked_links, users};
use opn_onl_backend::utils::url_policy;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use serde_json::{Value, json};

/// A fresh router for every call. The per-second, auth and link-creation
/// limiters are per router, and these tests fire many requests in a burst.
async fn app() -> axum_test::TestServer {
    spawn_real_app().await.0
}

struct Actors {
    db: DatabaseConnection,
    admin: String,
    user: String,
}

async fn register(email: &str) -> (String, i32) {
    let res = app()
        .await
        .post("/auth/register")
        .json(&json!({ "email": email, "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register {email}: {}", res.text());
    let body: Value = res.json();
    (
        body["token"].as_str().unwrap().to_string(),
        body["user_id"].as_i64().unwrap() as i32,
    )
}

/// An admin and a verified ordinary user.
async fn actors() -> Actors {
    let (_, db) = spawn_real_app().await;
    let (admin, admin_id) = register(&unique_email()).await;
    let row = users::Entity::find_by_id(admin_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let mut active: users::ActiveModel = row.into();
    active.is_admin = Set(true);
    active.update(&db).await.unwrap();

    let (user, user_id) = register(&unique_email()).await;
    mark_email_verified(&db, user_id).await;
    Actors { db, admin, user }
}

async fn admin_post(a: &Actors, path: &str, body: Value) -> axum_test::TestResponse {
    app()
        .await
        .post(path)
        .authorization_bearer(&a.admin)
        .json(&body)
        .await
}

async fn block_url(a: &Actors, url: &str) -> axum_test::TestResponse {
    admin_post(
        a,
        "/admin/blocked/links",
        json!({ "url": url, "reason": "test" }),
    )
    .await
}

async fn create_link(a: &Actors, url: &str) -> axum_test::TestResponse {
    app()
        .await
        .post("/links")
        .authorization_bearer(&a.user)
        .json(&json!({ "original_url": url }))
        .await
}

async fn create_status(a: &Actors, url: &str) -> u16 {
    create_link(a, url).await.status_code().as_u16()
}

/// Create a link to `url` and return `(id, code)`.
async fn make_link(a: &Actors, url: &str) -> (i64, String) {
    let res = create_link(a, url).await;
    assert_eq!(res.status_code(), 201, "create {url}: {}", res.text());
    let body: Value = res.json();
    (
        body["id"].as_i64().unwrap(),
        body["code"].as_str().unwrap().to_string(),
    )
}

async fn assert_all_blocked(a: &Actors, variants: &[String]) {
    for variant in variants {
        let res = create_link(a, variant).await;
        assert_eq!(
            res.status_code(),
            403,
            "{variant:?} must be refused as a blocked URL: {}",
            res.text()
        );
    }
}

/// Every respelling of `https://iana.org/<path>` that reaches the same page.
fn respellings(path: &str) -> Vec<String> {
    vec![
        format!("https://iana.org/{path}"),
        format!("https://iana.org/{path}/"),
        format!("https://iana.org/{path}?"),
        format!("https://iana.org/{path}#f"),
        format!("https://iana.org/{path}/#f"),
        format!("https://iana.org/{path}?x=1"),
        format!("https://iana.org/{path}/?x=1#f"),
        format!("HTTPS://IANA.ORG/{path}"),
        format!("https://iana.org:443/{path}"),
        format!("https://iana.org./{path}"),
        format!("https://someone@iana.org/{path}"),
        format!("  https://iana.org/{path}  "),
    ]
}

#[tokio::test]
async fn blocked_url_holds_against_every_respelling_on_create() {
    let a = actors().await;
    let path = format!("blocked-{}", unique_code());

    // Typed the way a browser shows it: trailing slash and a fragment.
    let blocked = block_url(&a, &format!("https://IANA.org:443/{path}/#top")).await;
    assert_eq!(blocked.status_code(), 201, "block: {}", blocked.text());
    assert_eq!(
        blocked.json::<Value>()["url"],
        json!(format!("https://iana.org/{path}")),
        "the response shows the canonical form that was stored"
    );

    assert_all_blocked(&a, &respellings(&path)).await;

    // Controls: other pages are not caught by it.
    assert_eq!(
        create_status(&a, &format!("https://iana.org/{path}-other")).await,
        201,
        "a sibling page stays allowed"
    );
    assert_eq!(
        create_status(&a, &format!("https://iana.org/{path}/child")).await,
        201,
        "a page below the blocked one stays allowed"
    );
}

#[tokio::test]
async fn blocked_url_without_trailing_slash_also_blocks_the_slash_form() {
    let a = actors().await;
    let path = format!("noslash-{}", unique_code());

    let blocked = block_url(&a, &format!("https://iana.org/{path}")).await;
    assert_eq!(blocked.status_code(), 201, "block: {}", blocked.text());

    assert_all_blocked(&a, &respellings(&path)).await;
}

#[tokio::test]
async fn blocked_url_entry_with_a_query_blocks_only_that_query() {
    let a = actors().await;
    let path = format!("query-{}", unique_code());

    let blocked = block_url(&a, &format!("https://iana.org/{path}?ref=spam#x")).await;
    assert_eq!(blocked.status_code(), 201, "block: {}", blocked.text());
    assert_eq!(
        blocked.json::<Value>()["url"],
        json!(format!("https://iana.org/{path}?ref=spam"))
    );

    assert_all_blocked(
        &a,
        &[
            format!("https://iana.org/{path}?ref=spam"),
            format!("https://iana.org/{path}/?ref=spam#f"),
            format!("HTTPS://IANA.ORG:443/{path}?ref=spam"),
        ],
    )
    .await;

    for allowed in [
        format!("https://iana.org/{path}"),
        format!("https://iana.org/{path}?ref=ok"),
        format!("https://iana.org/{path}?ref=spam&x=1"),
    ] {
        assert_eq!(
            create_status(&a, &allowed).await,
            201,
            "{allowed:?} is a different query and must stay allowed"
        );
    }
}

#[tokio::test]
async fn blocking_a_respelling_of_a_blocked_url_is_a_conflict() {
    let a = actors().await;
    let path = format!("dupe-{}", unique_code());

    let first = block_url(&a, &format!("https://iana.org/{path}")).await;
    assert_eq!(first.status_code(), 201, "block: {}", first.text());
    let again = block_url(&a, &format!("HTTPS://IANA.ORG/{path}/#x")).await;
    assert_eq!(again.status_code(), 409, "{}", again.text());

    let stored = blocked_links::Entity::find()
        .filter(blocked_links::Column::Url.eq(format!("https://iana.org/{path}")))
        .all(&a.db)
        .await
        .unwrap();
    assert_eq!(stored.len(), 1);
}

#[tokio::test]
async fn blocking_something_that_is_not_an_http_url_is_a_bad_request() {
    let a = actors().await;
    for input in [
        "not a url",
        "ftp://iana.org/file",
        "iana.org/no-scheme",
        "   ",
    ] {
        let res = block_url(&a, input).await;
        assert_eq!(res.status_code(), 400, "{input:?}: {}", res.text());
    }
}

#[tokio::test]
async fn blocked_url_respelling_is_refused_on_update() {
    let a = actors().await;
    let path = format!("update-{}", unique_code());
    let (id, _) = make_link(&a, &format!("https://iana.org/fine-{}", unique_code())).await;

    let blocked = block_url(&a, &format!("https://iana.org/{path}")).await;
    assert_eq!(blocked.status_code(), 201, "block: {}", blocked.text());

    for variant in [
        format!("https://iana.org/{path}#f"),
        format!("HTTPS://IANA.ORG/{path}/"),
        format!("https://iana.org:443/{path}?x=1"),
    ] {
        let res = app()
            .await
            .put(&format!("/links/{id}"))
            .authorization_bearer(&a.user)
            .json(&json!({ "original_url": variant }))
            .await;
        assert_eq!(
            res.status_code(),
            403,
            "update to {variant:?}: {}",
            res.text()
        );
    }

    // Control: a destination that is not blocked can still be set.
    let ok = app()
        .await
        .put(&format!("/links/{id}"))
        .authorization_bearer(&a.user)
        .json(&json!({ "original_url": format!("https://iana.org/{path}-elsewhere") }))
        .await;
    assert_eq!(ok.status_code(), 200, "{}", ok.text());
}

#[tokio::test]
async fn blocked_url_respelling_is_refused_in_bulk_create() {
    let a = actors().await;
    let path = format!("bulk-{}", unique_code());
    let allowed = format!("https://iana.org/{path}-fine");

    let blocked = block_url(&a, &format!("https://iana.org/{path}")).await;
    assert_eq!(blocked.status_code(), 201, "block: {}", blocked.text());

    let variants = [
        format!("https://iana.org/{path}/"),
        format!("https://iana.org/{path}?x=1"),
        format!("https://iana.org/{path}#f"),
        format!("HTTPS://IANA.ORG/{path}"),
        format!("https://iana.org:443/{path}"),
    ];
    let mut urls: Vec<String> = variants.to_vec();
    urls.push(allowed.clone());

    let res = app()
        .await
        .post("/links/bulk")
        .authorization_bearer(&a.user)
        .json(&json!({ "urls": urls }))
        .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    let body: Value = res.json();
    assert_eq!(
        body["links"].as_array().unwrap().len(),
        1,
        "only the allowed URL is created: {body}"
    );
    let errors = body["errors"].as_array().unwrap();
    assert_eq!(errors.len(), variants.len(), "{body}");
    for variant in &variants {
        assert!(
            errors
                .iter()
                .any(|e| e.as_str().unwrap().contains(variant.as_str())
                    && e.as_str().unwrap().contains("blocked")),
            "{variant:?} must be reported as blocked: {body}"
        );
    }
}

#[tokio::test]
async fn link_made_before_the_block_stops_redirecting_whatever_its_spelling() {
    let a = actors().await;
    let path = format!("redirect-{}", unique_code());
    let stored_spelling = format!("https://iana.org/{path}/?utm=1#frag");
    let (_, code) = make_link(&a, &stored_spelling).await;
    let (_, other_code) = make_link(&a, &format!("https://iana.org/{path}-other")).await;

    let before = app().await.get(&format!("/{code}")).await;
    assert_eq!(
        before.status_code(),
        307,
        "precondition: the link redirects"
    );

    let blocked = block_url(&a, &format!("HTTPS://iana.org:443/{path}")).await;
    assert_eq!(blocked.status_code(), 201, "block: {}", blocked.text());

    let after = app().await.get(&format!("/{code}")).await;
    assert_eq!(
        after.status_code(),
        410,
        "the stored spelling differs from the blocked entry but is the same page"
    );
    let control = app().await.get(&format!("/{other_code}")).await;
    assert_eq!(
        control.status_code(),
        307,
        "an unrelated link keeps working"
    );
}

/// Domain blocks were already host-normalised through `Url`; pin that the
/// Unicode, punycode, upper-case and trailing-dot spellings all agree.
#[tokio::test]
async fn blocked_domain_holds_against_every_spelling_of_an_idn_host() {
    let a = actors().await;
    let label = unique_code().to_lowercase();
    let unicode = format!("bücher-{label}.de");
    let punycode = url::Url::parse(&format!("http://{unicode}"))
        .unwrap()
        .host_str()
        .unwrap()
        .to_string();

    let blocked = admin_post(
        &a,
        "/admin/blocked/domains",
        json!({ "domain": format!("HTTPS://BÜCHER-{}.DE./path", label.to_uppercase()) }),
    )
    .await;
    assert_eq!(blocked.status_code(), 201, "block: {}", blocked.text());
    assert_eq!(blocked.json::<Value>()["domain"], json!(punycode));

    assert_all_blocked(
        &a,
        &[
            format!("https://{unicode}/p"),
            format!("https://{}/p", unicode.to_uppercase()),
            format!("https://{punycode}./p"),
            format!("https://shop.{unicode}/p"),
        ],
    )
    .await;
    assert_eq!(
        create_status(&a, &format!("https://not{unicode}/p")).await,
        201,
        "a lookalike host stays allowed"
    );
}

async fn try_register(email: &str) -> (u16, String) {
    let res = app()
        .await
        .post("/auth/register")
        .json(&json!({ "email": email, "password": "password123" }))
        .await;
    (res.status_code().as_u16(), res.text())
}

async fn assert_signup_blocked(email: &str) {
    let (status, body) = try_register(email).await;
    assert_eq!(status, 400, "{email:?} must be refused: {body}");
    assert!(body.contains("blocked"), "{email:?}: {body}");
}

fn idn_pair(label: &str) -> (String, String) {
    let unicode = format!("bücher-{label}.de");
    let punycode = url::Url::parse(&format!("http://{unicode}"))
        .unwrap()
        .host_str()
        .unwrap()
        .to_string();
    (unicode, punycode)
}

#[tokio::test]
async fn email_domain_block_typed_as_unicode_holds_for_every_spelling() {
    let a = actors().await;
    let label = unique_code().to_lowercase();
    let (unicode, punycode) = idn_pair(&label);

    let blocked = admin_post(
        &a,
        "/admin/blocked/email-domains",
        json!({ "domain": unicode }),
    )
    .await;
    assert_eq!(blocked.status_code(), 201, "block: {}", blocked.text());
    assert_eq!(
        blocked.json::<Value>()["domain"],
        json!(punycode),
        "the response shows the normalized domain that was stored"
    );

    assert_signup_blocked(&format!("x@{punycode}")).await;
    assert_signup_blocked(&format!("x@{unicode}")).await;
    assert_signup_blocked(&format!("x@{}", unicode.to_uppercase())).await;
    assert_signup_blocked(&format!("x@mail.{unicode}")).await;
}

#[tokio::test]
async fn email_domain_block_typed_as_a_full_address_holds_for_every_spelling() {
    let a = actors().await;
    let label = unique_code().to_lowercase();
    let (unicode, punycode) = idn_pair(&label);

    let blocked = admin_post(
        &a,
        "/admin/blocked/email-domains",
        json!({ "domain": format!("someone@{unicode}") }),
    )
    .await;
    assert_eq!(blocked.status_code(), 201, "block: {}", blocked.text());
    assert_eq!(blocked.json::<Value>()["domain"], json!(punycode));

    assert_signup_blocked(&format!("x@{punycode}")).await;
    assert_signup_blocked(&format!("x@{unicode}")).await;
}

#[tokio::test]
async fn email_domain_block_typed_as_punycode_holds_against_unicode_signup() {
    let a = actors().await;
    let label = unique_code().to_lowercase();
    let (unicode, punycode) = idn_pair(&label);

    let blocked = admin_post(
        &a,
        "/admin/blocked/email-domains",
        json!({ "domain": punycode.to_uppercase() }),
    )
    .await;
    assert_eq!(blocked.status_code(), 201, "block: {}", blocked.text());
    assert_eq!(blocked.json::<Value>()["domain"], json!(punycode));

    assert_signup_blocked(&format!("x@{unicode}")).await;
}

#[tokio::test]
async fn email_domain_block_disables_accounts_whatever_spelling_they_registered_with() {
    let a = actors().await;
    let label = unique_code().to_lowercase();
    let (unicode, punycode) = idn_pair(&label);

    let mut ids = Vec::new();
    for email in [
        format!("early1@{unicode}"),
        format!("early2@{punycode}"),
        format!("early3@{}", unicode.to_uppercase()),
    ] {
        ids.push(register(&email).await.1);
    }

    let blocked = admin_post(
        &a,
        "/admin/blocked/email-domains",
        json!({ "domain": unicode }),
    )
    .await;
    assert_eq!(blocked.status_code(), 201, "block: {}", blocked.text());
    assert_eq!(blocked.json::<Value>()["affected_users"], json!(3));
    for id in ids {
        let user = users::Entity::find_by_id(id)
            .one(&a.db)
            .await
            .unwrap()
            .unwrap();
        assert!(user.disabled_at.is_some(), "user {id} must be disabled");
    }
}

/// POST each target as a block and return `(target, status, body)`.
async fn post_blocks(a: &Actors, path: &str, targets: &[&str]) -> Vec<(String, u16, String)> {
    let mut results = Vec::new();
    for target in targets {
        let res = admin_post(a, path, json!({ "domain": target })).await;
        results.push((target.to_string(), res.status_code().as_u16(), res.text()));
    }
    results
}

fn body_for<'a>(results: &'a [(String, u16, String)], target: &str) -> &'a str {
    &results.iter().find(|(t, _, _)| t == target).unwrap().2
}

#[tokio::test]
async fn domain_block_refuses_single_label_names_and_wildcards() {
    let a = actors().await;
    let results = post_blocks(
        &a,
        "/admin/blocked/domains",
        &[
            "com",
            "COM.",
            "https://com/page",
            "localhost",
            "*.evil.org",
            "https://*.evil.org/",
            "*evil.org",
        ],
    )
    .await;
    // Delete whatever a missing guard let through before asserting: a stored
    // `com` would stay blocked for every other test.
    blocked_domains::Entity::delete_many()
        .filter(blocked_domains::Column::Domain.is_in([
            "com",
            "localhost",
            "*.evil.org",
            "*evil.org",
        ]))
        .exec(&a.db)
        .await
        .unwrap();

    for (target, status, body) in &results {
        assert_eq!(*status, 400, "{target:?} must be refused: {body}");
    }
    assert!(body_for(&results, "com").contains("single-label"));
    let wildcard = body_for(&results, "*.evil.org");
    assert!(
        wildcard.contains("Wildcards are not supported") && wildcard.contains("subdomains"),
        "{wildcard}"
    );

    // Controls: a full domain and an IP address are still blockable.
    let ok = admin_post(
        &a,
        "/admin/blocked/domains",
        json!({ "domain": format!("evil-{}.org", unique_code().to_lowercase()) }),
    )
    .await;
    assert_eq!(ok.status_code(), 201, "{}", ok.text());
    let ip = admin_post(
        &a,
        "/admin/blocked/domains",
        json!({ "domain": "198.51.100.77" }),
    )
    .await;
    assert!(
        matches!(ip.status_code().as_u16(), 201 | 409),
        "an IP host is a full name, not a single label: {}",
        ip.text()
    );
}

#[tokio::test]
async fn email_domain_block_refuses_single_label_names_and_wildcards() {
    let a = actors().await;
    let results = post_blocks(
        &a,
        "/admin/blocked/email-domains",
        &[
            "com",
            "x@com",
            "localhost",
            "*.evil.org",
            "x@*.evil.org",
            "*evil.org",
        ],
    )
    .await;
    blocked_email_domains::Entity::delete_many()
        .filter(blocked_email_domains::Column::Domain.is_in([
            "com",
            "localhost",
            "*.evil.org",
            "*evil.org",
        ]))
        .exec(&a.db)
        .await
        .unwrap();

    for (target, status, body) in &results {
        assert_eq!(*status, 400, "{target:?} must be refused: {body}");
    }
    assert!(body_for(&results, "com").contains("single-label"));
    assert!(body_for(&results, "*.evil.org").contains("Wildcards are not supported"));

    let ok = admin_post(
        &a,
        "/admin/blocked/email-domains",
        json!({ "domain": format!("spam-{}.iana.org", unique_code().to_lowercase()) }),
    )
    .await;
    assert_eq!(ok.status_code(), 201, "{}", ok.text());
}

async fn insert_blocked_link(db: &DatabaseConnection, url: &str) -> i32 {
    blocked_links::ActiveModel {
        url: Set(url.to_string()),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
    .id
}

/// Rows written before the canonical form existed are rewritten by the
/// migration step, after which the checks find them.
#[tokio::test]
async fn migration_rewrites_legacy_blocklist_rows_into_canonical_form() {
    let a = actors().await;
    let label = unique_code().to_lowercase();
    let (unicode, punycode) = idn_pair(&label);

    // Written as typed, before canonicalisation: none of these would match.
    let messy =
        insert_blocked_link(&a.db, &format!("HTTPS://IANA.ORG:443/legacy-{label}/#frag")).await;
    let dupe_first = insert_blocked_link(&a.db, &format!("https://iana.org/dupe-{label}/")).await;
    let dupe_second = insert_blocked_link(&a.db, &format!("https://iana.org/dupe-{label}")).await;
    let with_query = insert_blocked_link(&a.db, &format!("https://iana.org/q-{label}?a=1#x")).await;
    let unusable = insert_blocked_link(&a.db, &format!("not a url {label}")).await;
    let domain = blocked_domains::ActiveModel {
        domain: Set(unicode.clone()),
        ..Default::default()
    }
    .insert(&a.db)
    .await
    .unwrap();
    let email_domain = blocked_email_domains::ActiveModel {
        domain: Set(format!("{}.", unicode.to_uppercase())),
        ..Default::default()
    }
    .insert(&a.db)
    .await
    .unwrap();

    // Before: the legacy spellings do not protect anything.
    assert_eq!(
        create_status(&a, &format!("https://iana.org/legacy-{label}")).await,
        201,
        "precondition: the legacy row does not match yet"
    );

    migration::canonicalise_blocklists(&a.db).await.unwrap();

    let url_of = |id: i32| {
        let db = a.db.clone();
        async move {
            blocked_links::Entity::find_by_id(id)
                .one(&db)
                .await
                .unwrap()
                .map(|row| row.url)
        }
    };
    assert_eq!(
        url_of(messy).await,
        Some(format!("https://iana.org/legacy-{label}"))
    );
    assert_eq!(
        url_of(dupe_first).await,
        Some(format!("https://iana.org/dupe-{label}")),
        "the lowest id of a collapsed pair is kept and rewritten"
    );
    assert_eq!(url_of(dupe_second).await, None, "the later duplicate goes");
    assert_eq!(
        url_of(with_query).await,
        Some(format!("https://iana.org/q-{label}?a=1"))
    );
    assert_eq!(
        url_of(unusable).await,
        Some(format!("not a url {label}")),
        "a row with no canonical form is left alone"
    );
    assert_eq!(
        blocked_domains::Entity::find_by_id(domain.id)
            .one(&a.db)
            .await
            .unwrap()
            .unwrap()
            .domain,
        punycode
    );
    assert_eq!(
        blocked_email_domains::Entity::find_by_id(email_domain.id)
            .one(&a.db)
            .await
            .unwrap()
            .unwrap()
            .domain,
        punycode
    );

    // After: the rewritten rows block.
    assert_all_blocked(
        &a,
        &[
            format!("https://iana.org/legacy-{label}?x=1"),
            format!("https://iana.org/dupe-{label}#f"),
            format!("https://iana.org/q-{label}?a=1"),
            format!("https://{unicode}/p"),
        ],
    )
    .await;
    assert_signup_blocked(&format!("x@{unicode}")).await;
}

/// The migration carries a frozen copy of the canonicalisers (its crate cannot
/// depend on the backend). They must agree today; if the live rules change on
/// purpose, ship a new migration for existing rows and update this corpus.
#[test]
fn migration_canonicalisers_agree_with_the_live_ones() {
    let urls = [
        "https://evil.org/phish",
        "https://evil.org",
        "HTTPS://EVIL.ORG:443/Phish/?a=1&b=2#frag",
        "http://evil.org:80/x/",
        "http://evil.org:8080/x//",
        "https://evil.org./a/../b?",
        "https://user:pw@evil.org/p",
        "https://BÜCHER.de/straße/",
        "https://xn--bcher-kva.de/a%20b/?q=%E2%9C%93",
        "http://[::1]:80/x/",
        "http://1.2.3.4/x",
        "  https://evil.org/padded  ",
        "ftp://evil.org/x",
        "javascript:alert(1)",
        "not a url",
        "",
    ];
    for url in urls {
        assert_eq!(
            migration::canonical_blocked_url(url),
            url_policy::canonical_blocked_url(url),
            "{url:?}"
        );
    }
    let hosts = [
        "evil.org",
        "Evil.ORG.",
        "bücher.de",
        "BÜCHER.DE.",
        "xn--bcher-kva.de",
        "[::1]",
        "1.2.3.4",
        "*.evil.org",
        "a b.com",
        " . ",
        "",
    ];
    for host in hosts {
        assert_eq!(
            migration::canonical_host(host),
            url_policy::canonical_host(host),
            "{host:?}"
        );
    }
}
