//! Every API response carries nosniff and a frame ban; HSTS only when the
//! request came over HTTPS. The API host is reached without the frontend's
//! nginx, so nothing else adds them. Real router.

mod common;

use axum::http::{HeaderName, HeaderValue, Method, header};
use common::spawn_real_app;

fn header_value(res: &axum_test::TestResponse, name: HeaderName) -> Option<String> {
    res.headers()
        .get(&name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

#[tokio::test]
async fn responses_carry_security_headers_and_hsts_only_over_https() {
    let (server, _db) = spawn_real_app().await;

    let plain = server.get("/health").await;
    assert_eq!(plain.status_code(), 200, "{}", plain.text());
    assert_eq!(
        header_value(&plain, header::X_CONTENT_TYPE_OPTIONS).as_deref(),
        Some("nosniff")
    );
    assert_eq!(
        header_value(&plain, header::X_FRAME_OPTIONS).as_deref(),
        Some("DENY")
    );
    assert_eq!(
        header_value(&plain, header::STRICT_TRANSPORT_SECURITY),
        None,
        "HSTS must not be sent on a plain-HTTP request"
    );
    let body: serde_json::Value = plain.json();
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"), "{body}");

    let https = server
        .get("/health")
        .add_header(
            HeaderName::from_static("x-forwarded-proto"),
            HeaderValue::from_static("https"),
        )
        .await;
    assert!(
        header_value(&https, header::STRICT_TRANSPORT_SECURITY)
            .is_some_and(|v| v.starts_with("max-age=")),
        "HSTS missing behind an HTTPS proxy"
    );

    // Responses produced by outer middleware get them too.
    let missing = server.get("/no-such-code-xyz").await;
    assert_eq!(
        header_value(&missing, header::X_CONTENT_TYPE_OPTIONS).as_deref(),
        Some("nosniff")
    );
    let preflight = server
        .method(Method::OPTIONS, "/links")
        .add_header(
            header::ORIGIN,
            HeaderValue::from_static("https://opn.example"),
        )
        .add_header(
            header::ACCESS_CONTROL_REQUEST_METHOD,
            HeaderValue::from_static("POST"),
        )
        .await;
    assert_eq!(
        header_value(&preflight, header::X_FRAME_OPTIONS).as_deref(),
        Some("DENY")
    );
}
