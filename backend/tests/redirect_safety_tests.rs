//! Requests that are not visits must not use a link up or reveal where it goes.
//!
//! * A HEAD (axum serves it with the GET handler) and a GET from a link
//!   unfurler (a chat app drawing a preview card before the recipient clicks)
//!   used to consume a click cap, burn a one-time link and count as a click.
//! * `GET /{code}/preview` kept publishing the destination of links that are
//!   used up, expired, not yet live or blocked, and described a routed link by
//!   its fallback URL.
//! * The QR handler panicked on a multibyte `?color=`.
//!
//! The click buffer is observable here because the test state runs no flush
//! task, so these tests build `AppState` themselves instead of using
//! `common::spawn_real_app`.

mod common;

use std::sync::Arc;

use axum::http::{Method, header};
use chrono::{Duration, Utc};
use common::{mark_email_verified, set_env, setup_test_db, unique_code, unique_email};
use opn_onl_backend::entity::{blocked_domains, links};
use opn_onl_backend::handlers::websocket::WsState;
use opn_onl_backend::utils::ClickBuffer;
use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ActiveValue::Set, ColumnTrait, DatabaseConnection,
    EntityTrait, QueryFilter,
};
use serde_json::{Value, json};

const IPHONE_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) \
AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
const DESKTOP_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
const SLACKBOT_UA: &str = "Slackbot-LinkExpanding 1.0 (+https://api.slack.com/robots)";

struct App {
    server: axum_test::TestServer,
    db: DatabaseConnection,
    clicks: Arc<ClickBuffer>,
    ws: Arc<WsState>,
}

async fn spawn_app() -> App {
    // Same environment pinning as common::spawn_real_app().
    set_env("FORCE_HTTPS", "false");
    set_env("TRUST_PROXY_HEADERS", "false");
    if std::env::var("JWT_SECRET").is_err() {
        set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }
    let db = setup_test_db().await;
    let mut state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    let ws = Arc::new(WsState::new());
    state.ws_state = Some(ws.clone());
    let clicks = state.click_buffer.clone();
    let server = axum_test::TestServer::new(opn_onl_backend::build_router(state))
        .expect("failed to start test server");
    App {
        server,
        db,
        clicks,
        ws,
    }
}

async fn register_verified(app: &App) -> String {
    let res = app
        .server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    mark_email_verified(&app.db, body["user_id"].as_i64().expect("user_id") as i32).await;
    body["token"].as_str().expect("token").to_string()
}

/// Returns `(id, code)`.
async fn create_link(app: &App, token: &str, mut payload: Value) -> (i32, String) {
    payload["custom_alias"] = json!(unique_code());
    let res = app
        .server
        .post("/links")
        .authorization_bearer(token)
        .json(&payload)
        .await;
    assert_eq!(res.status_code(), 201, "create link: {}", res.text());
    let body: Value = res.json();
    (
        body["id"].as_i64().expect("id") as i32,
        body["code"].as_str().expect("code").to_string(),
    )
}

async fn stored(app: &App, code: &str) -> links::Model {
    links::Entity::find()
        .filter(links::Column::Code.eq(code))
        .one(&app.db)
        .await
        .expect("db error")
        .expect("link not found")
}

fn location(res: &axum_test::TestResponse) -> Option<String> {
    res.maybe_header(header::LOCATION)
        .map(|v| v.to_str().expect("utf-8 Location").to_string())
}

async fn head(app: &App, code: &str) -> axum_test::TestResponse {
    app.server.method(Method::HEAD, &format!("/{code}")).await
}

async fn get_as(app: &App, code: &str, user_agent: &str) -> axum_test::TestResponse {
    app.server
        .get(&format!("/{code}"))
        .add_header(header::USER_AGENT, user_agent)
        .await
}

async fn preview(app: &App, code: &str) -> Value {
    let res = app.server.get(&format!("/{code}/preview")).await;
    assert_eq!(res.status_code(), 200, "preview: {}", res.text());
    res.json()
}

