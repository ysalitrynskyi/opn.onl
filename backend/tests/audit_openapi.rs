//! Regression tests for the OpenAPI document. The audit registered several
//! handlers that shipped without `#[utoipa::path]` (api-keys, passkeys,
//! link-in-bio), so the published spec silently omitted them. This drives the
//! real router, fetches the served spec, and asserts those paths are now
//! present — and, implicitly, that `ApiDoc::openapi()` still builds (a bad
//! annotation would fail the build before this test could run).
//!
//! The 1.3.2 document also had dangling `$ref`s, declared no security on 58 of
//! its 101 operations, and never mentioned rate limiting. No OpenAPI validator
//! ships with the toolchain, so the structural checks below stand in for one.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use axum::http::{Method, StatusCode};
use axum_test::TestServer;
use common::{mark_email_verified, spawn_real_app, unique_email};
use opn_onl_backend::utils::rate_limiter::{
    RateLimitConfig, RateLimitResult, RateLimiter, RateLimiters,
};
use sea_orm::DatabaseConnection;
use serde_json::{Map, Value, json};

/// The document exactly as `build_router()` serves it.
async fn served_spec() -> Value {
    let (server, _db) = spawn_real_app().await;
    let res = server.get("/api-docs/openapi.json").await;
    assert_eq!(res.status_code(), 200, "openapi.json must be served");
    res.json()
}

/// Follow `$ref`s to the object they name. `None` when the value is absent or
/// a reference dangles.
fn resolve<'a>(spec: &'a Value, mut value: &'a Value) -> Option<&'a Value> {
    while let Some(reference) = value.get("$ref").and_then(Value::as_str) {
        value = spec.pointer(reference.strip_prefix('#')?)?;
    }
    (!value.is_null()).then_some(value)
}

/// `(METHOD, path, operation)` for every operation in the document.
fn documented_operations(spec: &Value) -> Vec<(String, String, &Value)> {
    let mut ops = Vec::new();
    for (path, item) in spec["paths"].as_object().expect("spec has a paths object") {
        for method in [
            "get", "put", "post", "delete", "options", "head", "patch", "trace",
        ] {
            if let Some(op) = item.get(method) {
                ops.push((method.to_ascii_uppercase(), path.clone(), op));
            }
        }
    }
    ops
}

/// The scheme names of each security requirement. An empty set is the empty
/// requirement `{}`, which makes credentials optional.
fn security_requirements(op: &Value) -> Vec<BTreeSet<String>> {
    op["security"]
        .as_array()
        .map(|requirements| {
            requirements
                .iter()
                .map(|r| {
                    r.as_object()
                        .into_iter()
                        .flatten()
                        .map(|(k, _)| k.clone())
                        .collect()
                })
                .collect()
        })
        .unwrap_or_default()
}

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

