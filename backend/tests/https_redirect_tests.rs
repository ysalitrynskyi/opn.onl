//! FORCE_HTTPS must not build Location from the client-supplied Host header.
//! nginx forwards `$host` and the backend binds 0.0.0.0, so nothing upstream
//! pins Host; the redirect target has to come from BASE_URL / FRONTEND_URL.

mod common;

use axum::http::header::{HOST, LOCATION};

/// These tests mutate process-wide env that the HTTPS-redirect middleware
/// reads per request. Serialize them so they cannot interleave with each
/// other — including `spawn_real_app`, which pins FORCE_HTTPS=false.
static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct RedirectEnv {
    _guard: tokio::sync::MutexGuard<'static, ()>,
    force: Option<String>,
    base: Option<String>,
    frontend: Option<String>,
}

impl RedirectEnv {
    async fn lock() -> Self {
        let _guard = ENV_LOCK.lock().await;
        Self {
            _guard,
            force: std::env::var("FORCE_HTTPS").ok(),
            base: std::env::var("BASE_URL").ok(),
            frontend: std::env::var("FRONTEND_URL").ok(),
        }
    }

    fn apply(&self, force_https: &str, base_url: Option<&str>, frontend_url: Option<&str>) {
        common::set_env("FORCE_HTTPS", force_https);
        match base_url {
            Some(v) => common::set_env("BASE_URL", v),
            None => common::remove_env("BASE_URL"),
        }
        match frontend_url {
            Some(v) => common::set_env("FRONTEND_URL", v),
            None => common::remove_env("FRONTEND_URL"),
        }
    }
}

impl Drop for RedirectEnv {
    fn drop(&mut self) {
        restore("FORCE_HTTPS", self.force.as_deref(), Some("false"));
        restore("BASE_URL", self.base.as_deref(), None);
        restore("FRONTEND_URL", self.frontend.as_deref(), None);
    }
}

fn restore(key: &str, previous: Option<&str>, fallback: Option<&str>) {
    match previous {
        Some(v) => common::set_env(key, v),
        None => match fallback {
            Some(v) => common::set_env(key, v),
            None => common::remove_env(key),
        },
    }
}

#[tokio::test]
async fn force_https_redirect_ignores_request_host_when_base_url_is_set() {
    let env = RedirectEnv::lock().await;
    let (server, _db) = common::spawn_real_app().await;
    env.apply("true", Some("https://opn.onl"), Some("https://opn.onl"));

    let token = "eyJhbGciOiJIUzI1NiJ9.payload.signature";
    let res = server
        .get(&format!("/ws?token={token}"))
        .add_header(HOST, "evil.example")
        .await;

    assert_eq!(
        res.status_code(),
        308,
        "FORCE_HTTPS without X-Forwarded-Proto=https must redirect: {}",
        res.text()
    );
    let location = res
        .headers()
        .get(LOCATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        !location.contains("evil.example"),
        "Location must not use the request Host: {location}"
    );
    assert!(
        location.starts_with("https://opn.onl/"),
        "Location must use BASE_URL host: {location}"
    );
    assert!(
        location.contains("/ws"),
        "Location must keep the original path: {location}"
    );
}