async fn preview_as(app: &App, code: &str, user_agent: &str) -> Value {
    let res = app
        .server
        .get(&format!("/{code}/preview"))
        .add_header(header::USER_AGENT, user_agent)
        .await;
    assert_eq!(res.status_code(), 200, "preview: {}", res.text());
    res.json()
}

async fn put_rules(app: &App, token: &str, link_id: i32, rules: Value) {
    let res = app
        .server
        .put(&format!("/links/{link_id}/rules"))
        .authorization_bearer(token)
        .json(&json!({ "rules": rules }))
        .await;
    assert_eq!(res.status_code(), 200, "PUT rules: {}", res.text());
}

/// Block `host` (and its subdomains) after the link was created, as an admin
/// does when a live destination turns out to be abusive.
async fn block_domain(app: &App, host: &str) {
    blocked_domains::ActiveModel {
        id: NotSet,
        domain: Set(host.to_string()),
        reason: Set(Some("test".to_string())),
        blocked_by: Set(None),
        created_at: Set(Utc::now().naive_utc()),
    }
    .insert(&app.db)
    .await
    .expect("block domain");
}

fn unique_host() -> String {
    format!("blk-{}.iana.org", uuid::Uuid::new_v4().simple())
}

// ---------------------------------------------------------------------------
// HEAD
// ---------------------------------------------------------------------------

/// A monitor, `curl -I` or a chat app's HEAD must not burn a one-time link, and
/// must not tell the caller where it goes.
#[tokio::test]
async fn head_does_not_burn_a_one_time_link_or_reveal_its_destination() {
    let app = spawn_app().await;
    let token = register_verified(&app).await;
    let dest = "https://iana.org/head-burn-secret";
    let (_id, code) = create_link(
        &app,
        &token,
        json!({ "original_url": dest, "burn_after_reading": true }),
    )
    .await;

    let res = head(&app, &code).await;
    assert_eq!(res.status_code(), 200, "HEAD on a live one-time link");
    assert_eq!(location(&res), None, "HEAD must not reveal the destination");
    assert!(res.as_bytes().is_empty(), "HEAD has no body");

    let row = stored(&app, &code).await;
    assert_eq!(row.click_count, 0, "HEAD must not consume the click");
    assert!(row.burned_at.is_none(), "HEAD must not burn the link");

    // The first real visitor still gets through, exactly once.
    let first = get_as(&app, &code, DESKTOP_UA).await;
    assert_eq!(
        first.status_code(),
        307,
        "first real visit: {}",
        first.text()
    );
    assert_eq!(location(&first).as_deref(), Some(dest));
    let second = get_as(&app, &code, DESKTOP_UA).await;
    assert_eq!(second.status_code(), 410, "the link burns after one visit");
}

#[tokio::test]
async fn heads_do_not_consume_a_click_cap() {
    let app = spawn_app().await;
    let token = register_verified(&app).await;
    let dest = "https://iana.org/head-cap-two";
    let (_id, code) = create_link(
        &app,
        &token,
        json!({ "original_url": dest, "max_clicks": 2 }),
    )
    .await;

    for _ in 0..3 {
        let res = head(&app, &code).await;
        assert_eq!(res.status_code(), 200);
        assert_eq!(location(&res), None);
    }
    assert_eq!(stored(&app, &code).await.click_count, 0);

    // Both real clicks are still available, and the cap still bites after them.
    for visit in 1..=2 {
        let res = get_as(&app, &code, DESKTOP_UA).await;
        assert_eq!(res.status_code(), 307, "visit {visit}: {}", res.text());
        assert_eq!(location(&res).as_deref(), Some(dest));
    }
    assert_eq!(get_as(&app, &code, DESKTOP_UA).await.status_code(), 410);
}

