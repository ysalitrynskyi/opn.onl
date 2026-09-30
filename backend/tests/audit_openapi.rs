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
        "/api/bio/avatar",
        "/links/{id}/rules",
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

/// The version used to be a string literal in the `#[openapi(info(...))]`
/// attribute, so it drifted the moment a release bumped Cargo.toml without
/// touching it — 1.3.0 shipped with the spec still advertising 1.2.1.
#[tokio::test]
async fn openapi_spec_reports_the_crate_version() {
    let (server, _db) = spawn_real_app().await;

    let spec: Value = server.get("/api-docs/openapi.json").await.json();

    assert_eq!(
        spec["info"]["version"].as_str(),
        Some(env!("CARGO_PKG_VERSION")),
        "served spec must advertise the crate version, not a hand-maintained literal"
    );
}

use std::collections::{BTreeMap, BTreeSet};

/// Paths the router serves that are not part of the published API contract.
/// Keep this list explicit — a new route that is not here must appear in
/// `openapi.rs` `paths(...)` or this test fails.
fn is_unpublished(path: &str) -> bool {
    matches!(path, "/health" | "/ws" | "/sse" | "/api-docs/openapi.json")
        || path == "/"
        || path.starts_with("/swagger-ui")
        || path.contains("__private__")
}

fn axum_path_to_openapi(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut chars = path.chars().peekable();
    while let Some(c) = chars.next() {
        if c == ':' {
            let mut name = String::new();
            while let Some(&n) = chars.peek() {
                if n.is_ascii_alphanumeric() || n == '_' {
                    name.push(n);
                    chars.next();
                } else {
                    break;
                }
            }
            out.push('{');
            out.push_str(&name);
            out.push('}');
        } else if c == '*' {
            // axum catch-all `*rest` / `{*rest}`
            let mut name = String::new();
            while let Some(&n) = chars.peek() {
                if n.is_ascii_alphanumeric() || n == '_' {
                    name.push(n);
                    chars.next();
                } else {
                    break;
                }
            }
            out.push('{');
            if !name.is_empty() {
                out.push_str(&name);
            } else {
                out.push('*');
            }
            out.push('}');
        } else {
            out.push(c);
        }
    }
    out
}

/// `(METHOD, /path)` pairs the live `build_router()` actually serves.
/// Parsed from axum 0.7's `Debug` impl (`PathRouter.node.paths` +
/// `MethodRouter` get/post/put/delete/patch). HEAD/OPTIONS/TRACE/CONNECT
/// are ignored — GET handlers also accept HEAD, and CORS answers OPTIONS.
fn operations_from_router_debug(debug: &str) -> BTreeSet<(String, String)> {
    let path_router = debug.split("fallback_router:").next().unwrap_or(debug);

    let mut id_to_path: BTreeMap<u32, String> = BTreeMap::new();
    let mut rest = path_router;
    while let Some(idx) = rest.find("RouteId(") {
        rest = &rest[idx + 8..];
        let Some(end) = rest.find(')') else { break };
        let Ok(id) = rest[..end].parse::<u32>() else {
            rest = &rest[end + 1..];
            continue;
        };
        rest = &rest[end + 1..];
        let trimmed = rest.trim_start();
        if let Some(after_colon) = trimmed.strip_prefix(':') {
            let after_colon = after_colon.trim_start();
            if let Some(quoted) = after_colon.strip_prefix('"')
                && let Some(qend) = quoted.find('"')
            {
                let path = quoted[..qend].to_string();
                if !path.contains("__private__") {
                    id_to_path.insert(id, axum_path_to_openapi(&path));
                }
                rest = &quoted[qend + 1..];
                continue;
            }
        }
    }

    let mut ops = BTreeSet::new();
    let mut rest = path_router;
    while let Some(idx) = rest.find("RouteId(") {
        rest = &rest[idx + 8..];
        let Some(end) = rest.find(')') else { break };
        let Ok(id) = rest[..end].parse::<u32>() else {
            rest = &rest[end + 1..];
            continue;
        };
        rest = &rest[end + 1..];
        let Some(path) = id_to_path.get(&id).cloned() else {
            continue;
        };
        let trimmed = rest.trim_start();
        let Some(after_colon) = trimmed.strip_prefix(':') else {
            continue;
        };
        let after_colon = after_colon.trim_start();
        if after_colon.starts_with("MethodRouter(") {
            let body = after_colon;
            let limit = body.find("allow_header:").unwrap_or(body.len().min(800));
            let body = &body[..limit];
            let field = |name: &str| -> bool {
                let needle = format!("{name}: ");
                if let Some(p) = body.find(&needle) {
                    let v = body[p + needle.len()..].trim_start();
                    v.starts_with("Route") || v.starts_with("BoxedHandler")
                } else {
                    false
                }
            };
            if field("get") {
                ops.insert(("GET".into(), path.clone()));
            }
            if field("delete") {
                ops.insert(("DELETE".into(), path.clone()));
            }
            if field("patch") {
                ops.insert(("PATCH".into(), path.clone()));
            }
            if field("post") {
                ops.insert(("POST".into(), path.clone()));
            }
            if field("put") {
                ops.insert(("PUT".into(), path.clone()));
            }
        } else if after_colon.starts_with("Route(") {
            ops.insert(("GET".into(), path));
        }
    }

    ops
}

