//! CORS behaviour the split-origin deployment depends on: the frontend and the
//! API are different origins, so every authenticated call is cross-origin.
//! Built from the real `build_cors()` layer. This binary owns its environment,
//! so setting FRONTEND_URL here cannot race another test.

mod common;

use axum::http::{HeaderName, HeaderValue, Method, StatusCode, header};
use axum::{Router, routing::get};

const ORIGIN: &str = "https://app.opn.example";

fn server() -> axum_test::TestServer {
    common::set_env("FRONTEND_URL", ORIGIN);
    common::remove_env("BASE_URL");
    let app = Router::new()
        .route("/links", get(|| async { "ok" }))
        .layer(opn_onl_backend::build_cors());
    axum_test::TestServer::new(app).unwrap()
}

fn header_value(res: &axum_test::TestResponse, name: HeaderName) -> String {
    res.headers()
        .get(&name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// `Access-Control-Allow-Headers: *` does not cover `Authorization` under the
/// Fetch spec, so the preflight must name it.
#[tokio::test]
async fn preflight_names_authorization() {
    let server = server();
    let preflight = server
        .method(Method::OPTIONS, "/links")
        .add_header(header::ORIGIN, HeaderValue::from_static(ORIGIN))
        .add_header(
            header::ACCESS_CONTROL_REQUEST_METHOD,
            HeaderValue::from_static("GET"),
        )
        .add_header(
            header::ACCESS_CONTROL_REQUEST_HEADERS,
            HeaderValue::from_static("authorization,content-type"),
        )
        .await;
    assert_eq!(preflight.status_code(), StatusCode::OK);
    let allowed = header_value(&preflight, header::ACCESS_CONTROL_ALLOW_HEADERS);
    assert!(
        allowed.split(',').any(|h| h.trim() == "authorization"),
        "preflight must name authorization explicitly, got {allowed:?}"
    );
}

/// The app reads Retry-After to tell the user how long to wait; a cross-origin
/// response only shows it to scripts when it is exposed.
#[tokio::test]
async fn responses_expose_the_rate_limit_headers() {
    let server = server();
    let res = server
        .get("/links")
        .add_header(header::ORIGIN, HeaderValue::from_static(ORIGIN))
        .await;
    assert_eq!(
        header_value(&res, header::ACCESS_CONTROL_ALLOW_ORIGIN),
        ORIGIN.to_ascii_lowercase()
    );
    let exposed = header_value(&res, header::ACCESS_CONTROL_EXPOSE_HEADERS);
    for name in ["retry-after", "x-ratelimit-limit", "x-ratelimit-remaining"] {
        assert!(
            exposed.split(',').any(|h| h.trim() == name),
            "{name} must be exposed, got {exposed:?}"
        );
    }
}