/// An uncapped link has nothing to protect, so a HEAD answers exactly like the
/// GET would. It is still not a visit: no buffered click, no count, no event.
#[tokio::test]
async fn head_on_a_plain_link_redirects_but_records_no_click() {
    let app = spawn_app().await;
    let token = register_verified(&app).await;
    let dest = "https://iana.org/head-plain";
    let (id, code) = create_link(&app, &token, json!({ "original_url": dest })).await;
    let mut events = app.ws.click_tx.subscribe();

    let res = head(&app, &code).await;
    assert_eq!(res.status_code(), 307);
    assert_eq!(location(&res).as_deref(), Some(dest));
    assert_eq!(app.clicks.pending_count(id), 0, "HEAD must not be buffered");
    assert!(
        events.try_recv().is_err(),
        "HEAD must not broadcast a click"
    );

    // Control: the same link counts a real GET (so the assertions above can fail).
    let get = get_as(&app, &code, DESKTOP_UA).await;
    assert_eq!(get.status_code(), 307);
    assert_eq!(location(&get).as_deref(), Some(dest));
    assert_eq!(app.clicks.pending_count(id), 1, "a GET is a counted click");
    let event = events.try_recv().expect("a GET broadcasts a click");
    assert_eq!(event.link_code, code);
}

/// Nothing is recorded for a HEAD on a capped link either: no analytics event,
/// no broadcast, no change to the stored count.
#[tokio::test]
async fn head_on_a_capped_link_records_no_click_event() {
    let app = spawn_app().await;
    let token = register_verified(&app).await;
    let (id, code) = create_link(
        &app,
        &token,
        json!({ "original_url": "https://iana.org/head-capped", "max_clicks": 5 }),
    )
    .await;
    let mut events = app.ws.click_tx.subscribe();
    let queued_before = app.clicks.queued_event_count();

    head(&app, &code).await;

    assert_eq!(stored(&app, &code).await.click_count, 0);
    assert_eq!(app.clicks.pending_count(id), 0);
    assert!(
        events.try_recv().is_err(),
        "HEAD must not broadcast a click"
    );
    // The buffer is private to this app, so a global count is safe to compare.
    assert_eq!(app.clicks.queued_event_count(), queued_before);

    // Control: a GET is consumed and broadcast.
    assert_eq!(get_as(&app, &code, DESKTOP_UA).await.status_code(), 307);
    assert_eq!(stored(&app, &code).await.click_count, 1);
    assert!(events.try_recv().is_ok(), "a GET broadcasts a click");
}

/// Links that need a password or opted into the interstitial send a HEAD to the
/// prompt, as they send a GET, before anything is consumed.
#[tokio::test]
async fn head_on_password_and_interstitial_links_goes_to_the_prompt() {
    let app = spawn_app().await;
    let token = register_verified(&app).await;
    let (_id, locked) = create_link(
        &app,
        &token,
        json!({
            "original_url": "https://iana.org/head-locked",
            "password": "hunter2!",
            "burn_after_reading": true,
        }),
    )
    .await;
    let (_id, interstitial) = create_link(
        &app,
        &token,
        json!({
            "original_url": "https://iana.org/head-interstitial",
            "safe_link_interstitial": true,
            "burn_after_reading": true,
        }),
    )
    .await;

    let res = head(&app, &locked).await;
    assert_eq!(res.status_code(), 307);
    assert!(
        location(&res).is_some_and(|l| l.ends_with(&format!("/password/{locked}"))),
        "{:?}",
        location(&res)
    );
    let res = head(&app, &interstitial).await;
    assert_eq!(res.status_code(), 307);
    assert!(
        location(&res).is_some_and(|l| l.ends_with(&format!("/r/{interstitial}"))),
        "{:?}",
        location(&res)
    );

    for code in [&locked, &interstitial] {
        let row = stored(&app, code).await;
        assert_eq!(row.click_count, 0);
        assert!(row.burned_at.is_none());
    }
}

/// A HEAD on a used-up link is still told so, like a GET.
#[tokio::test]
async fn head_on_a_used_up_link_is_gone() {
    let app = spawn_app().await;
    let token = register_verified(&app).await;
    let (_id, code) = create_link(
        &app,
        &token,
        json!({ "original_url": "https://iana.org/head-used", "burn_after_reading": true }),
    )
    .await;
    assert_eq!(get_as(&app, &code, DESKTOP_UA).await.status_code(), 307);

    let res = head(&app, &code).await;
    assert_eq!(res.status_code(), 410);
    assert_eq!(location(&res), None);
}

