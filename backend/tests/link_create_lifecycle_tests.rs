//! Real-router coverage for link create policy, aliases, expiry, passwords,
//! CSV export, and QR. Replaces the theatre in `links_comprehensive_tests.rs`.

mod common;

use chrono::{Duration, Utc};
use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use sea_orm::DatabaseConnection;
use serde_json::{json, Value};

async fn register_verified(server: &axum_test::TestServer, db: &DatabaseConnection) -> String {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    mark_email_verified(db, body["user_id"].as_i64().unwrap() as i32).await;
    body["token"].as_str().unwrap().to_string()
}

async fn create_link(server: &axum_test::TestServer, token: &str, payload: Value) -> Value {
    let res = server
        .post("/links")
        .authorization_bearer(token)
        .json(&payload)
        .await;
    assert_eq!(res.status_code(), 201, "create link: {}", res.text());
    res.json()
}

#[tokio::test]
async fn create_rejects_non_http_schemes_and_xss_payloads() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;

    let too_long = format!("https://iana.org/{}", "a".repeat(2048));
    let cases: &[(&str, &str)] = &[
        ("ftp://iana.org/file", "http"),
        ("javascript:alert(1)", "http"),
        ("iana.org", "Invalid"),
        ("https://iana.org/<script>alert(1)</script>", "malicious"),
        ("https://iana.org/img?onerror=alert(1)", "malicious"),
        (
            "https://iana.org/%3Cscript%3Ealert(1)%3C/script%3E",
            "malicious",
        ),
        (&too_long, "too long"),
    ];

    for (url, needle) in cases {
        let res = server
            .post("/links")
            .authorization_bearer(&token)
            .json(&json!({ "original_url": url }))
            .await;
        assert_eq!(res.status_code(), 400, "{url}: {}", res.text());
        let body = res.text().to_lowercase();
        assert!(
            body.contains(&needle.to_lowercase()),
            "{url} error should mention {needle}: {body}"
        );
    }
}

#[tokio::test]
async fn create_accepts_http_urls_with_path_query_and_port() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;

    for url in [
        "http://iana.org",
        "https://iana.org/path/to/page",
        "https://iana.org/search?q=test&page=1",
        "https://iana.org/page#section",
        "https://iana.org:8080/api",
    ] {
        let body = create_link(&server, &token, json!({ "original_url": url })).await;
        assert_eq!(body["original_url"], url, "stored destination for {url}");
        let code = body["code"].as_str().unwrap();
        let redirect = server.get(&format!("/{code}")).await;
        assert_eq!(
            redirect.status_code(),
            307,
            "redirect {url}: {}",
            redirect.text()
        );
        let location = redirect
            .headers()
            .get("location")
            .expect("location header")
            .to_str()
            .unwrap_or("");
        assert_eq!(location, url, "location for {url}");
    }
}

#[tokio::test]
async fn custom_alias_validation_on_create() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let dest = format!("https://iana.org/alias-{}", unique_code());

    let valid = format!("ok-{}", unique_code());
    let created = create_link(
        &server,
        &token,
        json!({ "original_url": dest, "custom_alias": valid }),
    )
    .await;
    assert_eq!(created["code"], valid);

    let with_underscore = format!("ok_{}", unique_code());
    let created = create_link(
        &server,
        &token,
        json!({
            "original_url": format!("{dest}/u"),
            "custom_alias": with_underscore
        }),
    )
    .await;
    assert_eq!(created["code"], with_underscore);

    let too_long_alias = "a".repeat(51);
    let rejected: &[(&str, &str)] = &[
        ("abc", "at least"),
        (&too_long_alias, "at most"),
        ("-mylink", "start or end"),
        ("mylink_", "start or end"),
        ("my@link", "letters, numbers, hyphens"),
        ("my link", "letters, numbers, hyphens"),
        ("about", "reserved"),
    ];
    for (alias, needle) in rejected {
        let res = server
            .post("/links")
            .authorization_bearer(&token)
            .json(&json!({
                "original_url": format!("{dest}/{alias}"),
                "custom_alias": alias
            }))
            .await;
        assert_eq!(res.status_code(), 400, "{alias}: {}", res.text());
        let body = res.text().to_lowercase();
        assert!(
            body.contains(needle),
            "{alias} error should mention {needle}: {body}"
        );
    }
}