fn operations_from_openapi(spec: &Value) -> BTreeSet<(String, String)> {
    let mut ops = BTreeSet::new();
    let Some(paths) = spec["paths"].as_object() else {
        return ops;
    };
    for (path, item) in paths {
        let Some(item) = item.as_object() else {
            continue;
        };
        for method in ["get", "post", "put", "delete", "patch"] {
            if item.get(method).is_some() {
                ops.insert((method.to_ascii_uppercase(), path.clone()));
            }
        }
    }
    ops
}

/// The published document and `build_router()` must describe the same HTTP
/// surface. Documented (method, path) pairs must be served; served pairs that
/// are not documented must sit on the unpublished allowlist.
#[tokio::test]
async fn openapi_document_matches_the_router() {
    let db = common::setup_test_db().await;
    let state = opn_onl_backend::AppState::for_tests(db).await;
    let router = opn_onl_backend::build_router(state);

    let router_debug = format!("{router:?}");
    let served = operations_from_router_debug(&router_debug);
    assert!(
        !served.is_empty(),
        "failed to parse any routes out of Router Debug:\n{}",
        router_debug.chars().take(2000).collect::<String>()
    );

    let spec = serde_json::to_value(opn_onl_backend::openapi::api_doc())
        .expect("api_doc() must serialize");
    let documented = operations_from_openapi(&spec);
    assert!(
        !documented.is_empty(),
        "api_doc() produced no paths: {spec}"
    );

    let missing_from_router: Vec<_> = documented.difference(&served).cloned().collect();
    assert!(
        missing_from_router.is_empty(),
        "OpenAPI documents routes the router does not serve: {missing_from_router:?}\nserved={served:?}"
    );

    let undocumented: Vec<_> = served
        .iter()
        .filter(|(method, path)| {
            !documented.contains(&(method.clone(), path.clone())) && !is_unpublished(path)
        })
        .cloned()
        .collect();
    assert!(
        undocumented.is_empty(),
        "router serves undocumented API routes (add a #[utoipa::path] + paths(...) entry, or put the path on the unpublished allowlist): {undocumented:?}"
    );
}

/// Every `$ref` in the published document must resolve, and registered object
/// schemas must have properties — an unregistered ToSchema type renders as `{{}}`.
#[tokio::test]
async fn openapi_schema_refs_resolve() {
    let spec = serde_json::to_value(opn_onl_backend::openapi::api_doc())
        .expect("api_doc() must serialize");
    let schemas = spec["components"]["schemas"]
        .as_object()
        .expect("spec must have components.schemas");

    fn collect_refs(value: &Value, out: &mut BTreeSet<String>) {
        match value {
            Value::Object(map) => {
                if let Some(Value::String(r)) = map.get("$ref") {
                    out.insert(r.clone());
                }
                for v in map.values() {
                    collect_refs(v, out);
                }
            }
            Value::Array(items) => {
                for v in items {
                    collect_refs(v, out);
                }
            }
            _ => {}
        }
    }

    let mut refs = BTreeSet::new();
    collect_refs(&spec, &mut refs);
    let mut dangling = Vec::new();
    for r in &refs {
        let Some(name) = r.strip_prefix("#/components/schemas/") else {
            continue;
        };
        if !schemas.contains_key(name) {
            dangling.push(r.clone());
        }
    }
    assert!(
        dangling.is_empty(),
        "published spec $ref's schemas that are not registered: {dangling:?}"
    );

    let mut empty = Vec::new();
    for (name, schema) in schemas {
        let Some(obj) = schema.as_object() else {
            continue;
        };
        let is_object = obj.get("type").and_then(|t| t.as_str()) == Some("object");
        let no_props = obj
            .get("properties")
            .and_then(|p| p.as_object())
            .map(|p| p.is_empty())
            .unwrap_or(true);
        let no_additional = obj.get("additionalProperties").is_none();
        if is_object && no_props && no_additional {
            empty.push(name.clone());
        }
    }
    assert!(
        empty.is_empty(),
        "object schemas with no properties (unregistered or empty ToSchema): {empty:?}"
    );
}

/// The redirect answers `307 Temporary Redirect` (`Redirect::temporary`); the
/// published spec said 302, so generated clients expected the wrong status.
#[test]
fn redirect_is_documented_as_307() {
    let doc = serde_json::to_value(opn_onl_backend::openapi::api_doc()).unwrap();
    let responses = &doc["paths"]["/{code}"]["get"]["responses"];
    assert!(responses.get("307").is_some(), "{responses}");
    assert!(responses.get("302").is_none(), "{responses}");
}