/// A HEAD on a routed, uncapped link gets the destination the GET would.
#[tokio::test]
async fn head_on_a_routed_link_follows_the_routing_rules() {
    let app = spawn_app().await;
    let token = register_verified(&app).await;
    let (id, code) = create_link(
        &app,
        &token,
        json!({ "original_url": "https://iana.org/head-fallback" }),
    )
    .await;
    let mobile = "https://iana.org/head-mobile";
    put_rules(
        &app,
        &token,
        id,
        json!([
            { "destination_url": mobile, "match_device": "Mobile", "priority": 0 },
            { "destination_url": "https://iana.org/head-web", "priority": 1 },
        ]),
    )
    .await;

    let res = app
        .server
        .method(Method::HEAD, &format!("/{code}"))
        .add_header(header::USER_AGENT, IPHONE_UA)
        .await;
    assert_eq!(res.status_code(), 307);
    assert_eq!(location(&res).as_deref(), Some(mobile));
    assert_eq!(app.clicks.pending_count(id), 0);
}

// ---------------------------------------------------------------------------
// Link unfurlers
// ---------------------------------------------------------------------------

/// The chat app fetches the link when it is pasted; the recipient who clicks
/// afterwards must still get through, once.
#[tokio::test]
async fn an_unfurler_does_not_burn_a_one_time_link() {
    let app = spawn_app().await;
    let token = register_verified(&app).await;
    let dest = "https://iana.org/unfurl-burn-secret";
    let (_id, code) = create_link(
        &app,
        &token,
        json!({ "original_url": dest, "burn_after_reading": true }),
    )
    .await;
    let mut events = app.ws.click_tx.subscribe();

    let res = get_as(&app, &code, SLACKBOT_UA).await;
    assert_eq!(res.status_code(), 200);
    assert_eq!(location(&res), None);
    let content_type = res.header(header::CONTENT_TYPE);
    assert!(
        content_type.to_str().unwrap().starts_with("text/html"),
        "{content_type:?}"
    );
    let page = res.text();
    assert!(page.contains("<title>opn.onl</title>"), "{page}");
    assert!(
        page.contains(r#"<meta name="robots" content="noindex">"#),
        "{page}"
    );
    assert!(
        !page.contains("unfurl-burn-secret"),
        "page leaks the destination: {page}"
    );
    assert!(
        !page.contains("iana.org"),
        "page leaks the destination host: {page}"
    );
    assert!(
        !page.contains("og:"),
        "page carries Open Graph tags: {page}"
    );

    let row = stored(&app, &code).await;
    assert_eq!(row.click_count, 0, "an unfurler must not consume the click");
    assert!(
        row.burned_at.is_none(),
        "an unfurler must not burn the link"
    );
    assert!(events.try_recv().is_err(), "an unfurler is not a click");

    // The recipient's browser opens the still-unused link, and burns it.
    let visit = get_as(&app, &code, DESKTOP_UA).await;
    assert_eq!(visit.status_code(), 307, "{}", visit.text());
    assert_eq!(location(&visit).as_deref(), Some(dest));
    assert_eq!(stored(&app, &code).await.click_count, 1);
    assert_eq!(get_as(&app, &code, DESKTOP_UA).await.status_code(), 410);
}

#[tokio::test]
async fn every_listed_unfurler_leaves_a_capped_link_alone() {
    let app = spawn_app().await;
    let token = register_verified(&app).await;
    let (_id, code) = create_link(
        &app,
        &token,
        json!({ "original_url": "https://iana.org/unfurl-cap-one", "max_clicks": 1 }),
    )
    .await;

    for ua in [
        "Slackbot-LinkExpanding 1.0 (+https://api.slack.com/robots)",
        "Slack-ImgProxy (+https://api.slack.com/robots)",
        "Mozilla/5.0 (compatible; Discordbot/2.0; +https://discordapp.com)",
        "WhatsApp/2.23.20.0 A",
        "facebookexternalhit/1.1 (+http://www.facebook.com/externalhit_uatext.php)",
        "Facebot Twitterbot/1.0",
        "Twitterbot/1.0",
        "TelegramBot (like TwitterBot)",
        "LinkedInBot/1.0 (compatible; Mozilla/5.0; Apache-HttpClient +http://www.linkedin.com)",
        "Mozilla/5.0 (Windows NT 6.1; WOW64) SkypeUriPreview Preview/0.5",
        "Mozilla/5.0 (compatible; redditbot/1.0; +http://www.reddit.com/feedback)",
        "Iframely/1.3.1 (+https://iframely.com/docs/about)",
        "Mozilla/5.0 (compatible; Embedly/0.2; +http://support.embed.ly/)",
        "Mozilla/5.0 (Windows NT 10.0) AppleWebKit/537.36 MicrosoftPreview/2.0",
        "http.rb/5.1.1 (Mastodon/4.2.0; +https://mastodon.social/)",
    ] {
        let res = get_as(&app, &code, ua).await;
        assert_eq!(res.status_code(), 200, "{ua}: {}", res.text());
        assert_eq!(location(&res), None, "{ua}");
    }
    assert_eq!(stored(&app, &code).await.click_count, 0);

    let visit = get_as(&app, &code, DESKTOP_UA).await;
    assert_eq!(visit.status_code(), 307);
}

/// Only links that can be used up are protected. An uncapped link has nothing
/// to burn, so an unfurler is redirected and counted like any other client.
#[tokio::test]
async fn an_unfurler_on_an_uncapped_link_is_redirected_and_counted() {
    let app = spawn_app().await;
    let token = register_verified(&app).await;
    let dest = "https://iana.org/unfurl-plain";
    let (id, code) = create_link(&app, &token, json!({ "original_url": dest })).await;

    let res = get_as(&app, &code, SLACKBOT_UA).await;
    assert_eq!(res.status_code(), 307);
    assert_eq!(location(&res).as_deref(), Some(dest));
    assert_eq!(app.clicks.pending_count(id), 1);
}

/// A used-up link is gone for an unfurler too; the placeholder page is only for
/// links that are still waiting for their visit.
#[tokio::test]
async fn an_unfurler_on_a_used_up_link_is_told_it_is_gone() {
    let app = spawn_app().await;
    let token = register_verified(&app).await;
    let (_id, code) = create_link(
        &app,
        &token,
        json!({ "original_url": "https://iana.org/unfurl-used", "burn_after_reading": true }),
    )
    .await;
    assert_eq!(get_as(&app, &code, DESKTOP_UA).await.status_code(), 307);

    assert_eq!(get_as(&app, &code, SLACKBOT_UA).await.status_code(), 410);
}

// ---------------------------------------------------------------------------
// Preview
// ---------------------------------------------------------------------------

fn assert_destination_withheld(body: &Value, secret: &str) {
    assert_eq!(body["original_url"], "", "original_url leaked: {body}");
    assert_eq!(body["domain"], "", "domain leaked: {body}");
    assert!(
        !body.to_string().contains(secret),
        "preview leaks the destination: {body}"
    );
}

#[tokio::test]
async fn preview_withholds_the_destination_of_a_link_that_hit_its_click_cap() {
    let app = spawn_app().await;
    let token = register_verified(&app).await;
    let (_id, code) = create_link(
        &app,
        &token,
        json!({ "original_url": "https://iana.org/preview-cap-secret", "max_clicks": 1 }),
    )
    .await;

    // Control: while it still has clicks left the preview describes it.
    let body = preview(&app, &code).await;
    assert_eq!(body["original_url"], "https://iana.org/preview-cap-secret");
    assert_eq!(body["domain"], "iana.org");
    assert_eq!(body["is_active"], true);
    assert_eq!(body["reputation"]["verdict"], "safe");

    assert_eq!(get_as(&app, &code, DESKTOP_UA).await.status_code(), 307);
    assert_eq!(get_as(&app, &code, DESKTOP_UA).await.status_code(), 410);

    let body = preview(&app, &code).await;
    assert_eq!(body["is_active"], false, "{body}");
    assert_destination_withheld(&body, "preview-cap-secret");
}

#[tokio::test]
async fn preview_withholds_the_destination_of_an_expired_link() {
    let app = spawn_app().await;
    let token = register_verified(&app).await;
    let (id, code) = create_link(
        &app,
        &token,
        json!({ "original_url": "https://iana.org/preview-expired-secret" }),
    )
    .await;
    let model = links::Entity::find_by_id(id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    let mut active: links::ActiveModel = model.into();
    active.expires_at = Set(Some((Utc::now() - Duration::hours(1)).naive_utc()));
    active.update(&app.db).await.expect("expire link");

    assert_eq!(get_as(&app, &code, DESKTOP_UA).await.status_code(), 410);

    let body = preview(&app, &code).await;
    assert_eq!(body["is_expired"], true, "{body}");
    assert_eq!(body["is_active"], false, "{body}");
    assert_destination_withheld(&body, "preview-expired-secret");
}

#[tokio::test]
async fn preview_withholds_the_destination_of_a_blocked_link() {
    let app = spawn_app().await;
    let token = register_verified(&app).await;
    let host = unique_host();
    let (_id, code) = create_link(
        &app,
        &token,
        json!({ "original_url": format!("https://{host}/preview-blocked-secret") }),
    )
    .await;
    assert_eq!(preview(&app, &code).await["domain"], host.as_str());

    block_domain(&app, &host).await;
    assert_eq!(get_as(&app, &code, DESKTOP_UA).await.status_code(), 410);

    let body = preview(&app, &code).await;
    assert_eq!(body["is_active"], false, "{body}");
    assert_eq!(body["reputation"]["verdict"], "malicious", "{body}");
    assert_destination_withheld(&body, "preview-blocked-secret");
}

/// A one-time link's destination stays secret until the visit that burns it,
/// and the preview does not vouch for a URL it will not show.
#[tokio::test]
async fn preview_of_a_one_time_link_flags_it_and_vouches_for_nothing() {
    let app = spawn_app().await;
    let token = register_verified(&app).await;
    let (_id, code) = create_link(
        &app,
        &token,
        json!({
            "original_url": "https://iana.org/preview-burn-secret",
            "burn_after_reading": true,
            "safe_link_interstitial": true,
        }),
    )
    .await;

    let body = preview(&app, &code).await;
    assert_eq!(body["is_active"], true, "{body}");
    assert_eq!(body["burn_after_reading"], true, "{body}");
    assert_eq!(body["safe_link_interstitial"], true, "{body}");
    assert_eq!(body["reputation"]["verdict"], "unknown", "{body}");
    assert_destination_withheld(&body, "preview-burn-secret");

    // Once burned it is also reported as not active.
    let visit = app
        .server
        .get(&format!("/{code}"))
        .add_query_param("confirm", "1")
        .await;
    assert_eq!(visit.status_code(), 307);
    let body = preview(&app, &code).await;
    assert_eq!(body["is_active"], false, "{body}");
    assert_destination_withheld(&body, "preview-burn-secret");
}

#[tokio::test]
async fn preview_of_a_locked_link_vouches_for_nothing() {
    let app = spawn_app().await;
    let token = register_verified(&app).await;
    let (_id, code) = create_link(
        &app,
        &token,
        json!({ "original_url": "https://iana.org/preview-locked-secret", "password": "hunter2!" }),
    )
    .await;

    let body = preview(&app, &code).await;
    assert_eq!(body["has_password"], true);
    assert_eq!(body["reputation"]["verdict"], "unknown", "{body}");
    assert_destination_withheld(&body, "preview-locked-secret");
}

/// The interstitial vets the destination the visitor will be sent to, which is
/// the matching routing rule, not the link's fallback URL.
#[tokio::test]
async fn preview_of_a_routed_link_describes_where_this_visitor_goes() {
    let app = spawn_app().await;
    let token = register_verified(&app).await;
    let fallback = "https://iana.org/routed-fallback";
    let (id, code) = create_link(&app, &token, json!({ "original_url": fallback })).await;
    put_rules(
        &app,
        &token,
        id,
        json!([{ "destination_url": "https://iana.org/routed-mobile", "match_device": "Mobile", "priority": 0 }]),
    )
    .await;

    let mobile = preview_as(&app, &code, IPHONE_UA).await;
    assert_eq!(mobile["original_url"], "https://iana.org/routed-mobile");
    assert_eq!(mobile["reputation"]["verdict"], "safe");

    // No rule matches a desktop browser, so it is sent to the fallback.
    let desktop = preview_as(&app, &code, DESKTOP_UA).await;
    assert_eq!(desktop["original_url"], fallback);

    // The redirect agrees with what the preview said.
    let visit = get_as(&app, &code, IPHONE_UA).await;
    assert_eq!(
        location(&visit).as_deref(),
        Some("https://iana.org/routed-mobile")
    );
}

/// An A/B split picks at random per visit, so no preview can say where this
/// visitor will land; it must not name one of the candidates and vouch for it.
#[tokio::test]
async fn preview_of_an_ab_split_names_no_single_destination() {
    let app = spawn_app().await;
    let token = register_verified(&app).await;
    let (id, code) = create_link(
        &app,
        &token,
        json!({ "original_url": "https://iana.org/split-fallback" }),
    )
    .await;
    put_rules(
        &app,
        &token,
        id,
        json!([
            { "destination_url": "https://iana.org/split-a", "priority": 0, "weight": 1 },
            { "destination_url": "https://iana.org/split-b", "priority": 0, "weight": 1 },
        ]),
    )
    .await;

    for _ in 0..10 {
        let body = preview_as(&app, &code, DESKTOP_UA).await;
        assert_eq!(body["is_active"], true, "{body}");
        assert_eq!(body["reputation"]["verdict"], "unknown", "{body}");
        assert_destination_withheld(&body, "split-");
    }
}

/// A routing rule cannot dodge the blocklist at redirect time, and the preview
/// reports the same outcome.
#[tokio::test]
async fn preview_of_a_link_routed_to_a_blocked_destination_is_inactive() {
    let app = spawn_app().await;
    let token = register_verified(&app).await;
    let host = unique_host();
    let (id, code) = create_link(
        &app,
        &token,
        json!({ "original_url": "https://iana.org/routed-block-fallback" }),
    )
    .await;
    put_rules(
        &app,
        &token,
        id,
        json!([{ "destination_url": format!("https://{host}/routed-block-secret"), "match_device": "Mobile", "priority": 0 }]),
    )
    .await;
    block_domain(&app, &host).await;

    assert_eq!(get_as(&app, &code, IPHONE_UA).await.status_code(), 410);
    let mobile = preview_as(&app, &code, IPHONE_UA).await;
    assert_eq!(mobile["is_active"], false, "{mobile}");
    assert_destination_withheld(&mobile, "routed-block-secret");

    // Visitors the rule does not apply to are unaffected.
    assert_eq!(get_as(&app, &code, DESKTOP_UA).await.status_code(), 307);
    assert_eq!(
        preview_as(&app, &code, DESKTOP_UA).await["original_url"],
        "https://iana.org/routed-block-fallback"
    );
}

// ---------------------------------------------------------------------------
// QR
// ---------------------------------------------------------------------------

/// `a€bb` is six bytes but four characters; slicing it at byte 2 used to panic
/// the request task and drop the connection.
#[tokio::test]
async fn qr_with_multibyte_colours_falls_back_to_the_default() {
    let app = spawn_app().await;
    let token = register_verified(&app).await;
    let (id, _code) = create_link(
        &app,
        &token,
        json!({ "original_url": "https://iana.org/qr-multibyte" }),
    )
    .await;

    // Control: a well-formed colour is honoured.
    let res = app
        .server
        .get(&format!("/links/{id}/qr?color=2f37d8"))
        .authorization_bearer(&token)
        .await;
    assert_eq!(res.status_code(), 200);

    for query in [
        "color=a%E2%82%ACbb",
        "bg=a%E2%82%ACbb",
        "color=%C3%A9%C3%A9%C3%A9",
        "color=a%E2%82%ACbb&bg=%E2%82%AC%E2%82%AC",
        "color=a%E2%82%ACbb&format=svg",
    ] {
        let res = app
            .server
            .get(&format!("/links/{id}/qr?{query}"))
            .authorization_bearer(&token)
            .await;
        assert_eq!(res.status_code(), 200, "{query}: {}", res.text());
    }
}