#[tokio::test]
async fn generated_short_code_is_six_alphanumeric_and_redirects() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let dest = format!("https://iana.org/gen-{}", unique_code());
    let body = create_link(&server, &token, json!({ "original_url": dest })).await;
    let code = body["code"].as_str().expect("code");
    assert_eq!(code.len(), 6, "generated codes are 6 characters: {code}");
    assert!(
        code.chars().all(|c| c.is_ascii_alphanumeric()),
        "generated code must be alphanumeric: {code}"
    );

    let redirect = server.get(&format!("/{code}")).await;
    assert_eq!(redirect.status_code(), 307, "{}", redirect.text());
    assert_eq!(
        redirect
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap(),
        dest
    );
}

#[tokio::test]
async fn expired_link_returns_410_on_redirect() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let dest = format!("https://iana.org/expired-{}", unique_code());
    let expires = (Utc::now() - Duration::hours(1)).to_rfc3339();
    let body = create_link(
        &server,
        &token,
        json!({ "original_url": dest, "expires_at": expires }),
    )
    .await;
    let code = body["code"].as_str().unwrap();

    let res = server.get(&format!("/{code}")).await;
    assert_eq!(res.status_code(), 410, "expired redirect: {}", res.text());
    assert!(
        res.text().contains("expired"),
        "body should explain expiry: {}",
        res.text()
    );
}

#[tokio::test]
async fn deleted_link_returns_404_on_redirect() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let dest = format!("https://iana.org/deleted-{}", unique_code());
    let body = create_link(&server, &token, json!({ "original_url": dest })).await;
    let id = body["id"].as_i64().unwrap();
    let code = body["code"].as_str().unwrap().to_string();

    let del = server
        .delete(&format!("/links/{id}"))
        .authorization_bearer(&token)
        .await;
    assert_eq!(del.status_code(), 200, "delete: {}", del.text());

    let res = server.get(&format!("/{code}")).await;
    assert_eq!(
        res.status_code(),
        404,
        "soft-deleted redirect must be 404, not a leak: {}",
        res.text()
    );
}

#[tokio::test]
async fn password_protected_link_wrong_and_right() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let dest = format!("https://iana.org/secret-{}", unique_code());
    let password = "p@$$w0rd!#%^&*()";
    let body = create_link(
        &server,
        &token,
        json!({ "original_url": dest, "password": password }),
    )
    .await;
    let code = body["code"].as_str().unwrap().to_string();

    let locked = server.get(&format!("/{code}")).await;
    assert_eq!(
        locked.status_code(),
        307,
        "missing password should bounce to the interstitial: {}",
        locked.text()
    );
    let location = locked.headers().get("location").unwrap().to_str().unwrap();
    assert!(
        location.contains("/password/") && location.contains(&code),
        "expected password page, got {location}"
    );

    let wrong = server
        .post(&format!("/{code}/verify"))
        .json(&json!({ "password": "wrong" }))
        .await;
    assert_eq!(wrong.status_code(), 401, "wrong password: {}", wrong.text());

    let ok = server
        .post(&format!("/{code}/verify"))
        .json(&json!({ "password": password }))
        .await;
    assert_eq!(ok.status_code(), 200, "correct password: {}", ok.text());
    let verified: Value = ok.json();
    assert!(
        verified.get("url").is_none(),
        "verify must not leak the destination: {verified}"
    );
    let unlock_url = verified["redirect_url"].as_str().expect("redirect_url");
    assert!(
        !unlock_url.contains("secret-"),
        "unlock URL leaked the destination: {unlock_url}"
    );

    let pw_header = axum::http::HeaderName::from_static("x-link-password");
    let unlocked = server
        .get(&format!("/{code}"))
        .add_header(pw_header, password)
        .await;
    assert_eq!(
        unlocked.status_code(),
        307,
        "header password should redirect: {}",
        unlocked.text()
    );
    assert_eq!(
        unlocked
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap(),
        dest
    );
}

/// A non-ASCII link password cannot travel in the `x-link-password` header,
/// so it must work through the JSON unlock flow the password page uses: the
/// wrong password is refused, the right one yields an unlock URL that
/// redirects to the destination.
#[tokio::test]
async fn password_protected_link_accepts_non_ascii_password() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let dest = format!("https://iana.org/unicode-secret-{}", unique_code());
    let password = "ключ-🔑-Schlüssel";
    let body = create_link(
        &server,
        &token,
        json!({ "original_url": dest, "password": password }),
    )
    .await;
    let code = body["code"].as_str().unwrap().to_string();

    let wrong = server
        .post(&format!("/{code}/verify"))
        .json(&json!({ "password": "ключ-🔑-schlüssel" }))
        .await;
    assert_eq!(wrong.status_code(), 401, "wrong password: {}", wrong.text());

    let ok = server
        .post(&format!("/{code}/verify"))
        .json(&json!({ "password": password }))
        .await;
    assert_eq!(ok.status_code(), 200, "correct password: {}", ok.text());
    let unlock_url = ok.json::<Value>()["redirect_url"]
        .as_str()
        .expect("redirect_url")
        .to_string();
    let unlock_url = url::Url::parse(&unlock_url).expect("absolute unlock URL");
    let unlocked = server
        .get(&format!(
            "{}?{}",
            unlock_url.path(),
            unlock_url.query().expect("unlock query")
        ))
        .await;
    assert_eq!(
        unlocked.status_code(),
        307,
        "unlock URL must redirect: {}",
        unlocked.text()
    );
    assert_eq!(
        unlocked
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap(),
        dest
    );
}

