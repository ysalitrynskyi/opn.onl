//! Regression test for the bio avatar proxy (audit privacy finding: rendering a
//! user-supplied external avatar as <img src> leaked the visitor's IP to the
//! avatar host). The proxy fetches server-side behind the SSRF guard; this pins
//! the guard: invalid URLs are rejected and internal targets can't be reached.

mod common;

use common::spawn_real_app;

#[tokio::test]
async fn avatar_proxy_rejects_bad_and_internal_urls() {
    let (server, _db) = spawn_real_app().await;

    // Not a URL → 400 (validate_url).
    let res = server.get("/api/bio/avatar?url=not-a-url").await;
    assert_eq!(res.status_code(), 400, "malformed URL must be rejected");

    // Internal/loopback and cloud-metadata targets must be refused — whether at
    // validation (raw-IP hosts are rejected, 400) or by the SSRF guard on connect
    // (502). The important property is that the proxy never reaches the address,
    // so it can't be turned into an SSRF into the private network.
    for internal in [
        "http%3A%2F%2F127.0.0.1%2Favatar.png",
        "http%3A%2F%2F169.254.169.254%2Flatest%2Fmeta-data%2F",
    ] {
        let status = server
            .get(&format!("/api/bio/avatar?url={internal}"))
            .await
            .status_code()
            .as_u16();
        assert!(
            status >= 400,
            "internal target {internal} must be refused, got {status}"
        );
    }
}
