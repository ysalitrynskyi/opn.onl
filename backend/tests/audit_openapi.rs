//! Regression test for the OpenAPI document. The audit registered several
//! handlers that shipped without `#[utoipa::path]` (api-keys, passkeys,
//! link-in-bio), so the published spec silently omitted them. This drives the
//! real router, fetches the served spec, and asserts those paths are now
//! present — and, implicitly, that `ApiDoc::openapi()` still builds (a bad
//! annotation would fail the build before this test could run).

mod common;

use common::spawn_real_app;
use serde_json::Value;

#[tokio::test]
async fn openapi_spec_serves_and_documents_newly_registered_handlers() {
    let (server, _db) = spawn_real_app().await;

    let res = server.get("/api-docs/openapi.json").await;
    assert_eq!(res.status_code(), 200, "openapi.json must be served");

    let spec: Value = res.json();
    assert!(spec.get("openapi").is_some(), "must be an OpenAPI document");
    let paths = spec["paths"].as_object().expect("spec has a paths object");

    // Previously-omitted handlers that the audit annotated and registered.
    for path in [
        "/auth/api-keys",
        "/auth/api-keys/{id}",
        "/auth/passkey/register/start",
        "/auth/passkey/register/finish",
        "/auth/passkey/login/start",
        "/auth/passkey/login/finish",
        "/auth/passkeys",
        "/auth/passkey/delete",
        "/auth/passkey/rename",
        "/auth/bio",
        "/api/bio/{username}",
    ] {
        assert!(
            paths.contains_key(path),
            "OpenAPI spec must document {path}; present paths: {:?}",
            paths.keys().collect::<Vec<_>>()
        );
    }

    let schemas = spec["components"]["schemas"]
        .as_object()
        .expect("spec must have components.schemas");

    // PasskeyAuthResponse was renamed specifically to avoid colliding with
    // auth::AuthResponse as a schema key — both must remain distinct.
    assert!(schemas.contains_key("AuthResponse"));
    assert!(schemas.contains_key("PasskeyAuthResponse"));
    assert_ne!(
        schemas["AuthResponse"], schemas["PasskeyAuthResponse"],
        "AuthResponse and PasskeyAuthResponse must not collapse into one schema"
    );

    // MessageResponse is $ref'd by several auth success responses. It was
    // missing from ApiDoc's schemas(...) — a dangling ref in the published
    // spec. Keep it registered.
    assert!(
        schemas.contains_key("MessageResponse"),
        "MessageResponse must be registered — auth paths $ref it"
    );
    let verify = &spec["paths"]["/auth/verify-email"]["post"]["responses"]["200"];
    let verify_ref = verify["content"]["application/json"]["schema"]["$ref"]
        .as_str()
        .unwrap_or("");
    assert_eq!(
        verify_ref, "#/components/schemas/MessageResponse",
        "verify-email success must $ref MessageResponse"
    );
}