#[tokio::test]
async fn force_https_falls_back_to_request_host_when_no_public_url() {
    let env = RedirectEnv::lock().await;
    let (server, _db) = common::spawn_real_app().await;
    env.apply("true", None, None);

    let res = server
        .get("/auth/settings")
        .add_header(HOST, "selfhost.example:3000")
        .await;

    assert_eq!(
        res.status_code(),
        308,
        "got {}: {}",
        res.status_code(),
        res.text()
    );
    let location = res
        .headers()
        .get(LOCATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(
        location, "https://selfhost.example:3000/auth/settings",
        "self-hoster with no BASE_URL/FRONTEND_URL still uses request Host"
    );
}

#[tokio::test]
async fn force_https_skips_redirect_when_forwarded_proto_is_https() {
    let env = RedirectEnv::lock().await;
    let (server, _db) = common::spawn_real_app().await;
    env.apply("true", Some("https://opn.onl"), None);

    let res = server
        .get("/health")
        .add_header(HOST, "evil.example")
        .add_header("x-forwarded-proto", "https")
        .await;

    assert_eq!(
        res.status_code(),
        200,
        "TLS-terminated request must not redirect: {}",
        res.text()
    );
}

/// The container health probe asks over plain HTTP and cannot send
/// X-Forwarded-Proto. A 308 would read as success to `curl -f`, so the probe
/// has to get the endpoint's own answer.
#[tokio::test]
async fn health_is_exempt_from_the_https_redirect() {
    let env = RedirectEnv::lock().await;
    let (server, _db) = common::spawn_real_app().await;
    env.apply("true", Some("https://opn.onl"), Some("https://opn.onl"));

    let res = server.get("/health").await;

    assert_eq!(
        res.status_code(),
        200,
        "plain-HTTP health probe must not be redirected: {}",
        res.text()
    );
    assert!(res.headers().get(LOCATION).is_none());
    let body: serde_json::Value = res.json();
    assert_eq!(body["status"], "healthy");
    assert_eq!(body["database"], "connected");
}

/// With the database gone the probe has to fail. It used to get the redirect
/// instead, and a dead database looked healthy.
#[tokio::test]
async fn health_reports_a_dead_database_even_under_force_https() {
    use opn_onl_backend::{AppState, build_router};

    let env = RedirectEnv::lock().await;
    let db = common::setup_test_db().await;
    let state = AppState::for_tests(db.clone()).await;
    let server = axum_test::TestServer::new(build_router(state)).expect("test server");
    env.apply("true", Some("https://opn.onl"), Some("https://opn.onl"));

    // This pool is private to the test; closing it closes every clone of it,
    // including the one the router holds.
    db.close().await.expect("close pool");

    let res = server.get("/health").await;

    assert_eq!(
        res.status_code(),
        503,
        "a probe must see the failure, not a redirect: {}",
        res.text()
    );
    let body: serde_json::Value = res.json();
    assert_eq!(body["status"], "unhealthy");
}

/// Control: the exemption is for `/health` only. Everything else still
/// redirects, including paths that merely start with it.
#[tokio::test]
async fn only_health_itself_is_exempt() {
    let env = RedirectEnv::lock().await;
    let (server, _db) = common::spawn_real_app().await;
    env.apply("true", Some("https://opn.onl"), Some("https://opn.onl"));

    for path in ["/auth/settings", "/healthz", "/health/x", "/links"] {
        let res = server.get(path).await;
        assert_eq!(res.status_code(), 308, "{path}: {}", res.text());
    }
}

/// FORCE_HTTPS used to be switched on only by an exact lower-case `true` or
/// `1`, so `True` or `yes` left a production instance serving plain HTTP.
#[tokio::test]
async fn force_https_accepts_the_spellings_an_operator_types() {
    let env = RedirectEnv::lock().await;
    let (server, _db) = common::spawn_real_app().await;

    // Few requests: the per-second limiter allows ten. The full spelling table
    // is covered where the flag is parsed (`utils::env`).
    for on in ["True", "TRUE", "1", "yes"] {
        env.apply(on, Some("https://opn.onl"), None);
        let res = server.get("/auth/settings").await;
        assert_eq!(res.status_code(), 308, "FORCE_HTTPS={on}");
    }
    for off in ["False", "0", "off", ""] {
        env.apply(off, Some("https://opn.onl"), None);
        let res = server.get("/auth/settings").await;
        assert_eq!(res.status_code(), 200, "FORCE_HTTPS={off:?}");
    }
}

#[test]
fn https_redirect_host_prefers_configured_urls_over_request_host() {
    assert_eq!(
        opn_onl_backend::https_redirect_host(
            Some("https://opn.onl"),
            Some("https://app.example"),
            Some("evil.example"),
        ),
        "opn.onl"
    );
    assert_eq!(
        opn_onl_backend::https_redirect_host(
            None,
            Some("https://opn.onl:443"),
            Some("evil.example"),
        ),
        "opn.onl:443"
    );
    assert_eq!(
        opn_onl_backend::https_redirect_host(None, None, Some("selfhost.example")),
        "selfhost.example"
    );
    assert_eq!(
        opn_onl_backend::https_redirect_host(Some("  "), Some(""), Some("selfhost.example")),
        "selfhost.example"
    );
    assert_eq!(
        opn_onl_backend::https_redirect_host(None, None, None),
        "localhost"
    );
}