#[tokio::test]
async fn export_links_csv_for_owner_only() {
    let (server, db) = spawn_real_app().await;
    let token_a = register_verified(&server, &db).await;
    let token_b = register_verified(&server, &db).await;

    let alias_a = format!("ex-{}", unique_code());
    let dest_a = format!("https://iana.org/path?a=1,b=2&q={}", unique_code());
    create_link(
        &server,
        &token_a,
        json!({
            "original_url": dest_a,
            "custom_alias": alias_a,
            "notes": "=1+1, \"quoted\""
        }),
    )
    .await;

    let alias_b = format!("ex-{}", unique_code());
    create_link(
        &server,
        &token_b,
        json!({
            "original_url": format!("https://iana.org/b-{}", unique_code()),
            "custom_alias": alias_b
        }),
    )
    .await;

    let unauth = server.get("/links/export").await;
    assert_eq!(unauth.status_code(), 401, "export requires auth");

    let res = server
        .get("/links/export")
        .authorization_bearer(&token_a)
        .await;
    assert_eq!(res.status_code(), 200, "export: {}", res.text());
    assert_eq!(
        res.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("text/csv")
    );
    let csv = res.text();
    assert!(
        csv.starts_with(
            "ID,Code,Original URL,Short URL,Click Count,Created At,Expires At,Has Password,Notes,Folder ID,Max Clicks,Starts At\n"
        ),
        "unexpected header: {csv}"
    );
    assert!(csv.contains(&alias_a), "owner row missing: {csv}");
    assert!(
        !csv.contains(&alias_b),
        "other user's link leaked into export: {csv}"
    );
    assert!(
        csv.contains(&format!("\"{dest_a}\"")),
        "comma in URL must be quoted: {csv}"
    );
    assert!(
        csv.contains("'=1+1, \"\"quoted\"\"\""),
        "formula-injection notes must be quoted and neutralized: {csv}"
    );
}

#[tokio::test]
async fn qr_code_png_for_owner_forbidden_for_stranger() {
    let (server, db) = spawn_real_app().await;
    let token_a = register_verified(&server, &db).await;
    let token_b = register_verified(&server, &db).await;
    let body = create_link(
        &server,
        &token_a,
        json!({ "original_url": format!("https://iana.org/qr-{}", unique_code()) }),
    )
    .await;
    let id = body["id"].as_i64().unwrap();

    let unauth = server.get(&format!("/links/{id}/qr")).await;
    assert_eq!(unauth.status_code(), 401, "qr requires auth");

    let forbidden = server
        .get(&format!("/links/{id}/qr"))
        .authorization_bearer(&token_b)
        .await;
    assert_eq!(
        forbidden.status_code(),
        403,
        "stranger qr: {}",
        forbidden.text()
    );

    let res = server
        .get(&format!("/links/{id}/qr"))
        .authorization_bearer(&token_a)
        .await;
    assert_eq!(res.status_code(), 200, "owner qr: {}", res.text());
    assert_eq!(
        res.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("image/png")
    );
    let bytes = res.as_bytes();
    assert!(
        bytes.starts_with(&[0x89, b'P', b'N', b'G']),
        "QR must be a PNG, got {} bytes starting {:?}",
        bytes.len(),
        &bytes[..bytes.len().min(8)]
    );
}