/// Every `$ref` in the served document must resolve to a component, and
/// registered object schemas must have properties — an unregistered ToSchema
/// type renders as `{}`. The 1.3.2 document had 28 dangling references.
#[tokio::test]
async fn openapi_refs_resolve() {
    let spec = served_spec().await;
    let schemas = spec["components"]["schemas"]
        .as_object()
        .expect("spec must have components.schemas");

    fn collect_refs(value: &Value, at: &str, out: &mut BTreeMap<String, String>) {
        match value {
            Value::Object(map) => {
                if let Some(Value::String(r)) = map.get("$ref") {
                    out.entry(r.clone()).or_insert_with(|| at.to_string());
                }
                for (key, v) in map {
                    collect_refs(v, &format!("{at}/{key}"), out);
                }
            }
            Value::Array(items) => {
                for (i, v) in items.iter().enumerate() {
                    collect_refs(v, &format!("{at}/{i}"), out);
                }
            }
            _ => {}
        }
    }

    let mut refs = BTreeMap::new();
    collect_refs(&spec, "#", &mut refs);
    assert!(!refs.is_empty(), "the document uses no $ref at all: {spec}");
    let dangling: Vec<String> = refs
        .iter()
        .filter(|(r, _)| {
            !r.strip_prefix('#')
                .filter(|pointer| pointer.starts_with("/components/"))
                .is_some_and(|pointer| spec.pointer(pointer).is_some())
        })
        .map(|(r, at)| format!("{r} (first used at {at})"))
        .collect();
    assert!(
        dangling.is_empty(),
        "the served spec has $refs that name no component:\n{}",
        dangling.join("\n")
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
#[tokio::test]
async fn redirect_is_documented_as_307() {
    let spec = served_spec().await;
    let responses = &spec["paths"]["/{code}"]["get"]["responses"];
    assert!(responses.get("307").is_some(), "{responses}");
    assert!(responses.get("302").is_none(), "{responses}");
}

// ---------------------------------------------------------------------------
// Security
// ---------------------------------------------------------------------------

/// Operations anyone may call without credentials. Every other operation must
/// name a bearer scheme in `security`, so a new protected route that forgets
/// it fails `protected_operations_declare_bearer_security`. Adding a route here
/// is the decision that it is public.
const PUBLIC_OPERATIONS: &[(&str, &str)] = &[
    ("POST", "/auth/register"),
    ("POST", "/auth/login"),
    ("POST", "/auth/verify-email"),
    ("POST", "/auth/resend-verification"),
    ("POST", "/auth/forgot-password"),
    ("POST", "/auth/reset-password"),
    ("GET", "/auth/settings"),
    ("POST", "/auth/passkey/login/start"),
    ("POST", "/auth/passkey/login/finish"),
    ("GET", "/api/bio/{username}"),
    ("GET", "/api/bio/avatar"),
    ("GET", "/links/check-code"),
    ("POST", "/links/build-utm"),
    ("POST", "/contact"),
    ("GET", "/{code}"),
    ("GET", "/{code}/preview"),
    ("POST", "/{code}/verify"),
];

/// Operations that work with or without credentials: `security` must hold the
/// empty requirement `{}` next to the bearer ones.
const OPTIONAL_AUTH_OPERATIONS: &[(&str, &str)] = &[("POST", "/links")];

#[tokio::test]
async fn protected_operations_declare_bearer_security() {
    let spec = served_spec().await;
    assert!(
        spec.get("security").is_none(),
        "security is declared per operation; a document-wide requirement would \
         also cover the public operations"
    );
    let schemes = spec["components"]["securitySchemes"]
        .as_object()
        .expect("spec must define components.securitySchemes");
    let is_bearer = |name: &str| {
        schemes.get(name).is_some_and(|scheme| {
            scheme["type"] == "http"
                && scheme["scheme"]
                    .as_str()
                    .is_some_and(|s| s.eq_ignore_ascii_case("bearer"))
        })
    };

    let operations = documented_operations(&spec);
    for listed in PUBLIC_OPERATIONS.iter().chain(OPTIONAL_AUTH_OPERATIONS) {
        assert!(
            operations
                .iter()
                .any(|(method, path, _)| (method.as_str(), path.as_str()) == *listed),
            "{listed:?} is listed as public or optional but is not documented"
        );
    }

    let mut wrong = Vec::new();
    for (method, path, op) in &operations {
        let key = (method.as_str(), path.as_str());
        let requirements = security_requirements(op);
        let anonymous = requirements.iter().any(BTreeSet::is_empty);
        let bearer = requirements
            .iter()
            .any(|r| !r.is_empty() && r.iter().all(|name| is_bearer(name)));
        for name in requirements.iter().flatten() {
            if !schemes.contains_key(name) {
                wrong.push(format!("{method} {path}: names undefined scheme {name}"));
            }
        }
        if PUBLIC_OPERATIONS.contains(&key) {
            if !requirements.is_empty() {
                wrong.push(format!(
                    "{method} {path}: public but declares {requirements:?}"
                ));
            }
        } else if OPTIONAL_AUTH_OPERATIONS.contains(&key) {
            if !(anonymous && bearer) {
                wrong.push(format!(
                    "{method} {path}: credentials are optional, so security needs {{}} and a \
                     bearer scheme, not {requirements:?}"
                ));
            }
        } else if anonymous || !bearer {
            wrong.push(format!(
                "{method} {path}: requires authentication but declares {requirements:?}"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "security declarations:\n{}",
        wrong.join("\n")
    );
}

/// Public operations whose handlers check a credential sent in the body, so
/// a placeholder body is answered 401 although no bearer token is needed.
const CHECKS_CREDENTIALS_IN_BODY: &[(&str, &str)] = &[
    ("POST", "/auth/login"),
    ("POST", "/auth/passkey/login/finish"),
];

/// The real router with budgets no sweep can spend, and every optional feature
/// switched on so that each handler gets as far as its credential check.
async fn spawn_app_without_rate_limits() -> (TestServer, DatabaseConnection) {
    // Same environment pinning as `common::spawn_real_app`.
    // FIXME: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("FORCE_HTTPS", "false") };
    // FIXME: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("TRUST_PROXY_HEADERS", "false") };
    if std::env::var("JWT_SECRET").is_err() {
        // FIXME: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("JWT_SECRET", "integration-test-secret-0123456789abcdef") };
    }
    // Account deletion is off by default and refuses before it reads the
    // token; a developer .env could switch the others off. The probe never
    // sends a session token, so it cannot delete anything.
    for flag in [
        "ENABLE_ACCOUNT_DELETION",
        "ENABLE_API_KEYS",
        "ENABLE_PASSKEYS",
        "ENABLE_LINK_IN_BIO",
    ] {
        // FIXME: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var(flag, "true") };
    }

    let db = common::setup_test_db().await;
    let mut state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    let roomy = || Arc::new(RateLimiter::new(RateLimitConfig::new(1_000_000, 60)));
    state.rate_limiters = Arc::new(RateLimiters {
        per_second: roomy(),
        general: roomy(),
        link_creation: roomy(),
        auth: roomy(),
        redirect: roomy(),
        password_verify: roomy(),
        password_verify_ip: roomy(),
        contact: roomy(),
    });
    let server =
        TestServer::new(opn_onl_backend::build_router(state)).expect("failed to start test server");
    (server, db)
}

/// A verified account's API key, created through the API.
async fn api_key_for_new_account(server: &TestServer, db: &DatabaseConnection) -> String {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let account: Value = res.json();
    let jwt = account["token"].as_str().expect("token");
    mark_email_verified(db, account["user_id"].as_i64().expect("user_id") as i32).await;

    let res = server
        .post("/auth/api-keys")
        .authorization_bearer(jwt)
        .json(&json!({ "name": "openapi probe" }))
        .await;
    assert_eq!(res.status_code(), 201, "create API key: {}", res.text());
    res.json::<Value>()["key"]
        .as_str()
        .expect("API key")
        .to_string()
}

/// Smallest value `schema` allows: required properties only, placeholder
/// scalars. The handler's extractors must accept it, or the document names
/// fields the handler does not read. Strings are long enough for the password
/// length checks some handlers run before the credential check.
fn sample(spec: &Value, schema: &Value) -> Value {
    let Some(schema) = resolve(spec, schema) else {
        return json!({});
    };
    if let Some(parts) = schema["allOf"].as_array() {
        let mut merged = Map::new();
        for part in parts {
            match sample(spec, part) {
                Value::Object(fields) => merged.extend(fields),
                other => return other,
            }
        }
        return Value::Object(merged);
    }
    if let Some(first) = schema["oneOf"].get(0).or_else(|| schema["anyOf"].get(0)) {
        return sample(spec, first);
    }
    if let Some(first) = schema["enum"].get(0) {
        return first.clone();
    }
    match schema["type"].as_str() {
        Some("string") => match schema["format"].as_str() {
            Some("date-time") => json!("2030-01-01T00:00:00Z"),
            Some("date") => json!("2030-01-01"),
            _ => json!("placeholder"),
        },
        Some("integer") | Some("number") => json!(1),
        Some("boolean") => json!(false),
        Some("array") => json!([]),
        _ => {
            let mut fields = Map::new();
            for name in schema["required"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                fields.insert(name.to_string(), sample(spec, &schema["properties"][name]));
            }
            Value::Object(fields)
        }
    }
}

/// A request body built from the document. WebAuthn credentials are opaque
/// objects there, so those two carry the smallest credential webauthn-rs
/// deserializes.
fn sample_body(spec: &Value, method: &str, path: &str, op: &Value) -> Option<Value> {
    let body = resolve(spec, &op["requestBody"])?;
    let mut sample = sample(spec, &body["content"]["application/json"]["schema"]);
    match (method, path) {
        ("POST", "/auth/passkey/register/finish") => {
            sample["credential"] = json!({
                "id": "AA",
                "rawId": "AA",
                "response": { "attestationObject": "AA", "clientDataJSON": "AA" },
                "type": "public-key",
            });
        }
        ("POST", "/auth/passkey/login/finish") => {
            sample["credential"] = json!({
                "id": "AA",
                "rawId": "AA",
                "response": {
                    "authenticatorData": "AA",
                    "clientDataJSON": "AA",
                    "signature": "AA",
                    "userHandle": null,
                },
                "type": "public-key",
            });
        }
        _ => {}
    }
    Some(sample)
}

/// Call a documented operation with its path parameters, required query
/// parameters, and body filled in from the document.
async fn call(
    server: &TestServer,
    spec: &Value,
    method: &str,
    path: &str,
    op: &Value,
    token: Option<&str>,
) -> (StatusCode, String) {
    let mut uri = path.to_string();
    let mut query = Vec::new();
    for param in op["parameters"].as_array().into_iter().flatten() {
        let param = resolve(spec, param).expect("parameter resolves");
        let name = param["name"].as_str().expect("parameter name");
        let value = match sample(spec, &param["schema"]) {
            Value::String(s) => s,
            other => other.to_string(),
        };
        match param["in"].as_str() {
            Some("path") => uri = uri.replace(&format!("{{{name}}}"), &value),
            Some("query") if param["required"] == true => query.push(format!("{name}={value}")),
            _ => {}
        }
    }
    if !query.is_empty() {
        uri = format!("{uri}?{}", query.join("&"));
    }

    let mut request = server.method(Method::from_bytes(method.as_bytes()).unwrap(), &uri);
    if let Some(body) = sample_body(spec, method, path, op) {
        request = request.json(&body);
    }
    if let Some(token) = token {
        request = request.authorization_bearer(token);
    }
    let res = request.await;
    (res.status_code(), res.text())
}

/// axum answered from an extractor (`Json`, `Query`, `Path`): the request
/// never reached the handler.
fn rejected_by_extractor(status: StatusCode, text: &str) -> bool {
    status == StatusCode::UNPROCESSABLE_ENTITY
        || [
            "Failed to deserialize the JSON body",
            "Failed to parse the request body as JSON",
            "Expected request with `Content-Type: application/json`",
            "Failed to deserialize query string",
            "Invalid URL: ",
            "Wrong number of path arguments",
        ]
        .iter()
        .any(|prefix| text.starts_with(prefix))
}

/// Calls every documented operation and compares its declared security with
/// what the handler does: a protected operation refuses an anonymous call with
/// 401 and a public one does not, and an `opn_` API key works exactly where
/// `api_key` is listed. Bodies and query strings come from the document, so a
/// misnamed field (the QA pass could not find the password-reset fields) shows
/// up as an extractor rejection.
#[tokio::test]
async fn documented_security_matches_the_handlers() {
    let (server, db) = spawn_app_without_rate_limits().await;
    let spec: Value = server.get("/api-docs/openapi.json").await.json();
    let api_key = api_key_for_new_account(&server, &db).await;

    let mut wrong = Vec::new();
    for (method, path, op) in documented_operations(&spec) {
        let requirements = security_requirements(op);
        let requires_auth =
            !requirements.is_empty() && !requirements.iter().any(BTreeSet::is_empty);
        let lists_api_key = requirements.iter().any(|r| r.contains("api_key"));

        let (status, text) = call(&server, &spec, &method, &path, op, None).await;
        if rejected_by_extractor(status, &text) {
            wrong.push(format!(
                "{method} {path}: the documented request was rejected before the handler ran: \
                 {status} {text}"
            ));
            continue;
        }
        if requires_auth && status != StatusCode::UNAUTHORIZED {
            wrong.push(format!(
                "{method} {path}: declares security but answered {status} without credentials"
            ));
        }
        if !requires_auth
            && status == StatusCode::UNAUTHORIZED
            && !CHECKS_CREDENTIALS_IN_BODY.contains(&(method.as_str(), path.as_str()))
        {
            wrong.push(format!(
                "{method} {path}: documented as usable without credentials but answered 401"
            ));
        }

        if !requirements.is_empty() {
            let (status, _) = call(&server, &spec, &method, &path, op, Some(&api_key)).await;
            let refused = status == StatusCode::UNAUTHORIZED;
            if lists_api_key && refused {
                wrong.push(format!(
                    "{method} {path}: lists api_key but refused an API key"
                ));
            }
            if !lists_api_key && !refused {
                wrong.push(format!(
                    "{method} {path}: omits api_key but accepted an API key ({status})"
                ));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "documented security disagrees with the handlers:\n{}",
        wrong.join("\n")
    );
}

/// The QA pass could not tell which fields the reset calls take. Both bodies
/// must reference registered schemas that name them.
#[tokio::test]
async fn password_reset_bodies_name_their_fields() {
    let spec = served_spec().await;
    for (path, fields) in [
        ("/auth/forgot-password", &["email"][..]),
        ("/auth/reset-password", &["token", "password"][..]),
    ] {
        let schema =
            &spec["paths"][path]["post"]["requestBody"]["content"]["application/json"]["schema"];
        assert!(
            schema.get("$ref").is_some(),
            "{path}: the request body must reference a registered schema, got {schema}"
        );
        let schema = resolve(&spec, schema).expect("request body schema resolves");
        for field in fields {
            assert!(
                schema["properties"].get(field).is_some()
                    && schema["required"]
                        .as_array()
                        .is_some_and(|required| required.contains(&json!(field))),
                "{path}: `{field}` must be a required property of {schema}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Rate limiting
// ---------------------------------------------------------------------------

const RATE_LIMIT_HEADERS: [&str; 3] = ["Retry-After", "X-RateLimit-Limit", "X-RateLimit-Remaining"];

/// Every route is behind `rate_limit_middleware`, so every operation must
/// document a 429 with the headers a client needs to back off. The 1.3.2
/// document had none, not even on sign-in and link creation.
#[tokio::test]
async fn every_operation_documents_the_rate_limit_response() {
    let spec = served_spec().await;
    let mut missing = Vec::new();
    for (method, path, op) in documented_operations(&spec) {
        let Some(response) = resolve(&spec, &op["responses"]["429"]) else {
            missing.push(format!("{method} {path}: no 429 response"));
            continue;
        };
        for header in RATE_LIMIT_HEADERS {
            if response["headers"][header]["schema"]["type"] != "integer" {
                missing.push(format!(
                    "{method} {path}: the 429 does not document {header}"
                ));
            }
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));

    for (method, path) in [
        ("post", "/auth/register"),
        ("post", "/auth/login"),
        ("post", "/auth/forgot-password"),
        ("post", "/auth/reset-password"),
        ("post", "/links"),
        ("post", "/links/{id}/clone"),
        ("post", "/contact"),
    ] {
        assert_eq!(
            spec["paths"][path][method]["responses"]["429"]["$ref"],
            "#/components/responses/TooManyRequests",
            "{method} {path}"
        );
    }
}

/// "N per second|minute|hour", measured by spending the limiter's budget.
fn measured_budget(limiter: &RateLimiter) -> String {
    let RateLimitResult::Allowed { limit, .. } = limiter.check("measure") else {
        panic!("a fresh limiter refused its first request");
    };
    let retry_after = loop {
        if let RateLimitResult::Limited {
            retry_after_secs, ..
        } = limiter.check("measure")
        {
            break retry_after_secs;
        }
    };
    let unit = match retry_after {
        0..=1 => "second",
        2..=60 => "minute",
        _ => "hour",
    };
    format!("{limit} per {unit}")
}

/// The Rate limits table in the API description must quote the budgets the
/// middleware enforces, row by row.
#[tokio::test]
async fn rate_limit_table_matches_the_limiters() {
    let spec = served_spec().await;
    let description = spec["info"]["description"]
        .as_str()
        .expect("info.description");
    let row = |budget: &str| {
        let prefix = format!("| {budget} |");
        description
            .lines()
            .find(|line| line.starts_with(&prefix))
            .unwrap_or_else(|| panic!("no `{prefix}` row in the Rate limits table"))
    };

    let limiters = RateLimiters::default();
    for (budget, limiter, qualifier) in [
        ("Burst", &limiters.per_second, ""),
        ("Sign-in", &limiters.auth, ""),
        ("Link creation", &limiters.link_creation, ""),
        ("Contact", &limiters.contact, ""),
        ("Link password", &limiters.password_verify, " per link"),
        ("Link password", &limiters.password_verify_ip, " in total"),
        ("Redirects", &limiters.redirect, ""),
        ("General", &limiters.general, ""),
    ] {
        let expected = format!("{}{qualifier}", measured_budget(limiter));
        assert!(
            row(budget).contains(&expected),
            "the {budget} row must say `{expected}`: {}",
            row(budget)
        );
    }
}

/// A real 429 from the router carries exactly the documented headers and body.
#[tokio::test]
async fn rate_limited_answer_matches_the_documented_response() {
    let (server, _db) = spawn_real_app().await;
    let spec: Value = server.get("/api-docs/openapi.json").await.json();
    let documented = resolve(&spec, &spec["components"]["responses"]["TooManyRequests"])
        .expect("TooManyRequests response");
    let body_schema = resolve(&spec, &documented["content"]["application/json"]["schema"])
        .expect("TooManyRequests body schema");

    // Sign-in allows 10 POSTs a minute, and the burst budget 10 requests a
    // second, so one of the first dozen attempts is refused.
    let mut limited = None;
    for _ in 0..30 {
        let res = server
            .post("/auth/verify-email")
            .json(&json!({ "token": "not-a-token" }))
            .await;
        if res.status_code() == StatusCode::TOO_MANY_REQUESTS {
            limited = Some(res);
            break;
        }
    }
    let res = limited.expect("30 sign-in attempts were never rate limited");

    for header in documented["headers"]
        .as_object()
        .expect("documented headers")
        .keys()
    {
        let value = res
            .maybe_header(header.as_str())
            .unwrap_or_else(|| panic!("the 429 lacks the documented {header} header"));
        assert!(
            value.to_str().is_ok_and(|v| v.parse::<u64>().is_ok()),
            "{header} must be an integer, got {value:?}"
        );
    }

    let body: Value = res.json();
    let properties = body_schema["properties"]
        .as_object()
        .expect("RateLimitResponse properties");
    let sent: BTreeSet<&String> = body.as_object().expect("JSON object body").keys().collect();
    let documented_fields: BTreeSet<&String> = properties.keys().collect();
    assert_eq!(sent, documented_fields, "429 body fields: {body}");
    for (name, property) in properties {
        let matches = match property["type"].as_str() {
            Some("string") => body[name].is_string(),
            Some("integer") => body[name].is_u64() || body[name].is_i64(),
            _ => true,
        };
        assert!(matches, "`{name}` does not match {property}: {body}");
    }
}

// ---------------------------------------------------------------------------
// Structural validity (stands in for an OpenAPI 3.0 validator)
// ---------------------------------------------------------------------------

/// Keywords an OpenAPI 3.0 Schema Object may carry. OpenAPI 3.1 additions
/// such as `const`, `examples`, or a list-valued `type` are invalid in 3.0.
const SCHEMA_KEYWORDS: &str = "\
    title multipleOf maximum exclusiveMaximum minimum exclusiveMinimum maxLength minLength \
    pattern maxItems minItems uniqueItems maxProperties minProperties required enum type not \
    allOf oneOf anyOf items properties additionalProperties description format default \
    nullable discriminator readOnly writeOnly example externalDocs deprecated xml";

const PARAMETER_KEYS: &str = "\
    name in description required deprecated allowEmptyValue style explode allowReserved schema \
    content example examples";

const HEADER_KEYS: &str = "\
    description required deprecated allowEmptyValue style explode allowReserved schema content \
    example examples";

fn unexpected_keys(object: &Value, allowed: &str, at: &str, errors: &mut Vec<String>) {
    let Some(object) = object.as_object() else {
        errors.push(format!("{at}: must be an object"));
        return;
    };
    for key in object.keys() {
        if !allowed.split_whitespace().any(|k| k == key) && !key.starts_with("x-") {
            errors.push(format!("{at}: `{key}` is not allowed here in OpenAPI 3.0"));
        }
    }
}

fn check_schema(schema: &Value, at: &str, errors: &mut Vec<String>) {
    if schema.get("$ref").is_some() {
        return; // openapi_refs_resolve checks the target
    }
    unexpected_keys(schema, SCHEMA_KEYWORDS, at, errors);
    let Some(object) = schema.as_object() else {
        return;
    };
    match object.get("type") {
        None => {}
        Some(Value::String(t))
            if ["array", "boolean", "integer", "number", "object", "string"]
                .contains(&t.as_str()) =>
        {
            if t == "array" && !object.contains_key("items") {
                errors.push(format!("{at}: an array schema needs `items`"));
            }
        }
        Some(other) => errors.push(format!("{at}: `type` {other} is not an OpenAPI 3.0 type")),
    }
    if let Some(required) = object.get("required") {
        let names: Vec<&str> = required
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let distinct: BTreeSet<&str> = names.iter().copied().collect();
        if names.is_empty()
            || distinct.len() != names.len()
            || required
                .as_array()
                .is_none_or(|all| all.len() != names.len())
        {
            errors.push(format!(
                "{at}: `required` must be a non-empty list of distinct names"
            ));
        }
        for name in names {
            if object.get("properties").and_then(|p| p.get(name)).is_none() {
                errors.push(format!("{at}: required `{name}` is not a property"));
            }
        }
    }
    if object
        .get("enum")
        .is_some_and(|e| e.as_array().is_none_or(Vec::is_empty))
    {
        errors.push(format!("{at}: `enum` must be a non-empty list"));
    }
    for flag in [
        "nullable",
        "readOnly",
        "writeOnly",
        "deprecated",
        "uniqueItems",
        "exclusiveMinimum",
        "exclusiveMaximum",
    ] {
        if object.get(flag).is_some_and(|v| !v.is_boolean()) {
            errors.push(format!("{at}: `{flag}` must be a boolean in OpenAPI 3.0"));
        }
    }
    if let Some(properties) = object.get("properties") {
        match properties.as_object() {
            Some(properties) => {
                for (name, property) in properties {
                    check_schema(property, &format!("{at}.properties.{name}"), errors);
                }
            }
            None => errors.push(format!("{at}: `properties` must be an object")),
        }
    }
    for key in ["items", "not"] {
        if let Some(inner) = object.get(key) {
            check_schema(inner, &format!("{at}.{key}"), errors);
        }
    }
    if let Some(extra) = object.get("additionalProperties")
        && !extra.is_boolean()
    {
        check_schema(extra, &format!("{at}.additionalProperties"), errors);
    }
    for key in ["allOf", "oneOf", "anyOf"] {
        if let Some(list) = object.get(key) {
            match list.as_array() {
                Some(items) if !items.is_empty() => {
                    for (i, inner) in items.iter().enumerate() {
                        check_schema(inner, &format!("{at}.{key}[{i}]"), errors);
                    }
                }
                _ => errors.push(format!("{at}: `{key}` must be a non-empty list")),
            }
        }
    }
}

fn check_content(content: &Value, at: &str, errors: &mut Vec<String>) {
    let Some(media_types) = content.as_object() else {
        errors.push(format!("{at}: `content` must be an object"));
        return;
    };
    for (media_type, media) in media_types {
        let at = format!("{at}.{media_type}");
        unexpected_keys(media, "schema example examples encoding", &at, errors);
        if let Some(schema) = media.get("schema") {
            check_schema(schema, &format!("{at}.schema"), errors);
        }
    }
}

fn check_schema_or_content(object: &Value, at: &str, errors: &mut Vec<String>) {
    match (object.get("schema"), object.get("content")) {
        (Some(schema), None) => check_schema(schema, &format!("{at}.schema"), errors),
        (None, Some(content)) => check_content(content, &format!("{at}.content"), errors),
        _ => errors.push(format!("{at}: needs exactly one of `schema` and `content`")),
    }
}

fn check_response(spec: &Value, response: &Value, at: &str, errors: &mut Vec<String>) {
    let Some(response) = resolve(spec, response) else {
        return; // a dangling reference; openapi_refs_resolve reports it
    };
    unexpected_keys(response, "description headers content links", at, errors);
    if !response.get("description").is_some_and(Value::is_string) {
        errors.push(format!("{at}: a response needs a `description`"));
    }
    for (name, header) in response["headers"].as_object().into_iter().flatten() {
        let at = format!("{at}.headers.{name}");
        unexpected_keys(header, HEADER_KEYS, &at, errors);
        check_schema_or_content(header, &at, errors);
    }
    if let Some(content) = response.get("content") {
        check_content(content, &format!("{at}.content"), errors);
    }
}

/// `default`, a range such as `4XX`, or a status from 100 to 599.
fn is_response_code(code: &str) -> bool {
    let bytes = code.as_bytes();
    code == "default"
        || (bytes.len() == 3
            && (b'1'..=b'5').contains(&bytes[0])
            && (&code[1..] == "XX" || bytes[1..].iter().all(u8::is_ascii_digit)))
}

fn check_operation(
    spec: &Value,
    path: &str,
    method: &str,
    op: &Value,
    operation_ids: &mut BTreeMap<String, String>,
    errors: &mut Vec<String>,
) {
    let at = format!("{method} {path}");
    unexpected_keys(
        op,
        "tags summary description externalDocs operationId parameters requestBody \
         responses callbacks deprecated security servers",
        &at,
        errors,
    );

    if let Some(id) = op.get("operationId") {
        match id.as_str() {
            Some(id) => {
                if let Some(first) = operation_ids.insert(id.to_string(), at.clone()) {
                    errors.push(format!("{at}: operationId `{id}` is also used by {first}"));
                }
            }
            None => errors.push(format!("{at}: operationId must be a string")),
        }
    }

    let mut declared = BTreeSet::new();
    let mut path_params = BTreeSet::new();
    for (i, param) in op["parameters"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let at = format!("{at} parameters[{i}]");
        let Some(param) = resolve(spec, param) else {
            continue;
        };
        unexpected_keys(param, PARAMETER_KEYS, &at, errors);
        let name = param["name"].as_str().unwrap_or_default();
        let location = param["in"].as_str().unwrap_or_default();
        if name.is_empty() {
            errors.push(format!("{at}: a parameter needs a `name`"));
        }
        if !["path", "query", "header", "cookie"].contains(&location) {
            errors.push(format!("{at}: `in` must be path, query, header, or cookie"));
        }
        if !declared.insert((name, location)) {
            errors.push(format!("{at}: `{name}` in {location} is declared twice"));
        }
        if location == "path" {
            path_params.insert(name);
            if param["required"] != true {
                errors.push(format!("{at}: path parameter `{name}` must be required"));
            }
        }
        check_schema_or_content(param, &at, errors);
    }
    let template: BTreeSet<&str> = path
        .split('{')
        .skip(1)
        .filter_map(|rest| rest.split_once('}').map(|(name, _)| name))
        .collect();
    if template != path_params {
        errors.push(format!(
            "{at}: the path template names {template:?} but the path parameters are \
             {path_params:?}"
        ));
    }

    if let Some(body) = op.get("requestBody")
        && let Some(body) = resolve(spec, body)
    {
        unexpected_keys(
            body,
            "description content required",
            &format!("{at} requestBody"),
            errors,
        );
        match body.get("content").and_then(Value::as_object) {
            Some(content) if !content.is_empty() => {
                check_content(
                    &body["content"],
                    &format!("{at} requestBody.content"),
                    errors,
                );
            }
            _ => errors.push(format!("{at}: requestBody needs `content`")),
        }
    }

    match op.get("responses").and_then(Value::as_object) {
        Some(responses) if !responses.is_empty() => {
            for (code, response) in responses {
                if !is_response_code(code) {
                    errors.push(format!("{at}: `{code}` is not a response status"));
                }
                check_response(spec, response, &format!("{at} {code}"), errors);
            }
        }
        _ => errors.push(format!("{at}: an operation needs at least one response")),
    }

    let schemes = &spec["components"]["securitySchemes"];
    for requirement in op["security"].as_array().into_iter().flatten() {
        for (name, scopes) in requirement.as_object().into_iter().flatten() {
            if schemes.get(name).is_none() {
                errors.push(format!("{at}: security names undefined scheme `{name}`"));
            }
            if !scopes
                .as_array()
                .is_some_and(|scopes| scopes.iter().all(Value::is_string))
            {
                errors.push(format!(
                    "{at}: scopes for `{name}` must be a list of strings"
                ));
            }
        }
    }
}

#[tokio::test]
async fn openapi_document_is_valid_openapi_3_0() {
    let spec = served_spec().await;
    let mut errors = Vec::new();

    unexpected_keys(
        &spec,
        "openapi info externalDocs servers security tags paths components",
        "document",
        &mut errors,
    );
    assert!(
        spec["openapi"]
            .as_str()
            .is_some_and(|v| v.starts_with("3.0.")),
        "not an OpenAPI 3.0 document: {}",
        spec["openapi"]
    );
    let info = &spec["info"];
    unexpected_keys(
        info,
        "title description termsOfService contact license version",
        "info",
        &mut errors,
    );
    for field in ["title", "version"] {
        if !info[field].is_string() {
            errors.push(format!("info.{field} is required"));
        }
    }
    if info.get("license").is_some() && !info["license"]["name"].is_string() {
        errors.push("info.license needs a name".to_string());
    }
    for (i, server) in spec["servers"].as_array().into_iter().flatten().enumerate() {
        if !server["url"].is_string() {
            errors.push(format!("servers[{i}] needs a url"));
        }
    }

    let components = &spec["components"];
    unexpected_keys(
        components,
        "schemas responses parameters examples requestBodies headers securitySchemes \
         links callbacks",
        "components",
        &mut errors,
    );
    for (kind, entries) in components.as_object().into_iter().flatten() {
        for name in entries
            .as_object()
            .into_iter()
            .flatten()
            .map(|(name, _)| name)
        {
            if name.is_empty()
                || !name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
            {
                errors.push(format!(
                    "components.{kind}: `{name}` is not a valid component name"
                ));
            }
        }
    }
    for (name, schema) in components["schemas"].as_object().into_iter().flatten() {
        check_schema(schema, &format!("components.schemas.{name}"), &mut errors);
    }
    for (name, response) in components["responses"].as_object().into_iter().flatten() {
        check_response(
            &spec,
            response,
            &format!("components.responses.{name}"),
            &mut errors,
        );
    }
    for (name, scheme) in components["securitySchemes"]
        .as_object()
        .into_iter()
        .flatten()
    {
        let at = format!("components.securitySchemes.{name}");
        match scheme["type"].as_str() {
            Some("http") => {
                unexpected_keys(
                    scheme,
                    "type scheme bearerFormat description",
                    &at,
                    &mut errors,
                );
                let bearer = scheme["scheme"]
                    .as_str()
                    .is_some_and(|s| s.eq_ignore_ascii_case("bearer"));
                if !scheme["scheme"].is_string() {
                    errors.push(format!("{at}: an http scheme needs `scheme`"));
                }
                if scheme.get("bearerFormat").is_some() && !bearer {
                    errors.push(format!("{at}: `bearerFormat` is only for bearer schemes"));
                }
            }
            Some("apiKey") => {
                unexpected_keys(scheme, "type name in description", &at, &mut errors);
                if !["query", "header", "cookie"]
                    .contains(&scheme["in"].as_str().unwrap_or_default())
                {
                    errors.push(format!("{at}: an apiKey scheme needs `in`"));
                }
            }
            Some("oauth2") | Some("openIdConnect") => {}
            other => errors.push(format!("{at}: unknown scheme type {other:?}")),
        }
    }

    let mut operation_ids = BTreeMap::new();
    let mut templates: BTreeMap<String, String> = BTreeMap::new();
    for (path, item) in spec["paths"].as_object().expect("paths") {
        if !path.starts_with('/') {
            errors.push(format!("path `{path}` must start with /"));
        }
        let mut shape = String::new();
        let mut in_param = false;
        for c in path.chars() {
            match c {
                '{' => {
                    in_param = true;
                    shape.push('{');
                }
                '}' => {
                    in_param = false;
                    shape.push('}');
                }
                _ if in_param => {}
                _ => shape.push(c),
            }
        }
        if let Some(other) = templates.insert(shape, path.clone()) {
            errors.push(format!(
                "`{path}` and `{other}` are the same templated path"
            ));
        }
        unexpected_keys(
            item,
            "$ref summary description servers parameters get put post delete options \
             head patch trace",
            path,
            &mut errors,
        );
    }
    for (method, path, op) in documented_operations(&spec) {
        check_operation(&spec, &path, &method, op, &mut operation_ids, &mut errors);
    }

    assert!(
        errors.is_empty(),
        "the served document is not valid OpenAPI 3.0:\n{}",
        errors.join("\n")
    );
}