/// An empty password is no password. Stored as a bcrypt hash of "", it made a
/// link that the unlock form (which will not submit an empty field) could
/// never open. On create "" leaves the link open; on update it leaves the
/// existing password alone, because clearing it is what remove_password is for.
#[tokio::test]
async fn empty_link_password_is_no_password() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let dest = format!("https://iana.org/open-{}", unique_code());
    let body = create_link(
        &server,
        &token,
        json!({ "original_url": dest, "password": "" }),
    )
    .await;
    assert_eq!(body["has_password"], json!(false), "created: {body}");
    let code = body["code"].as_str().unwrap().to_string();
    let id = body["id"].as_i64().unwrap();

    let redirect = server.get(&format!("/{code}")).await;
    assert_eq!(
        redirect.status_code(),
        307,
        "an open link must redirect: {}",
        redirect.text()
    );
    assert_eq!(
        redirect
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok()),
        Some(dest.as_str())
    );

    let set = server
        .put(&format!("/links/{id}"))
        .authorization_bearer(&token)
        .json(&json!({ "password": "correct-horse" }))
        .await;
    assert_eq!(set.status_code(), 200, "set password: {}", set.text());
    let blank = server
        .put(&format!("/links/{id}"))
        .authorization_bearer(&token)
        .json(&json!({ "password": "" }))
        .await;
    assert_eq!(blank.status_code(), 200, "empty update: {}", blank.text());
    assert_eq!(
        blank.json::<Value>()["has_password"],
        json!(true),
        "\"\" must not replace the password"
    );
    let unlock = server
        .post(&format!("/{code}/verify"))
        .json(&json!({ "password": "correct-horse" }))
        .await;
    assert_eq!(
        unlock.status_code(),
        200,
        "the original password must still unlock: {}",
        unlock.text()
    );
}

/// bcrypt reads only the first 72 bytes, so a longer link password would
/// unlock with anything sharing that prefix. Create and update refuse it,
/// counted in bytes (37 "é" are 74); exactly 72 bytes is accepted.
#[tokio::test]
async fn link_password_over_72_bytes_is_refused() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let dest = format!("https://iana.org/long-{}", unique_code());

    for too_long in ["k".repeat(73), "é".repeat(37)] {
        let res = server
            .post("/links")
            .authorization_bearer(&token)
            .json(&json!({ "original_url": dest, "password": too_long }))
            .await;
        assert_eq!(
            res.status_code(),
            400,
            "{} bytes accepted: {}",
            too_long.len(),
            res.text()
        );
        assert!(res.text().contains("at most 72 bytes"), "{}", res.text());
    }

    let body = create_link(
        &server,
        &token,
        json!({ "original_url": dest, "password": "k".repeat(72) }),
    )
    .await;
    assert_eq!(body["has_password"], json!(true), "created: {body}");
    let id = body["id"].as_i64().unwrap();

    let res = server
        .put(&format!("/links/{id}"))
        .authorization_bearer(&token)
        .json(&json!({ "password": "k".repeat(73) }))
        .await;
    assert_eq!(res.status_code(), 400, "update: {}", res.text());
    assert!(res.text().contains("at most 72 bytes"), "{}", res.text());
}

/// `data:` is refused only where a destination's redirect parameter would
/// treat it as a URL. A path that merely contains the letters, like Wikidata's
/// own namespace or the Commons `Data:` namespace, is an ordinary link.
#[tokio::test]
async fn data_in_an_ordinary_url_is_allowed_but_not_as_a_nested_url() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;

    for ok in [
        "https://www.wikidata.org/wiki/Wikidata:Main_Page",
        "https://commons.wikimedia.org/wiki/Data:Sandbox/Example.tab",
    ] {
        let body = create_link(&server, &token, json!({ "original_url": ok })).await;
        assert_eq!(body["original_url"], json!(ok));
    }

    for nested in [
        "https://iana.org/go?next=data:text/html,hi",
        "https://iana.org/go#data:text/html,hi",
        "https://iana.org/go?a=1&data:text/html,hi",
    ] {
        let res = server
            .post("/links")
            .authorization_bearer(&token)
            .json(&json!({ "original_url": nested }))
            .await;
        assert_eq!(res.status_code(), 400, "{nested} accepted: {}", res.text());
        assert!(res.text().contains("Data URLs"), "{nested}: {}", res.text());
    }
}

/// Creating a link whose expiry has already passed is allowed, but the
/// response must say it is inactive, as GET /links and the redirect do.
#[tokio::test]
async fn create_reports_an_already_expired_link_as_inactive() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;

    let expired = create_link(
        &server,
        &token,
        json!({
            "original_url": format!("https://iana.org/expired-{}", unique_code()),
            "expires_at": (Utc::now() - Duration::hours(1)).to_rfc3339(),
        }),
    )
    .await;
    assert_eq!(expired["is_active"], json!(false), "created: {expired}");
    let code = expired["code"].as_str().unwrap();
    let redirect = server.get(&format!("/{code}")).await;
    assert_eq!(redirect.status_code(), 410, "{}", redirect.text());

    let live = create_link(
        &server,
        &token,
        json!({ "original_url": format!("https://iana.org/live-{}", unique_code()) }),
    )
    .await;
    assert_eq!(live["is_active"], json!(true), "created: {live}");
}
