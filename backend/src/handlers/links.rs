use axum::{
    extract::{State, Path, Query},
    http::{StatusCode, HeaderMap},
    Json,
    response::{IntoResponse, Redirect},
};
use serde::{Deserialize, Serialize};
use sea_orm::*;
use validator::Validate;
use rand::{thread_rng, Rng};
use rand::distributions::Alphanumeric;
use chrono::{DateTime, Utc};
use bcrypt::{hash, DEFAULT_COST};
use utoipa::ToSchema;

use crate::AppState;
use crate::entity::{links, link_tags, tags, blocked_links, blocked_domains, users, click_events};
use crate::utils::jwt::decode_jwt;
use crate::utils::geoip::{lookup_ip, parse_user_agent};
use crate::handlers::websocket::ClickEvent;

/// Check if URL or its domain is blocked
async fn check_blocked(db: &DatabaseConnection, url: &str) -> Result<(), String> {
    let parsed_url = url::Url::parse(url).map_err(|_| "Invalid URL".to_string())?;
    // Normalized host: lowercase + strip trailing dot (defeats simple casing / FQDN-dot bypass).
    let host = parsed_url.host_str().unwrap_or("").trim_end_matches('.').to_lowercase();

    // Exact-URL block. Also check the trailing-slash-trimmed form so a "/" tweak can't bypass.
    let mut url_candidates = vec![url.to_string()];
    let trimmed = url.trim_end_matches('/').to_string();
    if !url_candidates.contains(&trimmed) {
        url_candidates.push(trimmed);
    }
    let blocked_url = blocked_links::Entity::find()
        .filter(blocked_links::Column::Url.is_in(url_candidates))
        .one(db)
        .await
        .ok()
        .flatten();
    if let Some(blocked) = blocked_url {
        return Err(format!("This URL is blocked: {}", blocked.reason.unwrap_or_else(|| "Policy violation".to_string())));
    }

    // Domain block (host + subdomains). A block on "evil.com" must also block
    // "sub.evil.com", so the set of blocked_domains rows that could match this
    // host is exactly the host itself plus each of its parent domains. Query only
    // those candidates against the indexed, normalized `domain` column instead of
    // loading and scanning the whole table on every uncached redirect. Stored
    // domains are normalized on write (see admin::block_domain) and by migration
    // m20220101_000028, so the candidates compare directly.
    if !host.is_empty() {
        let mut candidates: Vec<String> = Vec::new();
        let mut suffix = host.as_str();
        loop {
            candidates.push(suffix.to_string());
            match suffix.find('.') {
                Some(pos) => suffix = &suffix[pos + 1..],
                None => break,
            }
        }
        let hit = blocked_domains::Entity::find()
            .filter(blocked_domains::Column::Domain.is_in(candidates))
            .one(db)
            .await
            .ok()
            .flatten();
        if let Some(bd) = hit {
            return Err(format!("This domain is blocked: {}", bd.reason.unwrap_or_else(|| "Policy violation".to_string())));
        }
    }

    Ok(())
}

/// Returns true if the folder exists and the user may place links in it: either
/// they personally own it, or it belongs to an organization they are a member
/// of. Prevents assigning a link into another user's folder (cross-tenant IDOR).
async fn user_can_use_folder(db: &DatabaseConnection, folder_id: i32, user_id: i32) -> bool {
    use crate::entity::{folders, org_members};
    let folder = match folders::Entity::find_by_id(folder_id).one(db).await.ok().flatten() {
        Some(f) => f,
        None => return false,
    };
    if folder.user_id == Some(user_id) {
        return true;
    }
    if let Some(org_id) = folder.org_id {
        return org_members::Entity::find()
            .filter(org_members::Column::OrgId.eq(org_id))
            .filter(org_members::Column::UserId.eq(user_id))
            .one(db)
            .await
            .ok()
            .flatten()
            .is_some();
    }
    false
}

// ============= Configuration =============

/// Get minimum alias length from ENV (default: 5)
fn get_min_alias_length() -> usize {
    std::env::var("MIN_ALIAS_LENGTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5)
}

/// Get maximum alias length from ENV (default: 50)
fn get_max_alias_length() -> usize {
    std::env::var("MAX_ALIAS_LENGTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50)
}

/// Per-user link cap from MAX_LINKS_PER_USER. `None` (unset / unparseable / 0)
/// means unlimited. Surfaced in GET /auth/settings and enforced at link create.
fn get_max_links_per_user() -> Option<u64> {
    std::env::var("MAX_LINKS_PER_USER")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&n| n > 0)
}

/// Check if URL sanitization is enabled (default: true)
fn is_url_sanitization_enabled() -> bool {
    std::env::var("ENABLE_URL_SANITIZATION")
        .unwrap_or_else(|_| "true".to_string())
        .parse::<bool>()
        .unwrap_or(true)
}

/// Read a boolean env var that defaults to `true` (the safe/on setting). Any
/// value other than "false"/"0" (case-insensitive) is treated as enabled, so a
/// blank or malformed value fails safe rather than opening the guard.
fn env_flag_default_on(name: &str) -> bool {
    match std::env::var(name) {
        Ok(v) => {
            let v = v.trim().to_ascii_lowercase();
            v != "false" && v != "0" && v != "no"
        }
        Err(_) => true,
    }
}

/// Reject destinations that exist only to deliver a payload. Two independent,
/// config-gated guards (both default ON for the hosted service; a self-hoster
/// can turn either off):
///   * `BLOCK_DANGEROUS_FILE_EXTENSIONS` — links straight at a `.hta`, `.exe`,
///     `.scr`, … file.
///   * `BLOCK_RAW_IP_URLS` — links whose host is a bare IP literal.
///
/// Runs on every create/update/bulk/routing destination via `validate_url`.
fn check_url_content_policy(url: &str) -> Result<(), String> {
    if env_flag_default_on("BLOCK_DANGEROUS_FILE_EXTENSIONS") {
        if let Some(ext) = crate::utils::url_policy::dangerous_extension(url) {
            return Err(format!(
                "Links to .{ext} files are not allowed (potentially executable content)"
            ));
        }
    }
    if env_flag_default_on("BLOCK_RAW_IP_URLS")
        && crate::utils::url_policy::host_is_raw_ip(url)
    {
        return Err("Links to raw IP addresses are not allowed".to_string());
    }
    Ok(())
}

// ============= URL Validation =============

/// Validate URL is http/https only and sanitize if enabled
fn validate_url(url: &str) -> Result<String, String> {
    // Must be a valid URL
    let parsed = url::Url::parse(url).map_err(|_| "Invalid URL format".to_string())?;
    
    // Must be http or https
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("URL must use http or https protocol".to_string());
    }
    
    // Must have a host
    let Some(host) = parsed.host_str() else {
        return Err("URL must have a valid host".to_string());
    };
    if crate::utils::url_policy::is_disallowed_hostname(host) {
        return Err("Links to local/internal hosts are not allowed".to_string());
    }

    // Sanitization checks (if enabled)
    if is_url_sanitization_enabled() {
        let url_lower = url.to_lowercase();
        
        // Block javascript: URLs (XSS)
        if url_lower.contains("javascript:") {
            return Err("URL contains potentially malicious content".to_string());
        }
        
        // Block data: URLs (can contain malicious payloads)
        if url_lower.contains("data:") {
            return Err("Data URLs are not allowed".to_string());
        }
        
        // Block common XSS patterns in URL
        let xss_patterns = [
            "<script", "</script>", "onerror=", "onload=", "onclick=",
            "onmouseover=", "onfocus=", "onblur=", "eval(", "alert(",
            "document.cookie", "document.location", "window.location",
        ];
        
        for pattern in xss_patterns {
            if url_lower.contains(pattern) {
                return Err("URL contains potentially malicious content".to_string());
            }
        }
        
        // Block URLs with encoded malicious content
        if let Ok(decoded) = urlencoding::decode(url) {
            let decoded_lower = decoded.to_lowercase();
            for pattern in xss_patterns {
                if decoded_lower.contains(pattern) {
                    return Err("URL contains encoded malicious content".to_string());
                }
            }
        }
        
        // Block extremely long URLs (potential DoS)
        if url.len() > 2048 {
            return Err("URL is too long (max 2048 characters)".to_string());
        }
    }

    // Content-safety guards (dangerous file types, raw-IP hosts). Independent of
    // ENABLE_URL_SANITIZATION so they can't be disabled as a side effect.
    check_url_content_policy(url)?;

    Ok(url.to_string())
}

// ============= SSRF guard =============

/// Returns true if the address must never be reachable by server-side fetches
/// (loopback, private, link-local incl. the 169.254.169.254 cloud-metadata
/// endpoint, CGNAT, reserved, etc.). Used to block SSRF on user-supplied URLs.
fn is_disallowed_ip(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()      // 169.254.0.0/16 (cloud metadata lives here)
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.octets()[0] == 0                                        // 0.0.0.0/8
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64)   // 100.64.0.0/10 CGNAT
                || v4.octets()[0] >= 240                                      // 240.0.0.0/4 reserved
        }
        std::net::IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_disallowed_ip(&std::net::IpAddr::V4(v4));
            }
            v6.is_loopback()
                || v6.is_unspecified()
                || (v6.segments()[0] & 0xfe00) == 0xfc00   // fc00::/7 unique local
                || (v6.segments()[0] & 0xffc0) == 0xfe80   // fe80::/10 link-local
        }
    }
}

/// A host that passed the SSRF guard, together with the exact set of addresses
/// it resolved to. The connection is later pinned to these addresses so the IP
/// that is connected to is always the IP that was validated (no second,
/// independent DNS lookup that a rebinding attacker could answer differently).
#[derive(Debug)]
struct ValidatedTarget {
    /// Host as it appears in the URL (used for the `Host` header and TLS SNI).
    host: String,
    /// Validated addresses to pin the connection to (IP + URL port).
    addrs: Vec<std::net::SocketAddr>,
    /// True when the host is a literal IP, so no DNS override is needed.
    is_literal_ip: bool,
}

/// SSRF guard: resolve a URL's host and reject it if the host is, or resolves
/// to, any private/internal address. Returns the validated addresses so the
/// caller can pin the connection to them. Resolving here and connecting to the
/// exact addresses returned closes the DNS-rebinding TOCTOU: validation and
/// connection can no longer see different DNS answers.
async fn resolve_and_validate(url: &str) -> Result<ValidatedTarget, String> {
    let parsed = url::Url::parse(url).map_err(|_| "Invalid URL".to_string())?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err("Only http/https URLs are allowed".to_string()),
    }
    let host = parsed.host_str().ok_or_else(|| "URL has no host".to_string())?;
    let port = parsed.port_or_known_default().unwrap_or(80);

    let (addrs, is_literal_ip): (Vec<std::net::SocketAddr>, bool) =
        if let Ok(ip) = host.parse::<std::net::IpAddr>() {
            (vec![std::net::SocketAddr::new(ip, port)], true)
        } else {
            let resolved: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host, port))
                .await
                .map_err(|_| "Could not resolve host".to_string())?
                .collect();
            (resolved, false)
        };

    if addrs.is_empty() {
        return Err("Host did not resolve".to_string());
    }
    // Reject if ANY resolved address is internal/private. Because the connection
    // is then pinned to exactly this address set, a rebinding answer cannot slip
    // an internal IP in between validation and connect.
    if addrs.iter().any(|sa| is_disallowed_ip(&sa.ip())) {
        return Err("URL resolves to a disallowed (internal/private) address".to_string());
    }
    Ok(ValidatedTarget {
        host: host.to_string(),
        addrs,
        is_literal_ip,
    })
}

/// Build a reqwest client that connects **only** to the validated addresses for
/// this hop. `resolve_to_addrs` overrides DNS for the target host, so reqwest
/// does not perform its own (second) lookup, while the `Host` header and TLS
/// SNI stay set to the hostname — HTTPS certificate validation is unaffected.
fn build_pinned_client(
    target: &ValidatedTarget,
    user_agent: Option<&str>,
) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none());
    if let Some(ua) = user_agent {
        builder = builder.user_agent(ua);
    }
    // Literal-IP hosts trigger no DNS in reqwest, so there is nothing to pin.
    if !target.is_literal_ip {
        builder = builder.resolve_to_addrs(&target.host, &target.addrs);
    }
    builder
        .build()
        .map_err(|_| "Failed to build HTTP client".to_string())
}

/// Perform an HTTP request with the SSRF guard applied to the initial URL and to
/// every redirect hop. Redirects are followed manually (Policy::none) so each
/// `Location` is re-validated, defeating redirect-based SSRF. Every hop is
/// resolved, validated, and then connected to via a DNS-pinned client, so the
/// connected IP is always the validated IP (DNS rebinding cannot open a gap
/// between the check and the connect). Returns the final response.
async fn ssrf_guarded_fetch(
    method: reqwest::Method,
    start_url: &str,
    user_agent: Option<&str>,
) -> Result<reqwest::Response, String> {
    let mut current = start_url.to_string();
    // Initial request plus up to 5 redirects.
    for _ in 0..6 {
        let target = resolve_and_validate(&current).await?;
        let client = build_pinned_client(&target, user_agent)?;
        let resp = client
            .request(method.clone(), &current)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if resp.status().is_redirection() {
            if let Some(location) = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|l| l.to_str().ok())
            {
                let base = url::Url::parse(&current).map_err(|_| "Invalid URL".to_string())?;
                let next = base
                    .join(location)
                    .map_err(|_| "Invalid redirect location".to_string())?;
                current = next.to_string();
                continue;
            }
        }
        return Ok(resp);
    }
    Err("Too many redirects".to_string())
}

/// Validate alias format and length
fn validate_alias(alias: &str) -> Result<(), String> {
    let min_len = get_min_alias_length();
    let max_len = get_max_alias_length();
    
    if alias.len() < min_len {
        return Err(format!("Alias must be at least {} characters", min_len));
    }
    
    if alias.len() > max_len {
        return Err(format!("Alias must be at most {} characters", max_len));
    }
    
    // Only allow alphanumeric, hyphens, and underscores
    if !alias.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("Alias can only contain letters, numbers, hyphens, and underscores".to_string());
    }
    
    // Cannot start or end with hyphen/underscore
    if alias.starts_with('-') || alias.starts_with('_') || alias.ends_with('-') || alias.ends_with('_') {
        return Err("Alias cannot start or end with hyphen or underscore".to_string());
    }

    // Reserved words that would collide with a backend API route OR a frontend
    // SPA route. Short links are handed out as FRONTEND_URL/<code> (opn.onl),
    // and nginx serves the marketing/app routes from its allowlist, so an alias
    // matching a frontend route (e.g. "about", "pricing") would render that page
    // instead of redirecting — a dead link. Keep in sync with the nginx
    // allowlist and frontend/src/App.tsx.
    const RESERVED: &[&str] = &[
        // backend API routes
        "health", "links", "link", "auth", "admin", "orgs", "org", "organizations",
        "folders", "tags", "analytics", "contact", "ws", "sse", "api", "api-docs",
        "swagger-ui", "password", "verify", "preview", "me", "profile",
        "robots.txt", "favicon.ico", "sitemap.xml", "404",
        // frontend SPA routes (opn.onl/<route>)
        "features", "pricing", "about", "privacy", "terms", "faq", "docs",
        "developers", "login", "register", "dashboard", "settings",
        "forgot-password", "reset-password", "verify-email", "r",
    ];
    if RESERVED.contains(&alias.to_lowercase().as_str()) {
        return Err("This alias is reserved and cannot be used".to_string());
    }

    Ok(())
}

// ============= DTOs =============

#[derive(Deserialize, Validate, ToSchema)]
pub struct CreateLinkRequest {
    #[serde(default)]
    pub original_url: String,
    pub custom_alias: Option<String>,
    pub title: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub password: Option<String>,
    pub notes: Option<String>,
    pub folder_id: Option<i32>,
    pub org_id: Option<i32>,
    pub starts_at: Option<DateTime<Utc>>,
    pub max_clicks: Option<i32>,
    pub burn_after_reading: Option<bool>,
    pub safe_link_interstitial: Option<bool>,
    pub tag_ids: Option<Vec<i32>>,
}

#[derive(Deserialize, Validate, ToSchema)]
pub struct UpdateLinkRequest {
    pub original_url: Option<String>,
    pub title: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub password: Option<String>,
    pub remove_password: Option<bool>,
    pub remove_expiration: Option<bool>,
    pub notes: Option<String>,
    pub folder_id: Option<i32>,
    pub starts_at: Option<DateTime<Utc>>,
    pub max_clicks: Option<i32>,
    pub burn_after_reading: Option<bool>,
    pub safe_link_interstitial: Option<bool>,
    pub bio_visible: Option<bool>,
    pub remove_starts_at: Option<bool>,
    pub remove_max_clicks: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct BulkCreateLinkRequest {
    pub urls: Vec<String>,
    pub folder_id: Option<i32>,
    pub org_id: Option<i32>,
}

#[derive(Deserialize, ToSchema)]
pub struct BulkDeleteRequest {
    pub ids: Vec<i32>,
}

#[derive(Deserialize, ToSchema)]
pub struct BulkUpdateRequest {
    pub ids: Vec<i32>,
    pub folder_id: Option<i32>,
    pub expires_at: Option<DateTime<Utc>>,
    pub remove_expiration: Option<bool>,
}

#[derive(Deserialize, ToSchema, utoipa::IntoParams)]
pub struct LinksQuery {
    pub folder_id: Option<i32>,
    pub org_id: Option<i32>,
    pub tag_id: Option<i32>,
    pub search: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Serialize, ToSchema)]
pub struct BulkCreateLinkResponse {
    pub links: Vec<CreateLinkResponse>,
    pub errors: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct CreateLinkResponse {
    pub id: i32,
    pub code: String,
    pub short_url: String,
}

#[derive(Serialize, ToSchema, Clone)]
pub struct TagInfo {
    pub id: i32,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Serialize, ToSchema, Clone)]
pub struct LinkResponse {
    pub id: i32,
    pub code: String,
    pub short_url: String,
    pub api_url: String,
    pub original_url: String,
    pub title: Option<String>,
    pub click_count: i32,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub has_password: bool,
    pub notes: Option<String>,
    pub folder_id: Option<i32>,
    pub org_id: Option<i32>,
    pub starts_at: Option<String>,
    pub max_clicks: Option<i32>,
    pub burn_after_reading: bool,
    pub burned_at: Option<String>,
    pub safe_link_interstitial: bool,
    pub bio_visible: bool,
    pub is_active: bool,
    pub is_pinned: bool,
    pub tags: Vec<TagInfo>,
}

#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Serialize, ToSchema)]
pub struct SuccessResponse {
    pub message: String,
}

#[derive(Deserialize, ToSchema)]
pub struct VerifyPasswordRequest {
    pub password: String,
}

#[derive(Serialize, ToSchema)]
pub struct BulkDeleteResponse {
    pub deleted: u64,
}

#[derive(Serialize, ToSchema)]
pub struct BulkUpdateResponse {
    pub updated: u64,
}

// ============= Helper Functions =============

fn generate_short_code() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(6)
        .map(char::from)
        .collect()
}

/// Authenticate a request from its bearer token AND verify it against the DB:
/// the user must exist, must not be soft-deleted, and the token's `token_version`
/// must match the user's current version. This is what makes JWTs revocable
/// (a password change / reset / account-delete / passkey-revoke bumps the version).
pub async fn get_user_id_from_header(db: &sea_orm::DatabaseConnection, headers: &HeaderMap) -> Option<i32> {
    let auth_header = headers.get("Authorization")?.to_str().ok()?;
    let token = auth_header.strip_prefix("Bearer ")?;

    // API key path: tokens prefixed `opn_` are personal access tokens (used by the
    // MCP server / external API clients), looked up by their sha256 hash.
    if token.starts_with("opn_") {
        return resolve_api_key(db, token).await;
    }

    let claims = decode_jwt(token).ok()?;
    let user = users::Entity::find_by_id(claims.user_id)
        .filter(users::Column::DeletedAt.is_null())
        .one(db)
        .await
        .ok()??;
    if user.token_version == claims.token_version {
        Some(user.id)
    } else {
        None
    }
}

/// Invalidate cached redirect entries for these codes. No-op when Redis is not
/// configured. Any handler that changes link state (block/edit/delete/expire)
/// must call this, or a stale redirect keeps serving from cache until the TTL.
pub async fn invalidate_cached_codes(state: &AppState, codes: &[String]) {
    if let Some(cache) = &state.redis_cache {
        for code in codes {
            let _ = cache.invalidate_link(code).await;
        }
    }
}

/// Codes of a user's currently-active links, captured *before* a bulk soft-delete
/// so their cache entries can be dropped afterwards (soft-delete is an UPDATE, so
/// nothing else clears them). `invalidate_cached_codes` no-ops without Redis, so
/// callers pass the result unconditionally.
pub async fn active_link_codes_for_user(state: &AppState, user_id: i32) -> Vec<String> {
    links::Entity::find()
        .filter(links::Column::UserId.eq(user_id))
        .filter(links::Column::DeletedAt.is_null())
        .all(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|l| l.code)
        .collect()
}

/// sha256(key) base64-encoded — the value stored in `api_keys.key_hash`. Keys are
/// high-entropy random strings, so a fast hash (not bcrypt) is appropriate and
/// keeps per-request authentication O(1) via the unique index.
pub fn hash_api_key(key: &str) -> String {
    use sha2::{Digest, Sha256};
    use base64::Engine as _;
    let digest = Sha256::digest(key.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(digest)
}

/// Resolve an `opn_` API key to its (non-deleted) owner; best-effort stamps
/// `last_used_at`. Returns None for unknown/revoked keys or deleted owners.
async fn resolve_api_key(db: &sea_orm::DatabaseConnection, key: &str) -> Option<i32> {
    use crate::entity::api_keys;
    // Instance kill-switch: when ENABLE_API_KEYS=false, keys stop authenticating.
    if std::env::var("ENABLE_API_KEYS").map(|v| v == "false").unwrap_or(false) {
        return None;
    }
    let hash = hash_api_key(key);
    let rec = api_keys::Entity::find()
        .filter(api_keys::Column::KeyHash.eq(hash))
        .one(db)
        .await
        .ok()??;
    let user = users::Entity::find_by_id(rec.user_id)
        .filter(users::Column::DeletedAt.is_null())
        .one(db)
        .await
        .ok()??;
    let am = api_keys::ActiveModel {
        id: Set(rec.id),
        last_used_at: Set(Some(chrono::Utc::now().naive_utc())),
        ..Default::default()
    };
    let _ = am.update(db).await;
    Some(user.id)
}

fn get_base_url() -> String {
    // Use FRONTEND_URL for short links (e.g., https://opn.onl)
    std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:5173".to_string())
}

fn get_api_url() -> String {
    // Use BASE_URL for direct API/redirect links (e.g., https://l.opn.onl)
    std::env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
}

fn interstitial_feature_enabled() -> bool {
    std::env::var("ENABLE_SAFE_LINK_INTERSTITIAL")
        .map(|v| v != "false")
        .unwrap_or(true)
}

fn redirect_confirmed(confirm: Option<&str>) -> bool {
    confirm.map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false)
}

fn frontend_interstitial_redirect(code: &str) -> axum::response::Response {
    // Send to the SPA interstitial route (/r/<code>), NOT the bare short link —
    // the bare path is proxied straight back here and would loop. The SPA shows
    // the "you're leaving" screen and re-hits this endpoint with ?confirm=1.
    let frontend = get_base_url().trim_end_matches('/').to_string();
    Redirect::temporary(&format!("{frontend}/r/{code}")).into_response()
}

async fn get_link_tags(db: &DatabaseConnection, link_id: i32) -> Vec<TagInfo> {
    let link_tags_list = link_tags::Entity::find()
        .filter(link_tags::Column::LinkId.eq(link_id))
        .all(db)
        .await
        .unwrap_or_default();

    let tag_ids: Vec<i32> = link_tags_list.iter().map(|lt| lt.tag_id).collect();

    if tag_ids.is_empty() {
        return vec![];
    }

    let tags_list = tags::Entity::find()
        .filter(tags::Column::Id.is_in(tag_ids))
        .all(db)
        .await
        .unwrap_or_default();

    tags_list.into_iter().map(|t| TagInfo {
        id: t.id,
        name: t.name,
        color: t.color,
    }).collect()
}

// ============= Handlers =============

/// Create a new shortened link
#[utoipa::path(
    post,
    path = "/links",
    request_body = CreateLinkRequest,
    responses(
        (status = 201, description = "Link created", body = LinkResponse),
        (status = 400, description = "Invalid request"),
        (status = 409, description = "Alias already exists"),
    ),
    tag = "Links"
)]
pub async fn create_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateLinkRequest>,
) -> impl IntoResponse {
    // Validate URL first
    let validated_url = match validate_url(&payload.original_url) {
        Ok(url) => url,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })).into_response(),
    };

    let user_id = get_user_id_from_header(&state.db, &headers).await;

    // Check email verification for authenticated users
    if let Some(uid) = user_id {
        let user = users::Entity::find_by_id(uid)
            .one(&state.db)
            .await
            .ok()
            .flatten();
        
        if let Some(u) = user {
            if !u.email_verified {
                return (StatusCode::FORBIDDEN, Json(ErrorResponse {
                    error: "Please verify your email address before creating links".to_string()
                })).into_response();
            }
        }
    }

    // Enforce the per-user link cap (MAX_LINKS_PER_USER). This is surfaced in
    // GET /auth/settings; previously it was advertised but never enforced
    // (fail-open). Applies to authenticated users only (anonymous links have no
    // owner to cap). None/0 = unlimited.
    if let Some(uid) = user_id {
        if let Some(cap) = get_max_links_per_user() {
            let existing = links::Entity::find()
                .filter(links::Column::UserId.eq(uid))
                .filter(links::Column::DeletedAt.is_null())
                .count(&state.db)
                .await
                .unwrap_or(0);
            if existing >= cap {
                return (StatusCode::FORBIDDEN, Json(ErrorResponse {
                    error: format!("You have reached the maximum of {} links for this account", cap),
                })).into_response();
            }
        }
    }

    // Rate limit: same URL can only be shortened 10 times in 10 minutes
    if let Some(uid) = user_id {
        let ten_mins_ago = chrono::Utc::now() - chrono::Duration::minutes(10);
        let recent_same_url_count = links::Entity::find()
            .filter(links::Column::UserId.eq(uid))
            .filter(links::Column::OriginalUrl.eq(&validated_url))
            .filter(links::Column::CreatedAt.gte(ten_mins_ago.naive_utc()))
            .count(&state.db)
            .await
            .unwrap_or(0);
        
        if recent_same_url_count >= 10 {
            return (StatusCode::TOO_MANY_REQUESTS, Json(ErrorResponse { 
                error: "You have shortened this URL too many times. Please wait a few minutes.".to_string() 
            })).into_response();
        }
    }

    // Check if URL or domain is blocked (MUST be checked before any link creation)
    if let Err(e) = check_blocked(&state.db, &validated_url).await {
        return (StatusCode::FORBIDDEN, Json(ErrorResponse { error: e })).into_response();
    }

    // Check if custom aliases are enabled
    let custom_aliases_enabled = std::env::var("ENABLE_CUSTOM_ALIASES")
        .unwrap_or_else(|_| "true".to_string())
        .parse::<bool>()
        .unwrap_or(true);
    
    let code = if let Some(alias) = payload.custom_alias {
        // Check if custom aliases are enabled
        if !custom_aliases_enabled {
            return (StatusCode::FORBIDDEN, Json(ErrorResponse { error: "Custom aliases are disabled".to_string() })).into_response();
        }
        
        // Validate alias format and length
        if let Err(e) = validate_alias(&alias) {
            return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })).into_response();
        }
        
        // Check if alias exists (active links)
        let exists_active = links::Entity::find()
            .filter(links::Column::Code.eq(&alias))
            .filter(links::Column::DeletedAt.is_null())
            .one(&state.db)
            .await
            .unwrap_or(None);
        
        if exists_active.is_some() {
            return (StatusCode::CONFLICT, Json(ErrorResponse { error: "Alias already taken".to_string() })).into_response();
        }
        
        // An alias previously used by a now-deleted link cannot be reused: the
        // global UNIQUE on links.code still holds that code, so an insert would
        // fail. Reject explicitly with a clear message rather than 500 later.
        let exists_deleted = links::Entity::find()
            .filter(links::Column::Code.eq(&alias))
            .filter(links::Column::DeletedAt.is_not_null())
            .one(&state.db)
            .await
            .unwrap_or(None);
        if exists_deleted.is_some() {
            return (StatusCode::CONFLICT, Json(ErrorResponse { error: "This alias was previously used and cannot be reused".to_string() })).into_response();
        }

        alias
    } else {
        let mut code = generate_short_code();
        while links::Entity::find().filter(links::Column::Code.eq(&code)).one(&state.db).await.unwrap_or(None).is_some() {
            code = generate_short_code();
        }
        code
    };

    let password_hash = if let Some(password) = &payload.password {
        match hash(password, DEFAULT_COST) {
            Ok(h) => Some(h),
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to hash password").into_response(),
        }
    } else {
        None
    };

    // If org_id is provided, verify user is a member
    if let Some(org_id) = payload.org_id {
        if let Some(uid) = user_id {
            use crate::entity::org_members;
            let is_member = org_members::Entity::find()
                .filter(org_members::Column::OrgId.eq(org_id))
                .filter(org_members::Column::UserId.eq(uid))
                .one(&state.db)
                .await
                .ok()
                .flatten()
                .is_some();
            
            if !is_member {
                return (StatusCode::FORBIDDEN, Json(ErrorResponse { error: "Not a member of this organization".to_string() })).into_response();
            }
        } else {
            return (StatusCode::FORBIDDEN, Json(ErrorResponse { error: "Authentication required to create org links".to_string() })).into_response();
        }
    }

    // Validate scheduling / limit inputs.
    if let Some(max) = payload.max_clicks {
        if max <= 0 {
            return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "max_clicks must be greater than 0".to_string() })).into_response();
        }
    }
    if let (Some(starts), Some(expires)) = (payload.starts_at, payload.expires_at) {
        if starts >= expires {
            return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "starts_at must be before expires_at".to_string() })).into_response();
        }
    }

    // Verify folder ownership if one was specified (prevents assigning the link
    // into another user's folder).
    if let Some(folder_id) = payload.folder_id {
        let allowed = match user_id {
            Some(uid) => user_can_use_folder(&state.db, folder_id, uid).await,
            None => false,
        };
        if !allowed {
            return (StatusCode::FORBIDDEN, Json(ErrorResponse { error: "Folder not found or access denied".to_string() })).into_response();
        }
    }

    // Burn-after-reading (gated by ENABLE_BURN_AFTER_READING). A burn link needs a
    // click cap to ride the existing max_clicks enforcement; default to one-time use.
    let burn_enabled = std::env::var("ENABLE_BURN_AFTER_READING")
        .map(|v| v != "false")
        .unwrap_or(true);
    let burn_after_reading = burn_enabled && payload.burn_after_reading.unwrap_or(false);
    let effective_max_clicks = if burn_after_reading && payload.max_clicks.is_none() {
        Some(1)
    } else {
        payload.max_clicks
    };

    // Safe-link interstitial (gated by ENABLE_SAFE_LINK_INTERSTITIAL).
    let interstitial_enabled = std::env::var("ENABLE_SAFE_LINK_INTERSTITIAL")
        .map(|v| v != "false")
        .unwrap_or(true);
    let safe_link_interstitial =
        interstitial_enabled && payload.safe_link_interstitial.unwrap_or(false);

    let link = links::ActiveModel {
        original_url: Set(validated_url.clone()),
        code: Set(code.clone()),
        user_id: Set(user_id),
        expires_at: Set(payload.expires_at.map(|d| d.naive_utc())),
        password_hash: Set(password_hash.clone()),
        title: Set(payload.title.clone()),
        notes: Set(payload.notes.clone()),
        folder_id: Set(payload.folder_id),
        org_id: Set(payload.org_id),
        starts_at: Set(payload.starts_at.map(|d| d.naive_utc())),
        max_clicks: Set(effective_max_clicks),
        burn_after_reading: Set(burn_after_reading),
        safe_link_interstitial: Set(safe_link_interstitial),
        ..Default::default()
    };

    // Insert the link and its tags atomically so a tag failure can't leave a
    // half-created link behind.
    let txn = match state.db.begin().await {
        Ok(t) => t,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Database error".to_string() })).into_response(),
    };

    let link_id = match links::Entity::insert(link).exec(&txn).await {
        Ok(link_res) => link_res.last_insert_id,
        Err(_) => {
            let _ = txn.rollback().await;
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Database error".to_string() })).into_response();
        }
    };

    // Add tags if provided
    if let Some(tag_ids) = payload.tag_ids {
        for tag_id in tag_ids {
            let link_tag = link_tags::ActiveModel {
                link_id: Set(link_id),
                tag_id: Set(tag_id),
                ..Default::default()
            };
            if link_tag.insert(&txn).await.is_err() {
                let _ = txn.rollback().await;
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Failed to attach tags".to_string() })).into_response();
            }
        }
    }

    if txn.commit().await.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Database error".to_string() })).into_response();
    }

    let tags = get_link_tags(&state.db, link_id).await;
    let base_url = get_base_url();
    let api_url = get_api_url();
    (StatusCode::CREATED, Json(LinkResponse {
        id: link_id,
        code: code.clone(),
        short_url: format!("{}/{}", base_url, code),
        api_url: format!("{}/{}", api_url, code),
        original_url: payload.original_url,
        title: payload.title,
        click_count: 0,
        created_at: chrono::Utc::now().to_rfc3339(),
        expires_at: payload.expires_at.map(|d| d.to_rfc3339()),
        has_password: password_hash.is_some(),
        notes: payload.notes,
        folder_id: payload.folder_id,
        org_id: payload.org_id,
        starts_at: payload.starts_at.map(|d| d.to_rfc3339()),
        max_clicks: effective_max_clicks,
        burn_after_reading,
        burned_at: None,
        safe_link_interstitial,
        bio_visible: false,
        is_active: true,
        is_pinned: false,
        tags,
    })).into_response()
}

#[derive(Serialize, ToSchema)]
pub struct ReputationInfo {
    /// "safe" | "suspicious" | "malicious" | "unknown"
    pub verdict: String,
    /// Where the verdict came from, e.g. "internal_blocklist".
    pub source: String,
}

#[derive(Serialize, ToSchema)]
pub struct LinkPreviewResponse {
    pub code: String,
    pub short_url: String,
    pub original_url: String,
    pub domain: String,
    pub has_password: bool,
    pub is_expired: bool,
    pub created_at: String,
    pub click_count: i32,
    /// Destination reputation signal for the safe-link interstitial.
    pub reputation: ReputationInfo,
    /// Whether the instance has the safe-link interstitial feature enabled.
    pub interstitial_enabled: bool,
    /// Whether this link opted into showing the interstitial before redirecting.
    pub safe_link_interstitial: bool,
}

/// Get link preview (add + to any short link URL to see preview)
#[utoipa::path(
    get,
    path = "/{code}/preview",
    params(
        ("code" = String, Path, description = "Short link code")
    ),
    responses(
        (status = 200, description = "Link preview", body = LinkPreviewResponse),
        (status = 404, description = "Link not found"),
    ),
    tag = "Links"
)]
pub async fn preview_link(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> impl IntoResponse {
    // Remove trailing + if present (for URL compatibility)
    let clean_code = code.trim_end_matches('+');
    
    let link = links::Entity::find()
        .filter(links::Column::Code.eq(clean_code))
        .filter(links::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    match link {
        Some(link) => {
            let is_expired = if let Some(exp) = link.expires_at {
                exp < Utc::now().naive_utc()
            } else {
                false
            };

            // Extract domain from original URL
            let domain = url::Url::parse(&link.original_url)
                .map(|u| u.host_str().unwrap_or("unknown").to_string())
                .unwrap_or_else(|_| "unknown".to_string());

            let base_url = get_base_url();

            // Reputation: the internal blocklist is the source of truth we have
            // today. Blocked → malicious; plain HTTP can't be vouched for → unknown;
            // otherwise we have nothing bad on record → safe. (Fails open.)
            let interstitial_enabled = std::env::var("ENABLE_SAFE_LINK_INTERSTITIAL")
                .map(|v| v != "false")
                .unwrap_or(true);
            let verdict = if check_blocked(&state.db, &link.original_url).await.is_err() {
                "malicious"
            } else if link.original_url.starts_with("https://") {
                "safe"
            } else {
                "unknown"
            };

            // Do not disclose the destination of a password-protected or
            // burn-after-reading link through the public, unauthenticated
            // preview: the password gate exists to keep the destination secret,
            // and a burn link is a one-time secret (preview does not consume the
            // burn). Plain links — including plain safe-link-interstitial links —
            // still show their destination so the interstitial can render it.
            let protected = link.password_hash.is_some() || link.burn_after_reading;
            let (shown_url, shown_domain) = if protected {
                (String::new(), String::new())
            } else {
                (link.original_url.clone(), domain)
            };

            (StatusCode::OK, Json(LinkPreviewResponse {
                code: link.code.clone(),
                short_url: format!("{}/{}", base_url, link.code),
                original_url: shown_url,
                domain: shown_domain,
                has_password: link.password_hash.is_some(),
                is_expired,
                created_at: link.created_at.to_string(),
                click_count: link.click_count,
                reputation: ReputationInfo {
                    verdict: verdict.to_string(),
                    source: "internal_blocklist".to_string(),
                },
                interstitial_enabled,
                safe_link_interstitial: link.safe_link_interstitial,
            })).into_response()
        }
        None => {
            (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Link not found".to_string() })).into_response()
        }
    }
}

#[derive(Deserialize, Default)]
pub struct RedirectQuery {
    /// Set to `1` after the visitor confirms the safe-link interstitial in the SPA.
    confirm: Option<String>,
}

/// Redirect to original URL
#[utoipa::path(
    get,
    path = "/{code}",
    params(
        ("code" = String, Path, description = "Short link code")
    ),
    responses(
        (status = 302, description = "Redirect to original URL"),
        (status = 401, description = "Password required"),
        (status = 404, description = "Link not found"),
        (status = 410, description = "Link expired or inactive"),
    ),
    tag = "Links"
)]
pub async fn redirect_link(
    State(state): State<AppState>,
    Path(code): Path<String>,
    Query(query): Query<RedirectQuery>,
    headers: HeaderMap,
) -> impl IntoResponse {
    use crate::utils::cache::CachedLink;
    
    // Try to get from Redis cache first (for non-password-protected links)
    if let Some(cache) = &state.redis_cache {
        if let Some(cached) = cache.get_link(&code).await {
            // Skip cache for password-protected links, max_clicks links, and
            // interstitial links (need per-request interstitial/confirm handling).
            if !cached.has_password && cached.max_clicks.is_none() && !cached.safe_link_interstitial {
                // Check if link is active based on cached data
                let now = chrono::Utc::now().timestamp();
                
                if let Some(starts_at) = cached.starts_at {
                    if now < starts_at {
                        return (StatusCode::GONE, "Link is scheduled to activate later").into_response();
                    }
                }
                
                if let Some(expires_at) = cached.expires_at {
                    if now > expires_at {
                        return (StatusCode::GONE, "Link has expired").into_response();
                    }
                }
                
                if let Some(max_clicks) = cached.max_clicks {
                    if cached.click_count >= max_clicks {
                        return (StatusCode::GONE, "Link has reached maximum clicks").into_response();
                    }
                }
                
                // Record click using buffer (synchronous, non-blocking).
                // Only uncapped links reach the cache fast-path (gated above),
                // so the buffer owns the aggregate count here.
                record_click_buffered(
                    &state.click_buffer,
                    state.ws_state.as_ref().map(|w| w.as_ref()),
                    cached.id,
                    &code,
                    cached.user_id,
                    ClickAccounting::Buffered { db_click_count: cached.click_count },
                    &headers,
                );
                
                // Invalidate cache after click
                let cache_clone = state.redis_cache.clone();
                let code_clone = code.clone();
                tokio::spawn(async move {
                    if let Some(c) = cache_clone {
                        let _ = c.invalidate_link(&code_clone).await;
                    }
                });
                
                return Redirect::temporary(&cached.original_url).into_response();
            }
        }
    }
    
    // Fallback to database lookup
    let link = links::Entity::find()
        .filter(links::Column::Code.eq(&code))
        .filter(links::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(link) = link {
        // Check if link is active
        if !link.is_active() {
            let reason = link.inactive_reason().unwrap_or("Link is inactive");
            return (StatusCode::GONE, reason).into_response();
        }

        // Enforce content blocking at redirect time so a block applied after the
        // link was created is retroactive. Runs before the caching block below, so
        // a blocked link is never (re)written to the cache.
        if check_blocked(&state.db, &link.original_url).await.is_err() {
            return (StatusCode::GONE, "This link has been disabled").into_response();
        }

        // Advisory fast-fail for capped links, e.g. so an exhausted link 410s
        // before prompting for a password or interstitial, and so counts still
        // buffered from before a cap was added are respected. This read is NOT
        // the enforcement point — N concurrent requests can all pass it before
        // any click is recorded. The authoritative check is the atomic
        // conditional UPDATE below (consume_capped_click), which runs once the
        // request is actually going to be served a destination.
        if let Some(max) = link.max_clicks {
            if link.click_count + state.click_buffer.pending_count(link.id) >= max {
                let msg = if link.burn_after_reading {
                    "This one-time link has already been opened"
                } else {
                    "Link has reached maximum clicks"
                };
                return (StatusCode::GONE, msg).into_response();
            }
        }

        if link.password_hash.is_some() {
            let provided_password = headers.get("x-link-password")
                .and_then(|h| h.to_str().ok());

            if let Some(pwd) = provided_password {
                // Anti-bruteforce: this inline password check lives on the redirect
                // hot path, which the rate-limit middleware classifies as a redirect
                // (100/s) — so it is NOT covered by the 5/min password_verify limiter
                // that guards POST /:code/verify. Enforce that same limiter here,
                // keyed by IP+code, before spending a (deliberately slow) bcrypt.
                let ip = crate::utils::rate_limiter::client_ip_from_headers(&headers)
                    .unwrap_or_else(|| "unknown".to_string());
                if let crate::utils::rate_limiter::RateLimitResult::Limited { retry_after_secs, .. } =
                    state.rate_limiters.password_verify.check(&format!("pwverify:{}:{}", ip, code))
                {
                    return (
                        StatusCode::TOO_MANY_REQUESTS,
                        [("Retry-After", retry_after_secs.to_string())],
                        "Too many password attempts. Try again later.",
                    )
                        .into_response();
                }

                if let Some(hash_str) = &link.password_hash {
                    // bcrypt::verify is CPU-heavy (cost 12, ~250ms) and blocking.
                    // Run it on the blocking pool so a burst of attempts can't
                    // starve the async runtime's worker threads.
                    let pwd = pwd.to_string();
                    let hash_str = hash_str.clone();
                    let verified = tokio::task::spawn_blocking(move || {
                        bcrypt::verify(&pwd, &hash_str).unwrap_or(false)
                    })
                    .await
                    .unwrap_or(false);
                    if !verified {
                        return (StatusCode::UNAUTHORIZED, "Invalid password").into_response();
                    }
                }
            } else {
                let frontend_url = std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:5173".to_string());
                return Redirect::temporary(&format!("{}/password/{}", frontend_url, code)).into_response();
            }
        }

        if link.safe_link_interstitial
            && interstitial_feature_enabled()
            && !redirect_confirmed(query.confirm.as_deref())
        {
            return frontend_interstitial_redirect(&code);
        }

        // Smart conditional routing. When enabled and this link has rules, resolve a
        // per-request destination from the visitor's device/OS/country/language.
        // Routed links are never cached (resolution is per-request), so they always
        // reach this DB path. When the flag is off, rules are ignored and the link
        // degrades to a plain redirect. Resolved (and blocklist-checked) BEFORE the
        // cap consume below, so a blocked routed destination can't waste a click
        // slot or burn a one-time link without serving anything.
        let routing_enabled = std::env::var("ENABLE_CONDITIONAL_ROUTING")
            .map(|v| v != "false")
            .unwrap_or(true);
        let routing_rules = if routing_enabled {
            crate::entity::routing_rules::Entity::find()
                .filter(crate::entity::routing_rules::Column::LinkId.eq(link.id))
                .order_by_asc(crate::entity::routing_rules::Column::Priority)
                .all(&state.db)
                .await
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let routed_destination = if !routing_rules.is_empty() {
            let ip = crate::utils::rate_limiter::client_ip_from_headers(&headers);
            let geo = ip.as_ref().map(|ip| lookup_ip(ip)).unwrap_or_default();
            let ua_info = headers
                .get("user-agent")
                .and_then(|h| h.to_str().ok())
                .map(parse_user_agent)
                .unwrap_or_default();
            let accept_language = headers.get("accept-language").and_then(|h| h.to_str().ok());

            let destination = crate::utils::routing::resolve_destination(
                &routing_rules,
                &ua_info,
                &geo,
                accept_language,
                &link.original_url,
            );

            // A routing rule must not be able to bypass the blocklist.
            if check_blocked(&state.db, &destination).await.is_err() {
                return (StatusCode::GONE, "This link has been disabled").into_response();
            }
            Some(destination)
        } else {
            None
        };

        // Authoritative cap enforcement. From here on this request will be
        // served a destination, so for capped links the click is consumed NOW
        // with a single conditional UPDATE (click_count < max_clicks). Under
        // concurrency exactly max_clicks requests can win this update — a
        // burn-after-reading link is exactly-once. 0 rows updated means a
        // concurrent request took the last slot (or, transiently, that the cap
        // was just removed / the link soft-deleted by a concurrent update — a
        // retry then sees the link's new state).
        let accounting = if link.max_clicks.is_some() {
            match consume_capped_click(&state.db, link.id).await {
                Ok(Some(new_count)) => ClickAccounting::Consumed { new_click_count: new_count },
                Ok(None) => {
                    let msg = if link.burn_after_reading {
                        "This one-time link has already been opened"
                    } else {
                        "Link has reached maximum clicks"
                    };
                    return (StatusCode::GONE, msg).into_response();
                }
                // Fail closed: a capped (possibly burn) link must never
                // redirect without its click being counted.
                Err(_) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
                }
            }
        } else {
            ClickAccounting::Buffered { db_click_count: link.click_count }
        };

        if let Some(destination) = routed_destination {
            record_click_buffered(
                &state.click_buffer,
                state.ws_state.as_ref().map(|w| w.as_ref()),
                link.id,
                &code,
                link.user_id,
                accounting,
                &headers,
            );
            return Redirect::temporary(&destination).into_response();
        }

        // Cache the link for future requests (only plain redirects — no password,
        // click cap, or interstitial, which need the DB path).
        if link.password_hash.is_none() && link.max_clicks.is_none() && !link.safe_link_interstitial {
            if let Some(cache) = &state.redis_cache {
                let cached = CachedLink {
                    id: link.id,
                    original_url: link.original_url.clone(),
                    has_password: false,
                    expires_at: link.expires_at.map(|e| e.and_utc().timestamp()),
                    starts_at: link.starts_at.map(|s| s.and_utc().timestamp()),
                    max_clicks: link.max_clicks,
                    click_count: link.click_count,
                    user_id: link.user_id,
                    safe_link_interstitial: link.safe_link_interstitial,
                };
                let _ = cache.set_link(&code, &cached).await;
            }
        }

        // Record click using buffer. For capped links the aggregate count (and
        // the burned_at stamp when the cap is exhausted) was already handled
        // atomically by consume_capped_click; only the analytics row and the
        // realtime broadcast go through here.
        record_click_buffered(
            &state.click_buffer,
            state.ws_state.as_ref().map(|w| w.as_ref()),
            link.id,
            &code,
            link.user_id,
            accounting,
            &headers,
        );

        Redirect::temporary(&link.original_url).into_response()
    } else {
        (StatusCode::NOT_FOUND, "Link not found").into_response()
    }
}

/// How a click's aggregate count is accounted, so it is counted exactly once.
#[derive(Clone, Copy)]
enum ClickAccounting {
    /// Uncapped link: the click buffer owns the count — the per-link counter is
    /// incremented and added to `links.click_count` at flush.
    Buffered { db_click_count: i32 },
    /// Capped (max_clicks) link: the count was already consumed atomically at
    /// the DB by `consume_capped_click`, so only the analytics event row is
    /// buffered — incrementing the counter too would double-count at flush.
    Consumed { new_click_count: i32 },
}

/// Atomically consume one click slot on a capped (`max_clicks`) link.
///
/// A single conditional UPDATE so concurrent redirects cannot overshoot the
/// cap: only a row with `click_count < max_clicks` is incremented, and
/// `burned_at` is stamped in the same statement when this click exhausts a
/// burn-after-reading link. Returns `Ok(Some(new_click_count))` when a slot
/// was consumed, `Ok(None)` when the cap is already exhausted.
async fn consume_capped_click(
    db: &DatabaseConnection,
    link_id: i32,
) -> Result<Option<i32>, DbErr> {
    let stmt = Statement::from_sql_and_values(
        DbBackend::Postgres,
        r#"UPDATE links
           SET click_count = click_count + 1,
               burned_at = CASE
                   WHEN burn_after_reading AND burned_at IS NULL AND click_count + 1 >= max_clicks
                       THEN $2
                   ELSE burned_at
               END
           WHERE id = $1
             AND deleted_at IS NULL
             AND max_clicks IS NOT NULL
             AND click_count < max_clicks
           RETURNING click_count"#,
        [link_id.into(), chrono::Utc::now().naive_utc().into()],
    );
    let row = db.query_one(stmt).await?;
    row.map(|r| r.try_get::<i32>("", "click_count")).transpose()
}

/// Helper function to record a click event using the click buffer
fn record_click_buffered(
    click_buffer: &crate::utils::ClickBuffer,
    ws_state: Option<&crate::handlers::websocket::WsState>,
    link_id: i32,
    link_code: &str,
    user_id: Option<i32>,
    accounting: ClickAccounting,
    headers: &HeaderMap,
) {
    use crate::utils::click_buffer::ClickData;

    // Client IP via the same trust rules as the rate limiter (no spoofable
    // first-XFF token in analytics/geo either).
    let ip = crate::utils::rate_limiter::client_ip_from_headers(headers);

    let user_agent = headers.get("user-agent")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    // Store only the referring host, never the full URL — its path/query can
    // carry visitor PII we neither need nor want to retain.
    let referer = headers.get("referer")
        .and_then(|h| h.to_str().ok())
        .and_then(crate::utils::privacy::anonymize_referer);

    // GeoIP lookup
    let geo = ip.as_ref().map(|ip| lookup_ip(ip)).unwrap_or_default();
    
    // Parse user agent
    let ua_info = user_agent.as_ref().map(|ua| parse_user_agent(ua)).unwrap_or_default();

    // Add to click buffer instead of writing directly. Only the truncated IP
    // is stored (IPv4 /24, IPv6 /48) — the full address is used above for the
    // geo lookup and then dropped.
    let click_data = ClickData {
        link_id,
        ip_address: ip.as_deref().and_then(crate::utils::privacy::anonymize_ip),
        user_agent,
        referer,
        country: geo.country.clone(),
        city: geo.city.clone(),
        region: geo.region,
        latitude: geo.latitude,
        longitude: geo.longitude,
        device: ua_info.device.clone(),
        browser: ua_info.browser.clone(),
        os: ua_info.os,
    };
    match accounting {
        ClickAccounting::Buffered { .. } => click_buffer.add_click(click_data),
        ClickAccounting::Consumed { .. } => click_buffer.add_event_only(click_data),
    }

    // Broadcast real-time event
    let new_click_count = match accounting {
        ClickAccounting::Buffered { db_click_count } => db_click_count + 1,
        ClickAccounting::Consumed { new_click_count } => new_click_count,
    };
    if let Some(ws) = ws_state {
        let event = ClickEvent {
            link_id,
            link_code: link_code.to_string(),
            user_id,
            click_count: new_click_count,
            country: geo.country,
            city: geo.city,
            device: ua_info.device,
            browser: ua_info.browser,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        ws.broadcast_click(event);
    }
}

/// Verify password for protected link
#[utoipa::path(
    post,
    path = "/{code}/verify",
    params(
        ("code" = String, Path, description = "Short link code")
    ),
    request_body = VerifyPasswordRequest,
    responses(
        (status = 200, description = "Password verified"),
        (status = 401, description = "Invalid password"),
        (status = 404, description = "Link not found"),
        (status = 410, description = "Link expired"),
    ),
    tag = "Links"
)]
pub async fn verify_link_password(
    State(state): State<AppState>,
    Path(code): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<VerifyPasswordRequest>,
) -> impl IntoResponse {
    let link = links::Entity::find()
        .filter(links::Column::Code.eq(&code))
        .filter(links::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(link) = link {
        if !link.is_active() {
            let reason = link.inactive_reason().unwrap_or("Link is inactive");
            return (StatusCode::GONE, Json(ErrorResponse { error: reason.to_string() })).into_response();
        }

        // Advisory parity with redirect_link: respect clicks still buffered
        // from before a cap was added. The atomic consume below is the
        // enforcement point.
        if let Some(max) = link.max_clicks {
            if link.click_count + state.click_buffer.pending_count(link.id) >= max {
                let msg = if link.burn_after_reading {
                    "This one-time link has already been opened"
                } else {
                    "Link has reached maximum clicks"
                };
                return (StatusCode::GONE, Json(ErrorResponse { error: msg.to_string() })).into_response();
            }
        }

        if let Some(hash_str) = &link.password_hash {
            if !bcrypt::verify(&payload.password, hash_str).unwrap_or(false) {
                return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "Invalid password".to_string() })).into_response();
            }
        }

        // This endpoint discloses the destination URL (after verification for
        // password links; historically for passwordless links too), so capped
        // links must consume their click atomically here exactly like a
        // redirect — otherwise a burst of verifies opens a burn link more than
        // once, and the passwordless form would leak it without any count.
        let accounting = if link.max_clicks.is_some() {
            match consume_capped_click(&state.db, link.id).await {
                Ok(Some(new_count)) => ClickAccounting::Consumed { new_click_count: new_count },
                Ok(None) => {
                    let msg = if link.burn_after_reading {
                        "This one-time link has already been opened"
                    } else {
                        "Link has reached maximum clicks"
                    };
                    return (StatusCode::GONE, Json(ErrorResponse { error: msg.to_string() })).into_response();
                }
                // Fail closed: never disclose the URL uncounted.
                Err(_) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Database error".to_string() })).into_response();
                }
            }
        } else {
            ClickAccounting::Buffered { db_click_count: link.click_count }
        };

        // Password links count every unlock as a click (legacy behavior, same
        // as a redirect). Passwordless links only record when the cap consumed
        // one above — uncapped passwordless verifies stay uncounted as before.
        if link.password_hash.is_some() || matches!(accounting, ClickAccounting::Consumed { .. }) {
            record_click_buffered(
                &state.click_buffer,
                state.ws_state.as_ref().map(|w| w.as_ref()),
                link.id,
                &link.code,
                link.user_id,
                accounting,
                &headers,
            );
        }

        return (StatusCode::OK, Json(serde_json::json!({ "url": link.original_url }))).into_response();
    }

    (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Link not found".to_string() })).into_response()
}

/// Get QR code for a link
#[utoipa::path(
    get,
    path = "/links/{id}/qr",
    params(
        ("id" = i32, Path, description = "Link ID")
    ),
    responses(
        (status = 200, description = "QR code image", content_type = "image/png"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Link not found"),
    ),
    tag = "Links"
)]
pub async fn get_qr_code(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Query(opts): Query<QrOptions>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Verify authentication
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
    };

    let link = links::Entity::find_by_id(id)
        .filter(links::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(link) = link {
        // Verify ownership (allow if user owns the link or it belongs to their org)
        let has_access = if link.user_id == Some(user_id) {
            true
        } else if let Some(org_id) = link.org_id {
            use crate::entity::org_members;
            org_members::Entity::find()
                .filter(org_members::Column::OrgId.eq(org_id))
                .filter(org_members::Column::UserId.eq(user_id))
                .one(&state.db)
                .await
                .ok()
                .flatten()
                .is_some()
        } else {
            false
        };
        
        if !has_access {
            return (StatusCode::FORBIDDEN, "You don't have permission to access this link").into_response();
        }

        let url = format!("{}/{}", get_base_url(), link.code);

        // QR branding is a non-destructive kill-switch (default ON). When disabled,
        // all options are ignored and we serve the plain black/white PNG, which is
        // byte-identical to the legacy behavior.
        let branding_enabled = std::env::var("ENABLE_QR_BRANDING")
            .map(|v| v != "false")
            .unwrap_or(true);
        let effective = if branding_enabled { opts } else { QrOptions::default() };

        match build_qr_image(&url, &effective) {
            Some((bytes, content_type)) => (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, content_type)],
                bytes,
            ).into_response(),
            None => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to generate QR code").into_response(),
        }
    } else {
        (StatusCode::NOT_FOUND, "Link not found").into_response()
    }
}

/// Query options for branded QR rendering. All optional — an empty set renders
/// the plain black/white PNG (byte-identical to the legacy behavior).
#[derive(Debug, Default, Deserialize)]
pub struct QrOptions {
    /// Foreground (module) color as hex, with or without `#`, e.g. `2f37d8`.
    pub color: Option<String>,
    /// Background color as hex. Defaults to white.
    pub bg: Option<String>,
    /// Overlay the brand mark in the center (uses higher error-correction).
    pub logo: Option<bool>,
    /// Output format: `png` (default) or `svg`.
    pub format: Option<String>,
    /// Target PNG size in pixels (clamped to 256..=1024). Ignored for SVG.
    pub size: Option<u32>,
}

/// Brand mark embedded at compile time (square cobalt app icon). Decoded once.
/// `include_bytes!` means there is no runtime filesystem dependency, so the
/// minimal Docker runtime image needs no extra COPY. Self-disables to a plain
/// QR if the bytes ever fail to decode.
static QR_LOGO: once_cell::sync::Lazy<Option<image::DynamicImage>> =
    once_cell::sync::Lazy::new(|| {
        image::load_from_memory(include_bytes!("../../assets/qr-logo.png")).ok()
    });

/// Parse a `#rrggbb` / `rrggbb` hex string into RGB. Returns None on any malformed
/// input so callers can fall back to the default color.
fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    Some([
        u8::from_str_radix(&s[0..2], 16).ok()?,
        u8::from_str_radix(&s[2..4], 16).ok()?,
        u8::from_str_radix(&s[4..6], 16).ok()?,
    ])
}

/// Strip baked-in white/near-white backgrounds and recolor the mark to `fg`.
///
/// Transparent pixels keep `fg` as their RGB (only their alpha is zeroed) so a
/// downstream Lanczos resize interpolates a constant color — no dark/colored
/// fringe along the mark's edges.
fn tinted_logo_rgba(fg: [u8; 3]) -> Option<image::RgbaImage> {
    let logo = QR_LOGO.as_ref()?;
    let rgba = logo.to_rgba8();
    let mut out = image::RgbaImage::new(rgba.width(), rgba.height());
    for (x, y, pixel) in rgba.enumerate_pixels() {
        let [r, g, b, a] = pixel.0;
        let alpha = if a < 24 || (r > 232 && g > 232 && b > 232) { 0 } else { a };
        out.put_pixel(x, y, image::Rgba([fg[0], fg[1], fg[2], alpha]));
    }
    Some(out)
}

/// Linear blend of `over` onto `base` by `alpha` in [0,1].
fn blend_channel(base: u8, over: u8, alpha: f32) -> u8 {
    (over as f32 * alpha + base as f32 * (1.0 - alpha)).round().clamp(0.0, 255.0) as u8
}

/// Composite the brand mark into the center of an RGBA QR raster, on a
/// circular background-colored backplate so the occluded modules read cleanly.
/// The backplate edge is anti-aliased (1px coverage ramp) to avoid jaggies.
fn overlay_logo(img: &mut image::RgbaImage, bg: [u8; 3], fg: [u8; 3]) {
    let logo = match tinted_logo_rgba(fg) {
        Some(l) => l,
        None => return,
    };
    let (w, h) = img.dimensions();
    // Logo occupies ~22% of the QR width; the backplate is a touch larger.
    let target = ((w as f32) * 0.22) as u32;
    if target == 0 {
        return;
    }
    let plate = (target as f32) * 1.22;
    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;
    let radius = plate / 2.0;
    let x0 = (cx - radius - 1.0).floor().max(0.0) as u32;
    let x1 = (cx + radius + 1.0).ceil().min(w as f32) as u32;
    let y0 = (cy - radius - 1.0).floor().max(0.0) as u32;
    let y1 = (cy + radius + 1.0).ceil().min(h as f32) as u32;
    for yy in y0..y1 {
        for xx in x0..x1 {
            let dx = xx as f32 + 0.5 - cx;
            let dy = yy as f32 + 0.5 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            // 1 fully inside, 0 fully outside, linear ramp across the edge.
            let coverage = (radius + 0.5 - dist).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let cur = img.get_pixel(xx, yy).0;
            img.put_pixel(xx, yy, image::Rgba([
                blend_channel(cur[0], bg[0], coverage),
                blend_channel(cur[1], bg[1], coverage),
                blend_channel(cur[2], bg[2], coverage),
                255,
            ]));
        }
    }
    let resized = image::imageops::resize(
        &logo,
        target,
        target,
        image::imageops::FilterType::Lanczos3,
    );
    let lx = ((w - target) / 2) as i64;
    let ly = ((h - target) / 2) as i64;
    image::imageops::overlay(img, &resized, lx, ly);
}

/// Render a QR for `url` per `opts`. Returns `(bytes, content_type)`.
///
/// Pure (no DB / auth / env) so it is unit-testable. When no options are set it
/// renders the plain Luma PNG exactly as before. Invalid hex / unknown formats
/// fall back gracefully rather than erroring.
fn build_qr_image(url: &str, opts: &QrOptions) -> Option<(Vec<u8>, &'static str)> {
    use qrcode::{EcLevel, QrCode};
    use std::io::Cursor;

    let want_logo = opts.logo.unwrap_or(false);
    let fmt = opts.format.as_deref().unwrap_or("png").to_lowercase();
    let fg = opts.color.as_deref().and_then(parse_hex);
    let bg = opts.bg.as_deref().and_then(parse_hex).unwrap_or([255, 255, 255]);
    let dark = fg.unwrap_or([0, 0, 0]);

    // A center logo occludes modules, so request high error-correction for it.
    let qr = if want_logo {
        QrCode::with_error_correction_level(url.as_bytes(), EcLevel::H)
    } else {
        QrCode::new(url.as_bytes())
    }
    .ok()?;

    if fmt == "svg" {
        use qrcode::render::svg;
        let fg_hex = fg
            .map(|c| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]))
            .unwrap_or_else(|| "#000000".to_string());
        let bg_hex = format!("#{:02x}{:02x}{:02x}", bg[0], bg[1], bg[2]);
        let mut svg_xml = qr
            .render::<svg::Color>()
            .dark_color(svg::Color(&fg_hex))
            .light_color(svg::Color(&bg_hex))
            .quiet_zone(true)
            .min_dimensions(256, 256)
            .build();
        if want_logo {
            if let (Some(uri), Some(dim)) = (qr_logo_data_uri_tinted(dark), parse_svg_width(&svg_xml)) {
                let logo_sz = ((dim as f32) * 0.22) as u32;
                let pos = (dim - logo_sz) / 2;
                let center = dim as f32 / 2.0;
                // Circular backplate (matches the PNG's 1.22× plate) so the logo
                // reads cleanly over the modules instead of sitting bare on them.
                let plate_r = (logo_sz as f32 * 1.22) / 2.0;
                let backplate = format!(
                    "<circle cx=\"{c}\" cy=\"{c}\" r=\"{r:.2}\" fill=\"{bg}\"/>",
                    c = center, r = plate_r, bg = bg_hex
                );
                let img_tag = format!(
                    "<image x=\"{x}\" y=\"{y}\" width=\"{s}\" height=\"{s}\" href=\"{href}\" preserveAspectRatio=\"xMidYMid meet\"/>",
                    x = pos, y = pos, s = logo_sz, href = uri
                );
                svg_xml = svg_xml.replace("</svg>", &format!("{}{}</svg>", backplate, img_tag));
            }
        }
        return Some((svg_xml.into_bytes(), "image/svg+xml"));
    }

    let size = opts.size.unwrap_or(512).clamp(256, 1024);
    let bytes = if fg.is_some() || bg != [255, 255, 255] || want_logo {
        // Colored / branded → RGBA raster.
        let mut img = qr
            .render::<image::Rgba<u8>>()
            .dark_color(image::Rgba([dark[0], dark[1], dark[2], 255]))
            .light_color(image::Rgba([bg[0], bg[1], bg[2], 255]))
            .quiet_zone(true)
            .min_dimensions(size, size)
            .build();
        if want_logo {
            overlay_logo(&mut img, bg, dark);
        }
        let mut buf = Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png).ok()?;
        buf.into_inner()
    } else {
        // Plain path: identical output to the legacy handler.
        let img = qr.render::<image::Luma<u8>>().build();
        let mut buf = Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png).ok()?;
        buf.into_inner()
    };
    Some((bytes, "image/png"))
}

/// Base64 `data:` URI of the brand mark tinted to `fg`, for branded SVG output.
fn qr_logo_data_uri_tinted(fg: [u8; 3]) -> Option<String> {
    use base64::Engine as _;
    use std::io::Cursor;
    let tinted = tinted_logo_rgba(fg)?;
    let mut buf = Cursor::new(Vec::new());
    tinted.write_to(&mut buf, image::ImageFormat::Png).ok()?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
    Some(format!("data:image/png;base64,{b64}"))
}

/// Extract the `width="N"` integer from a qrcode-rendered SVG header.
fn parse_svg_width(svg: &str) -> Option<u32> {
    let marker = "width=\"";
    let start = svg.find(marker)? + marker.len();
    let rest = &svg[start..];
    let end = rest.find('"')?;
    rest[..end].parse().ok()
}

#[cfg(test)]
mod qr_render_tests {
    use super::{build_qr_image, parse_hex, QrOptions};

    const PNG_MAGIC: &[u8] = &[0x89, b'P', b'N', b'G'];

    fn opts(
        color: Option<&str>,
        logo: Option<bool>,
        format: Option<&str>,
    ) -> QrOptions {
        QrOptions {
            color: color.map(|s| s.to_string()),
            bg: None,
            logo,
            format: format.map(|s| s.to_string()),
            size: None,
        }
    }

    #[test]
    fn plain_is_valid_png() {
        let (bytes, ct) = build_qr_image("https://opn.onl/abc123", &QrOptions::default()).unwrap();
        assert_eq!(ct, "image/png");
        assert!(bytes.starts_with(PNG_MAGIC));
        // Decodes as a real image.
        assert!(image::load_from_memory(&bytes).is_ok());
    }

    #[test]
    fn colored_is_valid_png() {
        let (bytes, ct) =
            build_qr_image("https://opn.onl/abc123", &opts(Some("2f37d8"), None, None)).unwrap();
        assert_eq!(ct, "image/png");
        assert!(image::load_from_memory(&bytes).is_ok());
    }

    #[test]
    fn hash_prefixed_color_ok() {
        let (bytes, _) =
            build_qr_image("https://opn.onl/x", &opts(Some("#2f37d8"), None, None)).unwrap();
        assert!(image::load_from_memory(&bytes).is_ok());
    }

    #[test]
    fn invalid_color_falls_back() {
        // Garbage hex must not error — it falls back to a plain render.
        let (bytes, ct) =
            build_qr_image("https://opn.onl/x", &opts(Some("nothex"), None, None)).unwrap();
        assert_eq!(ct, "image/png");
        assert!(bytes.starts_with(PNG_MAGIC));
    }

    #[test]
    fn svg_format_returns_svg() {
        let (bytes, ct) =
            build_qr_image("https://opn.onl/abc123", &opts(None, None, Some("svg"))).unwrap();
        assert_eq!(ct, "image/svg+xml");
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("<svg"));
    }

    #[test]
    fn logo_overlay_does_not_panic() {
        let (bytes, ct) =
            build_qr_image("https://opn.onl/abc123", &opts(Some("2f37d8"), Some(true), None)).unwrap();
        assert_eq!(ct, "image/png");
        assert!(image::load_from_memory(&bytes).is_ok());
    }

    #[test]
    fn logo_color_follows_foreground() {
        let (blue, _) =
            build_qr_image("https://opn.onl/abc123", &opts(Some("2f37d8"), Some(true), None)).unwrap();
        let (rose, _) =
            build_qr_image("https://opn.onl/abc123", &opts(Some("e11d48"), Some(true), None)).unwrap();
        assert_ne!(blue, rose, "tinted logo should change with foreground color");
    }

    #[test]
    fn svg_with_logo_has_backplate() {
        let (bytes, _) =
            build_qr_image("https://opn.onl/abc123", &opts(None, Some(true), Some("svg"))).unwrap();
        let s = String::from_utf8(bytes).unwrap();
        // The logo must sit on a circular backplate, not bare on the modules.
        assert!(s.contains("<circle"), "branded SVG should draw a backplate circle");
    }

    #[test]
    fn svg_with_logo_embeds_image() {
        let (bytes, _) =
            build_qr_image("https://opn.onl/abc123", &opts(None, Some(true), Some("svg"))).unwrap();
        let s = String::from_utf8(bytes).unwrap();
        // Logo asset is embedded, so the branded SVG should carry a data URI.
        assert!(s.contains("<image") && s.contains("data:image/png;base64,"));
    }

    #[test]
    fn parse_hex_roundtrip() {
        assert_eq!(parse_hex("#2f37d8"), Some([0x2f, 0x37, 0xd8]));
        assert_eq!(parse_hex("2f37d8"), Some([0x2f, 0x37, 0xd8]));
        assert_eq!(parse_hex("xyz"), None);
        assert_eq!(parse_hex("2f37"), None);
    }
}

#[cfg(test)]
mod api_key_tests {
    use super::hash_api_key;

    #[test]
    fn hash_is_deterministic_distinct_and_opaque() {
        let a = hash_api_key("opn_abc123");
        assert_eq!(a, hash_api_key("opn_abc123"), "same key → same hash");
        assert_ne!(a, hash_api_key("opn_different"), "different keys → different hashes");
        assert_eq!(a.len(), 44, "sha256 base64 is 44 chars");
        assert!(!a.contains("opn_abc123"), "hash must not contain the raw key");
    }
}

#[cfg(test)]
mod ssrf_tests {
    use super::{build_pinned_client, resolve_and_validate, ValidatedTarget};
    use std::net::SocketAddr;

    /// Minimal HTTP/1.1 server that answers every connection with `200 ok`.
    /// Returns the address it is listening on (always 127.0.0.1:<ephemeral>).
    async fn spawn_ok_server() -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = [0u8; 2048];
                    let _ = sock.read(&mut buf).await;
                    let _ = sock
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                        )
                        .await;
                    let _ = sock.flush().await;
                });
            }
        });
        addr
    }

    /// Core DNS-rebinding regression: the client must connect to the address the
    /// guard validated, NOT to whatever DNS says at connect time. The host here
    /// (`pinned.invalid`) has no DNS record at all — reserved `.invalid` never
    /// resolves — so the fetch can only succeed if the pin forces the connection
    /// to the validated address. Before the fix reqwest did its own resolution
    /// and this would be unreachable.
    #[tokio::test]
    async fn pinned_client_connects_to_validated_address_not_dns() {
        let addr = spawn_ok_server().await;
        let target = ValidatedTarget {
            host: "pinned.invalid".to_string(),
            addrs: vec![addr],
            is_literal_ip: false,
        };
        let client = build_pinned_client(&target, None).unwrap();

        let resp = client
            .get(format!("http://pinned.invalid:{}/", addr.port()))
            .send()
            .await
            .expect("pinned connection should reach the validated address");
        assert_eq!(resp.status().as_u16(), 200);
        assert_eq!(resp.text().await.unwrap(), "ok");
    }

    #[tokio::test]
    async fn literal_private_and_metadata_ips_are_refused() {
        for url in [
            "http://127.0.0.1/",
            "http://127.0.0.1:8080/admin",
            "http://169.254.169.254/latest/meta-data/", // cloud metadata
            "http://10.0.0.5/",
            "http://192.168.1.1/",
            "http://100.64.0.1/",  // CGNAT
            "http://[::1]/",       // IPv6 loopback
            "http://[fd00::1]/",   // IPv6 ULA
            "http://0.0.0.0/",
            "http://[::ffff:127.0.0.1]/", // IPv4-mapped loopback
        ] {
            let err = resolve_and_validate(url).await.expect_err(&format!("{url} must be refused"));
            assert!(
                err.contains("disallowed") || err.contains("resolve"),
                "unexpected error for {url}: {err}"
            );
        }
    }

    /// A hostname that only resolves to a private IP is refused at resolve time,
    /// before any connection is attempted. `localhost` → 127.0.0.1 / ::1.
    #[tokio::test]
    async fn hostname_resolving_to_private_ip_is_refused() {
        let err = resolve_and_validate("http://localhost:3000/")
            .await
            .expect_err("localhost must be refused");
        assert!(err.contains("disallowed"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn non_http_scheme_and_missing_host_are_refused() {
        assert!(resolve_and_validate("file:///etc/passwd").await.is_err());
        assert!(resolve_and_validate("gopher://127.0.0.1/").await.is_err());
        assert!(resolve_and_validate("ftp://example.com/").await.is_err());
        assert!(resolve_and_validate("not a url").await.is_err());
    }

    #[tokio::test]
    async fn public_literal_ip_is_allowed_and_marked_literal() {
        let target = resolve_and_validate("http://93.184.216.34/")
            .await
            .expect("public literal IP should pass");
        assert!(target.is_literal_ip);
        assert_eq!(target.addrs, vec!["93.184.216.34:80".parse::<SocketAddr>().unwrap()]);
    }

    /// Positive path against a real external HTTPS host: pinning must not break
    /// certificate validation (Host/SNI stay set to the hostname). Network-
    /// dependent, so ignored by default; run with `--ignored`.
    #[tokio::test]
    #[ignore = "requires network"]
    async fn real_public_https_still_fetches_through_guard() {
        let resp = super::ssrf_guarded_fetch(reqwest::Method::GET, "https://example.com/", None)
            .await
            .expect("public HTTPS fetch should succeed with pinning");
        assert!(resp.status().is_success());
        let body = resp.text().await.unwrap();
        assert!(body.to_lowercase().contains("example domain"));
    }

    /// The avatar proxy is public and top-level-navigable, so what it returns is
    /// rendered in this origin. `canonical_avatar_content_type` is the gate that
    /// keeps active content (SVG, or a spoofed image label on an HTML/JS body)
    /// from being served. Regression guard: it must reject SVG and anything not
    /// on the inert-raster allowlist, and it must return a fixed canonical type
    /// rather than echoing the upstream header. If someone reverts to a
    /// `starts_with("image/")` check, the SVG assertions here fail.
    #[test]
    fn avatar_content_type_allows_only_inert_raster_images() {
        use super::canonical_avatar_content_type as classify;

        // Allowed raster types, including case/charset/whitespace variants, map
        // to a fixed canonical value.
        assert_eq!(classify("image/png"), Some("image/png"));
        assert_eq!(classify("IMAGE/PNG"), Some("image/png"));
        assert_eq!(classify("image/jpeg"), Some("image/jpeg"));
        assert_eq!(classify("image/jpg"), Some("image/jpeg"));
        assert_eq!(classify("image/jpeg; charset=binary"), Some("image/jpeg"));
        assert_eq!(classify("  image/webp  "), Some("image/webp"));
        assert_eq!(classify("image/gif"), Some("image/gif"));
        assert_eq!(classify("image/avif"), Some("image/avif"));
        assert_eq!(classify("image/bmp"), Some("image/bmp"));
        assert_eq!(classify("image/x-icon"), Some("image/x-icon"));
        assert_eq!(classify("image/vnd.microsoft.icon"), Some("image/x-icon"));

        // Active or non-image content must be rejected. SVG is the important one:
        // it is `image/*` yet can execute script.
        for bad in [
            "image/svg+xml",
            "image/svg+xml; charset=utf-8",
            "text/html",
            "application/xhtml+xml",
            "application/javascript",
            "text/xml",
            "image/svg",
            "",
            "image/",
            "img/png",
        ] {
            assert_eq!(classify(bad), None, "{bad:?} must not be served as an avatar");
        }
    }
}

/// A single routing rule as accepted from the API.
#[derive(Deserialize, ToSchema)]
pub struct RoutingRuleInput {
    pub priority: Option<i32>,
    pub match_device: Option<String>,
    pub match_os: Option<String>,
    pub match_country: Option<String>,
    pub match_lang: Option<String>,
    pub destination_url: String,
    pub weight: Option<i32>,
}

#[derive(Deserialize, ToSchema)]
pub struct ReplaceRoutingRulesRequest {
    pub rules: Vec<RoutingRuleInput>,
}

#[derive(Serialize, ToSchema)]
pub struct RoutingRuleResponse {
    pub id: i32,
    pub priority: i32,
    pub match_device: Option<String>,
    pub match_os: Option<String>,
    pub match_country: Option<String>,
    pub match_lang: Option<String>,
    pub destination_url: String,
    pub weight: i32,
}

#[derive(Serialize, ToSchema)]
pub struct RoutingRulesSavedResponse {
    pub count: usize,
}

const MAX_ROUTING_RULES: usize = 20;

/// Return the link if `user_id` owns it directly or via its organization.
async fn link_for_owner(
    db: &DatabaseConnection,
    id: i32,
    user_id: i32,
) -> Option<links::Model> {
    let link = links::Entity::find_by_id(id)
        .filter(links::Column::DeletedAt.is_null())
        .one(db)
        .await
        .ok()
        .flatten()?;
    if link.user_id == Some(user_id) {
        return Some(link);
    }
    if let Some(org_id) = link.org_id {
        use crate::entity::org_members;
        let is_member = org_members::Entity::find()
            .filter(org_members::Column::OrgId.eq(org_id))
            .filter(org_members::Column::UserId.eq(user_id))
            .one(db)
            .await
            .ok()
            .flatten()
            .is_some();
        if is_member {
            return Some(link);
        }
    }
    None
}

/// List the routing rules for a link.
pub async fn get_routing_rules(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
    };
    if link_for_owner(&state.db, id, user_id).await.is_none() {
        return (StatusCode::FORBIDDEN, "You don't have permission to access this link").into_response();
    }
    let rules = crate::entity::routing_rules::Entity::find()
        .filter(crate::entity::routing_rules::Column::LinkId.eq(id))
        .order_by_asc(crate::entity::routing_rules::Column::Priority)
        .all(&state.db)
        .await
        .unwrap_or_default();
    let out: Vec<RoutingRuleResponse> = rules
        .into_iter()
        .map(|r| RoutingRuleResponse {
            id: r.id,
            priority: r.priority,
            match_device: r.match_device,
            match_os: r.match_os,
            match_country: r.match_country,
            match_lang: r.match_lang,
            destination_url: r.destination_url,
            weight: r.weight,
        })
        .collect();
    (StatusCode::OK, Json(out)).into_response()
}

/// Replace all routing rules for a link (delete-then-insert in a transaction).
pub async fn replace_routing_rules(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
    Json(payload): Json<ReplaceRoutingRulesRequest>,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
    };
    let link = match link_for_owner(&state.db, id, user_id).await {
        Some(l) => l,
        None => {
            return (StatusCode::FORBIDDEN, "You don't have permission to modify this link").into_response()
        }
    };

    // For an org-owned link, a member who is not the direct owner may rewrite
    // routing destinations only if their role grants edit rights. Viewers can
    // read the rules (get_routing_rules) but must not mutate them — mirrors the
    // member_can_edit gate used by folders.rs / tags.rs.
    if let Some(org_id) = link.org_id {
        if link.user_id != Some(user_id)
            && !crate::handlers::organizations::member_can_edit(&state.db, org_id, user_id).await
        {
            return (StatusCode::FORBIDDEN, "You don't have permission to modify this link").into_response();
        }
    }

    if payload.rules.len() > MAX_ROUTING_RULES {
        return (
            StatusCode::BAD_REQUEST,
            format!("A link can have at most {} routing rules", MAX_ROUTING_RULES),
        )
            .into_response();
    }

    // Validate every destination (format + blocklist) before persisting anything.
    let mut validated: Vec<(String, &RoutingRuleInput)> = Vec::with_capacity(payload.rules.len());
    for rule in &payload.rules {
        let url = match validate_url(&rule.destination_url) {
            Ok(u) => u,
            Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
        };
        if check_blocked(&state.db, &url).await.is_err() {
            return (StatusCode::BAD_REQUEST, "A destination URL is blocked".to_string()).into_response();
        }
        validated.push((url, rule));
    }

    let txn = match state.db.begin().await {
        Ok(t) => t,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    };
    if crate::entity::routing_rules::Entity::delete_many()
        .filter(crate::entity::routing_rules::Column::LinkId.eq(id))
        .exec(&txn)
        .await
        .is_err()
    {
        let _ = txn.rollback().await;
        return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
    }
    for (url, rule) in &validated {
        let am = crate::entity::routing_rules::ActiveModel {
            link_id: Set(id),
            priority: Set(rule.priority.unwrap_or(0)),
            match_device: Set(rule.match_device.clone().filter(|s| !s.is_empty())),
            match_os: Set(rule.match_os.clone().filter(|s| !s.is_empty())),
            match_country: Set(rule.match_country.clone().filter(|s| !s.is_empty())),
            match_lang: Set(rule.match_lang.clone().filter(|s| !s.is_empty())),
            destination_url: Set(url.clone()),
            weight: Set(rule.weight.unwrap_or(1).max(1)),
            ..Default::default()
        };
        if am.insert(&txn).await.is_err() {
            let _ = txn.rollback().await;
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    }
    if txn.commit().await.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
    }

    // Invalidate the cache so the link leaves the fast path and picks up its rules.
    if let Some(cache) = &state.redis_cache {
        let _ = cache.invalidate_link(&link.code).await;
    }

    (StatusCode::OK, Json(RoutingRulesSavedResponse { count: validated.len() })).into_response()
}

/// Get user's links with filtering
#[utoipa::path(
    get,
    path = "/links",
    params(LinksQuery),
    responses(
        (status = 200, description = "List of links", body = Vec<LinkResponse>),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Links"
)]
pub async fn get_user_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LinksQuery>,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "Unauthorized".to_string() })).into_response(),
    };

    let mut link_query = links::Entity::find()
        .filter(links::Column::UserId.eq(user_id))
        .filter(links::Column::DeletedAt.is_null());

    // Filter by folder
    if let Some(folder_id) = query.folder_id {
        link_query = link_query.filter(links::Column::FolderId.eq(folder_id));
    }

    // Filter by organization
    if let Some(org_id) = query.org_id {
        link_query = link_query.filter(links::Column::OrgId.eq(org_id));
    }

    // Search by URL or code
    if let Some(search) = query.search {
        link_query = link_query.filter(
            Condition::any()
                .add(links::Column::OriginalUrl.contains(&search))
                .add(links::Column::Code.contains(&search))
                .add(links::Column::Notes.contains(&search))
        );
    }

    // Filter by tag
    if let Some(tag_id) = query.tag_id {
        let link_tag_ids: Vec<i32> = link_tags::Entity::find()
            .filter(link_tags::Column::TagId.eq(tag_id))
            .all(&state.db)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|lt| lt.link_id)
            .collect();

        link_query = link_query.filter(links::Column::Id.is_in(link_tag_ids));
    }

    let link_query = link_query.order_by_desc(links::Column::CreatedAt);

    // Pagination
    let link_query = if let Some(limit) = query.limit {
        link_query.limit(limit)
    } else {
        link_query
    };

    let link_query = if let Some(offset) = query.offset {
        link_query.offset(offset)
    } else {
        link_query
    };

    let user_links = link_query.all(&state.db).await.unwrap_or_default();

    let base_url = get_base_url();
    let api_url = get_api_url();
    let mut response = Vec::new();
    for l in user_links {
        let tags = get_link_tags(&state.db, l.id).await;
        response.push(LinkResponse {
            id: l.id,
            code: l.code.clone(),
            short_url: format!("{}/{}", base_url, l.code),
            api_url: format!("{}/{}", api_url, l.code),
            original_url: l.original_url.clone(),
            title: l.title.clone(),
            click_count: l.click_count,
            created_at: l.created_at.to_string(),
            expires_at: l.expires_at.map(|d| d.to_string()),
            has_password: l.password_hash.is_some(),
            notes: l.notes.clone(),
            folder_id: l.folder_id,
            org_id: l.org_id,
            starts_at: l.starts_at.map(|s| s.to_string()),
            max_clicks: l.max_clicks,
            burn_after_reading: l.burn_after_reading,
            burned_at: l.burned_at.map(|d| d.to_string()),
            safe_link_interstitial: l.safe_link_interstitial,
            bio_visible: l.bio_visible,
            is_active: l.is_active(),
            is_pinned: l.is_pinned,
            tags,
        });
    }

    (StatusCode::OK, Json(response)).into_response()
}

/// Delete a link
#[utoipa::path(
    delete,
    path = "/links/{id}",
    params(
        ("id" = i32, Path, description = "Link ID")
    ),
    responses(
        (status = 200, description = "Link deleted"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found"),
    ),
    tag = "Links"
)]
pub async fn delete_link(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "Unauthorized".to_string() })).into_response(),
    };

    let link = links::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(link) = link {
        if link.user_id != Some(user_id) {
            return (StatusCode::FORBIDDEN, Json(ErrorResponse { error: "You don't have permission to delete this link".to_string() })).into_response();
        }

        if link.deleted_at.is_some() {
            return (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Link not found".to_string() })).into_response();
        }

        // Soft delete - set deleted_at timestamp instead of actually deleting
        let mut active_link: links::ActiveModel = link.clone().into();
        active_link.deleted_at = Set(Some(chrono::Utc::now().naive_utc()));

        // Invalidate cache
        if let Some(cache) = &state.redis_cache {
            let _ = cache.invalidate_link(&link.code).await;
        }

        match active_link.update(&state.db).await {
            Ok(_) => (StatusCode::OK, Json(SuccessResponse { message: "Link deleted successfully".to_string() })).into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Failed to delete link".to_string() })).into_response(),
        }
    } else {
        (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Link not found".to_string() })).into_response()
    }
}

/// Update a link
#[utoipa::path(
    put,
    path = "/links/{id}",
    params(
        ("id" = i32, Path, description = "Link ID")
    ),
    request_body = UpdateLinkRequest,
    responses(
        (status = 200, description = "Link updated", body = LinkResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found"),
    ),
    tag = "Links"
)]
pub async fn update_link(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
    Json(payload): Json<UpdateLinkRequest>,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "Unauthorized".to_string() })).into_response(),
    };

    let link = links::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(link) = link {
        if link.user_id != Some(user_id) {
            return (StatusCode::FORBIDDEN, Json(ErrorResponse { error: "You don't have permission to update this link".to_string() })).into_response();
        }

        let mut active_link: links::ActiveModel = link.clone().into();

        // Validate scheduling / limit inputs the same way create_link does, so an
        // update can't leave a link in an invalid state (e.g. max_clicks <= 0
        // bricks the link; starts_at >= expires_at makes it never active).
        if payload.remove_max_clicks != Some(true) {
            if let Some(mc) = payload.max_clicks {
                if mc <= 0 {
                    return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "max_clicks must be greater than 0".to_string() })).into_response();
                }
            }
        }
        let eff_starts: Option<DateTime<Utc>> = if payload.remove_starts_at == Some(true) {
            None
        } else {
            payload.starts_at.or_else(|| link.starts_at.map(|d| d.and_utc()))
        };
        let eff_expires: Option<DateTime<Utc>> = if payload.remove_expiration == Some(true) {
            None
        } else {
            payload.expires_at.or_else(|| link.expires_at.map(|d| d.and_utc()))
        };
        if let (Some(s), Some(e)) = (eff_starts, eff_expires) {
            if s >= e {
                return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "starts_at must be before expires_at".to_string() })).into_response();
            }
        }

        if let Some(ref url) = payload.original_url {
            // Validate URL format and sanitize
            let validated_url = match validate_url(url) {
                Ok(u) => u,
                Err(e) => return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })).into_response(),
            };
            // Check if new URL is blocked
            if let Err(e) = check_blocked(&state.db, &validated_url).await {
                return (StatusCode::FORBIDDEN, Json(ErrorResponse { error: e })).into_response();
            }
            active_link.original_url = Set(validated_url);
        }

        if payload.remove_expiration == Some(true) {
            active_link.expires_at = Set(None);
        } else if let Some(expires) = payload.expires_at {
            active_link.expires_at = Set(Some(expires.naive_utc()));
        }

        if payload.remove_password == Some(true) {
            active_link.password_hash = Set(None);
        } else if let Some(password) = payload.password {
            match hash(password, DEFAULT_COST) {
                Ok(h) => active_link.password_hash = Set(Some(h)),
                Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Failed to hash password".to_string() })).into_response(),
            }
        }

        if let Some(title) = payload.title {
            active_link.title = Set(Some(title));
        }

        if let Some(notes) = payload.notes {
            active_link.notes = Set(Some(notes));
        }

        if let Some(folder_id) = payload.folder_id {
            if !user_can_use_folder(&state.db, folder_id, user_id).await {
                return (StatusCode::FORBIDDEN, Json(ErrorResponse { error: "Folder not found or access denied".to_string() })).into_response();
            }
            active_link.folder_id = Set(Some(folder_id));
        }

        if payload.remove_starts_at == Some(true) {
            active_link.starts_at = Set(None);
        } else if let Some(starts_at) = payload.starts_at {
            active_link.starts_at = Set(Some(starts_at.naive_utc()));
        }

        if payload.remove_max_clicks == Some(true) {
            active_link.max_clicks = Set(None);
        } else if let Some(max_clicks) = payload.max_clicks {
            active_link.max_clicks = Set(Some(max_clicks));
        }

        // Burn-after-reading (gated by ENABLE_BURN_AFTER_READING).
        let burn_enabled = std::env::var("ENABLE_BURN_AFTER_READING")
            .map(|v| v != "false")
            .unwrap_or(true);
        if burn_enabled {
            if let Some(burn) = payload.burn_after_reading {
                active_link.burn_after_reading = Set(burn);
                if burn {
                    // Ensure a burn link has a click cap (default one-time use),
                    // accounting for any cap change in this same request.
                    let has_cap = if payload.remove_max_clicks == Some(true) {
                        false
                    } else if let Some(mc) = payload.max_clicks {
                        mc > 0
                    } else {
                        link.max_clicks.is_some()
                    };
                    if !has_cap {
                        active_link.max_clicks = Set(Some(1));
                    }
                }
            }
        }

        // Safe-link interstitial (gated by ENABLE_SAFE_LINK_INTERSTITIAL).
        let interstitial_enabled = std::env::var("ENABLE_SAFE_LINK_INTERSTITIAL")
            .map(|v| v != "false")
            .unwrap_or(true);
        if interstitial_enabled {
            if let Some(interstitial) = payload.safe_link_interstitial {
                active_link.safe_link_interstitial = Set(interstitial);
            }
        }

        // Link-in-bio visibility (gated by ENABLE_LINK_IN_BIO).
        let link_in_bio_enabled = std::env::var("ENABLE_LINK_IN_BIO")
            .map(|v| v != "false")
            .unwrap_or(true);
        if link_in_bio_enabled {
            if let Some(visible) = payload.bio_visible {
                active_link.bio_visible = Set(visible);
            }
        }

        match active_link.update(&state.db).await {
            Ok(updated) => {
                // Invalidate cache
                if let Some(cache) = &state.redis_cache {
                    let _ = cache.invalidate_link(&updated.code).await;
                }

                let tags = get_link_tags(&state.db, updated.id).await;
                let base_url = get_base_url();
                let api_url = get_api_url();
                (StatusCode::OK, Json(LinkResponse {
                    id: updated.id,
                    code: updated.code.clone(),
                    short_url: format!("{}/{}", base_url, updated.code),
                    api_url: format!("{}/{}", api_url, updated.code),
                    original_url: updated.original_url.clone(),
                    title: updated.title.clone(),
                    click_count: updated.click_count,
                    created_at: updated.created_at.to_string(),
                    expires_at: updated.expires_at.map(|d| d.to_string()),
                    has_password: updated.password_hash.is_some(),
                    notes: updated.notes.clone(),
                    folder_id: updated.folder_id,
                    org_id: updated.org_id,
                    starts_at: updated.starts_at.map(|s| s.to_string()),
                    max_clicks: updated.max_clicks,
                    burn_after_reading: updated.burn_after_reading,
                    burned_at: updated.burned_at.map(|d| d.to_string()),
                    safe_link_interstitial: updated.safe_link_interstitial,
                    bio_visible: updated.bio_visible,
                    is_active: updated.is_active(),
                    is_pinned: updated.is_pinned,
                    tags,
                })).into_response()
            }
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Failed to update link".to_string() })).into_response(),
        }
    } else {
        (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Link not found".to_string() })).into_response()
    }
}

/// Bulk create links
#[utoipa::path(
    post,
    path = "/links/bulk",
    request_body = BulkCreateLinkRequest,
    responses(
        (status = 200, description = "Links created", body = BulkCreateLinkResponse),
    ),
    tag = "Links"
)]
pub async fn bulk_create_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<BulkCreateLinkRequest>,
) -> impl IntoResponse {
    let user_id = get_user_id_from_header(&state.db, &headers).await;

    // Bulk create is authenticated-only. Anonymous single-create is a feature,
    // but a 500-URLs-per-request batch reachable without an account is a
    // rate-limit amplification vector (one create token buys hundreds of links).
    if user_id.is_none() {
        return (StatusCode::UNAUTHORIZED, Json(BulkCreateLinkResponse {
            links: vec![],
            errors: vec!["Authentication required for bulk link creation".to_string()],
        })).into_response();
    }

    // Cap batch size to avoid unbounded per-item work / DoS.
    if payload.urls.len() > 500 {
        return (StatusCode::BAD_REQUEST, Json(BulkCreateLinkResponse {
            links: vec![],
            errors: vec!["Too many URLs in one request (max 500)".to_string()],
        })).into_response();
    }

    // Check email verification for authenticated users
    if let Some(uid) = user_id {
        let user = users::Entity::find_by_id(uid)
            .one(&state.db)
            .await
            .ok()
            .flatten();

        if let Some(u) = user {
            if !u.email_verified {
                return (StatusCode::FORBIDDEN, Json(BulkCreateLinkResponse {
                    links: vec![], 
                    errors: vec!["Please verify your email address before creating links".to_string()] 
                })).into_response();
            }
        }
    }

    // If org_id is provided, verify user is a member
    if let Some(org_id) = payload.org_id {
        if let Some(uid) = user_id {
            use crate::entity::org_members;
            let is_member = org_members::Entity::find()
                .filter(org_members::Column::OrgId.eq(org_id))
                .filter(org_members::Column::UserId.eq(uid))
                .one(&state.db)
                .await
                .ok()
                .flatten()
                .is_some();
            
            if !is_member {
                return (StatusCode::FORBIDDEN, Json(BulkCreateLinkResponse { 
                    links: vec![], 
                    errors: vec!["Not a member of this organization".to_string()] 
                })).into_response();
            }
        } else {
            return (StatusCode::FORBIDDEN, Json(BulkCreateLinkResponse { 
                links: vec![], 
                errors: vec!["Authentication required to create org links".to_string()] 
            })).into_response();
        }
    }

    let mut result_links = Vec::new();
    let mut errors = Vec::new();
    let base_url = get_base_url();
    // Per-link rate key: charged once per URL below so a bulk request cannot
    // create more links than the single-create budget allows.
    let ip = crate::utils::rate_limiter::client_ip_from_headers(&headers)
        .unwrap_or_else(|| "unknown".to_string());

    // Per-user link cap (MAX_LINKS_PER_USER), enforced across the whole batch so
    // bulk create can't be used to exceed the limit. `None` = unlimited /
    // anonymous. Tracks the remaining budget as links are created.
    let mut remaining_budget: Option<u64> = None;
    if let (Some(uid), Some(cap)) = (user_id, get_max_links_per_user()) {
        let existing = links::Entity::find()
            .filter(links::Column::UserId.eq(uid))
            .filter(links::Column::DeletedAt.is_null())
            .count(&state.db)
            .await
            .unwrap_or(0);
        remaining_budget = Some(cap.saturating_sub(existing));
    }

    for url in payload.urls {
        // Charge the per-IP create budget per link. A bulk request is not a
        // discount: once the hourly create budget is spent, the remaining URLs
        // are reported as rate-limited instead of silently amplifying past it.
        if let crate::utils::rate_limiter::RateLimitResult::Limited { retry_after_secs, .. } =
            state.rate_limiters.link_creation.check(&format!("create:{}", ip))
        {
            errors.push(format!("{}: rate limit reached, try again in {}s", url, retry_after_secs));
            continue;
        }

        // Validate URL before creating link. Surface the specific reason
        // (bad format, dangerous file type, raw IP, …) rather than a generic
        // message, so a bulk upload tells the user which links were rejected why.
        if let Err(e) = validate_url(&url) {
            errors.push(format!("{}: {}", url, e));
            continue;
        }

        // Check if URL or domain is blocked
        if let Err(e) = check_blocked(&state.db, &url).await {
            errors.push(format!("{}: {}", url, e));
            continue;
        }

        // Stop creating once the per-user cap is reached.
        if let Some(0) = remaining_budget {
            errors.push(format!("{}: account link limit reached", url));
            continue;
        }

        let code: String = thread_rng()
            .sample_iter(&Alphanumeric)
            .take(6)
            .map(char::from)
            .collect();

        let link = links::ActiveModel {
            original_url: Set(url.clone()),
            code: Set(code.clone()),
            user_id: Set(user_id),
            folder_id: Set(payload.folder_id),
            org_id: Set(payload.org_id),
            ..Default::default()
        };

        match links::Entity::insert(link).exec(&state.db).await {
            Ok(link_res) => {
                result_links.push(CreateLinkResponse {
                    id: link_res.last_insert_id,
                    code: code.clone(),
                    short_url: format!("{}/{}", base_url, code),
                });
                if let Some(b) = remaining_budget.as_mut() {
                    *b = b.saturating_sub(1);
                }
            }
            Err(e) => {
                errors.push(format!("Failed to shorten {}: {}", url, e));
            }
        }
    }

    (StatusCode::OK, Json(BulkCreateLinkResponse { links: result_links, errors })).into_response()
}

/// Bulk delete links
#[utoipa::path(
    post,
    path = "/links/bulk/delete",
    request_body = BulkDeleteRequest,
    responses(
        (status = 200, description = "Links deleted", body = BulkDeleteResponse),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Links"
)]
pub async fn bulk_delete_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<BulkDeleteRequest>,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "Unauthorized".to_string() })).into_response(),
    };

    if payload.ids.len() > 500 {
        return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Too many items in one request (max 500)".to_string() })).into_response();
    }

    let mut deleted = 0u64;
    let mut invalidated: Vec<String> = Vec::new();

    for id in payload.ids {
        let link = links::Entity::find_by_id(id)
            .one(&state.db)
            .await
            .ok()
            .flatten();

        if let Some(link) = link {
            if link.user_id == Some(user_id) && link.deleted_at.is_none() {
                // Soft delete instead of hard delete
                let code = link.code.clone();
                let mut active_link: links::ActiveModel = link.into();
                active_link.deleted_at = Set(Some(chrono::Utc::now().naive_utc()));

                if active_link.update(&state.db).await.is_ok() {
                    deleted += 1;
                    invalidated.push(code);
                }
            }
        }
    }

    // Drop cached redirects for the deleted codes so they stop resolving now,
    // not after the cache TTL.
    invalidate_cached_codes(&state, &invalidated).await;

    (StatusCode::OK, Json(BulkDeleteResponse { deleted })).into_response()
}

/// Bulk update links
#[utoipa::path(
    post,
    path = "/links/bulk/update",
    request_body = BulkUpdateRequest,
    responses(
        (status = 200, description = "Links updated", body = BulkUpdateResponse),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Links"
)]
pub async fn bulk_update_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<BulkUpdateRequest>,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "Unauthorized".to_string() })).into_response(),
    };

    if payload.ids.len() > 500 {
        return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Too many items in one request (max 500)".to_string() })).into_response();
    }

    let mut updated = 0u64;
    let mut invalidated: Vec<String> = Vec::new();

    for id in payload.ids {
        let link = links::Entity::find_by_id(id)
            .one(&state.db)
            .await
            .ok()
            .flatten();

        if let Some(link) = link {
            if link.user_id == Some(user_id) {
                let code = link.code.clone();
                let mut active_link: links::ActiveModel = link.into();

                if let Some(folder_id) = payload.folder_id {
                    // Skip the folder move for items whose target folder the user can't use.
                    if user_can_use_folder(&state.db, folder_id, user_id).await {
                        active_link.folder_id = Set(Some(folder_id));
                    }
                }

                if payload.remove_expiration == Some(true) {
                    active_link.expires_at = Set(None);
                } else if let Some(expires) = payload.expires_at {
                    active_link.expires_at = Set(Some(expires.naive_utc()));
                }

                if active_link.update(&state.db).await.is_ok() {
                    updated += 1;
                    invalidated.push(code);
                }
            }
        }
    }

    // An expiry/state change must drop the cached redirect, or the old target
    // keeps serving until the cache TTL.
    invalidate_cached_codes(&state, &invalidated).await;

    (StatusCode::OK, Json(BulkUpdateResponse { updated })).into_response()
}

/// Export links to CSV
#[utoipa::path(
    get,
    path = "/links/export",
    responses(
        (status = 200, description = "CSV file", content_type = "text/csv"),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Links"
)]
pub async fn export_links_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
    };

    let user_links = links::Entity::find()
        .filter(links::Column::UserId.eq(user_id))
        .filter(links::Column::DeletedAt.is_null())
        .order_by_desc(links::Column::CreatedAt)
        .all(&state.db)
        .await
        .unwrap_or_default();

    let base_url = get_base_url();

    // Escape a value for safe CSV output: neutralize spreadsheet formula
    // injection (leading = + - @) and always quote, doubling inner quotes.
    fn csv_field(value: &str) -> String {
        let mut escaped = value.replace('"', "\"\"");
        if value.starts_with(['=', '+', '-', '@']) {
            escaped.insert(0, '\'');
        }
        format!("\"{}\"", escaped)
    }

    let mut csv_content = String::from("ID,Code,Original URL,Short URL,Click Count,Created At,Expires At,Has Password,Notes,Folder ID,Max Clicks,Starts At\n");

    for link in user_links {
        csv_content.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{}\n",
            link.id,
            csv_field(&link.code),
            csv_field(&link.original_url),
            csv_field(&format!("{}/{}", base_url, link.code)),
            link.click_count,
            csv_field(&link.created_at.format("%Y-%m-%d %H:%M:%S").to_string()),
            csv_field(&link.expires_at.map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string()).unwrap_or_default()),
            link.password_hash.is_some(),
            csv_field(&link.notes.clone().unwrap_or_default()),
            link.folder_id.map(|f| f.to_string()).unwrap_or_default(),
            link.max_clicks.map(|m| m.to_string()).unwrap_or_default(),
            csv_field(&link.starts_at.map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string()).unwrap_or_default()),
        ));
    }

    (
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "text/csv"),
            (axum::http::header::CONTENT_DISPOSITION, "attachment; filename=\"links.csv\""),
        ],
        csv_content,
   ).into_response()
}

// ============= New Feature: Clone Link =============

#[derive(Serialize, ToSchema)]
pub struct CloneLinkResponse {
    pub id: i32,
    pub code: String,
    pub short_url: String,
    pub original_url: String,
    pub message: String,
}

/// Clone an existing link with a new short code
#[utoipa::path(
    post,
    path = "/links/{id}/clone",
    params(
        ("id" = i32, Path, description = "Link ID to clone")
    ),
    responses(
        (status = 201, description = "Link cloned", body = CloneLinkResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Link not found"),
    ),
    tag = "Links"
)]
pub async fn clone_link(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "Unauthorized".to_string() })).into_response(),
    };

    let link = links::Entity::find_by_id(id)
        .filter(links::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(link) = link {
        // Verify ownership
        if link.user_id != Some(user_id) {
            return (StatusCode::FORBIDDEN, Json(ErrorResponse { error: "You don't have permission to clone this link".to_string() })).into_response();
        }

        // Generate new short code
        let mut code = generate_short_code();
        while links::Entity::find().filter(links::Column::Code.eq(&code)).one(&state.db).await.unwrap_or(None).is_some() {
            code = generate_short_code();
        }

        // Create new link with same settings but new code
        let new_link = links::ActiveModel {
            original_url: Set(link.original_url.clone()),
            code: Set(code.clone()),
            user_id: Set(Some(user_id)),
            expires_at: Set(link.expires_at),
            password_hash: Set(link.password_hash.clone()),
            title: Set(link.title.clone().map(|t| format!("{} (copy)", t))),
            notes: Set(link.notes.clone()),
            folder_id: Set(link.folder_id),
            org_id: Set(link.org_id),
            starts_at: Set(link.starts_at),
            max_clicks: Set(link.max_clicks),
            is_pinned: Set(false), // Don't copy pin status
            ..Default::default()
        };

        match links::Entity::insert(new_link).exec(&state.db).await {
            Ok(res) => {
                // Copy tags
                let link_tags_list = link_tags::Entity::find()
                    .filter(link_tags::Column::LinkId.eq(id))
                    .all(&state.db)
                    .await
                    .unwrap_or_default();

                for lt in link_tags_list {
                    let new_lt = link_tags::ActiveModel {
                        link_id: Set(res.last_insert_id),
                        tag_id: Set(lt.tag_id),
                        ..Default::default()
                    };
                    let _ = new_lt.insert(&state.db).await;
                }

                let base_url = get_base_url();
                (StatusCode::CREATED, Json(CloneLinkResponse {
                    id: res.last_insert_id,
                    code: code.clone(),
                    short_url: format!("{}/{}", base_url, code),
                    original_url: link.original_url,
                    message: "Link cloned successfully".to_string(),
                })).into_response()
            }
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Failed to clone link".to_string() })).into_response(),
        }
    } else {
        (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Link not found".to_string() })).into_response()
    }
}

// ============= New Feature: Toggle Pin =============

#[derive(Serialize, ToSchema)]
pub struct PinResponse {
    pub is_pinned: bool,
    pub message: String,
}

/// Toggle pin status for a link
#[utoipa::path(
    post,
    path = "/links/{id}/pin",
    params(
        ("id" = i32, Path, description = "Link ID")
    ),
    responses(
        (status = 200, description = "Pin status toggled", body = PinResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Link not found"),
    ),
    tag = "Links"
)]
pub async fn toggle_pin(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "Unauthorized".to_string() })).into_response(),
    };

    let link = links::Entity::find_by_id(id)
        .filter(links::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(link) = link {
        if link.user_id != Some(user_id) {
            return (StatusCode::FORBIDDEN, Json(ErrorResponse { error: "You don't have permission to pin this link".to_string() })).into_response();
        }

        let new_pin_status = !link.is_pinned;
        let mut active_link: links::ActiveModel = link.into();
        active_link.is_pinned = Set(new_pin_status);

        match active_link.update(&state.db).await {
            Ok(_) => (StatusCode::OK, Json(PinResponse {
                is_pinned: new_pin_status,
                message: if new_pin_status { "Link pinned".to_string() } else { "Link unpinned".to_string() },
            })).into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Failed to update pin status".to_string() })).into_response(),
        }
    } else {
        (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Link not found".to_string() })).into_response()
    }
}

// ============= New Feature: Check Code Availability =============

#[derive(Deserialize, ToSchema, utoipa::IntoParams)]
pub struct CheckCodeQuery {
    pub code: String,
}

#[derive(Serialize, ToSchema)]
pub struct CheckCodeResponse {
    pub available: bool,
    pub code: String,
    pub message: String,
}

/// Check if a custom alias/code is available
#[utoipa::path(
    get,
    path = "/links/check-code",
    params(CheckCodeQuery),
    responses(
        (status = 200, description = "Code availability checked", body = CheckCodeResponse),
    ),
    tag = "Links"
)]
pub async fn check_code_availability(
    State(state): State<AppState>,
    Query(query): Query<CheckCodeQuery>,
) -> impl IntoResponse {
    let code = query.code.trim();
    
    // Validate alias format
    if let Err(e) = validate_alias(code) {
        return (StatusCode::OK, Json(CheckCodeResponse {
            available: false,
            code: code.to_string(),
            message: e,
        })).into_response();
    }

    // Consider deleted links too: their code is still held by the global UNIQUE
    // constraint on links.code, so it is not actually available for reuse.
    let exists = links::Entity::find()
        .filter(links::Column::Code.eq(code))
        .one(&state.db)
        .await
        .unwrap_or(None)
        .is_some();

    if exists {
        (StatusCode::OK, Json(CheckCodeResponse {
            available: false,
            code: code.to_string(),
            message: "This alias is already taken".to_string(),
        })).into_response()
    } else {
        (StatusCode::OK, Json(CheckCodeResponse {
            available: true,
            code: code.to_string(),
            message: "This alias is available".to_string(),
        })).into_response()
    }
}

// ============= New Feature: URL Health Check =============

#[derive(Deserialize, ToSchema)]
pub struct HealthCheckRequest {
    pub url: String,
}

#[derive(Serialize, ToSchema)]
pub struct UrlHealthResponse {
    pub url: String,
    pub reachable: bool,
    pub status_code: Option<u16>,
    pub response_time_ms: Option<u64>,
    pub error: Option<String>,
}

/// Check if a URL is reachable
#[utoipa::path(
    post,
    path = "/links/health-check",
    request_body = HealthCheckRequest,
    responses(
        (status = 200, description = "URL health checked", body = UrlHealthResponse),
        (status = 400, description = "Invalid URL"),
    ),
    tag = "Links"
)]
pub async fn check_url_health(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<HealthCheckRequest>,
) -> impl IntoResponse {
    // Require authentication: this performs a server-side fetch of a user-supplied URL.
    if get_user_id_from_header(&state.db, &headers).await.is_none() {
        return (StatusCode::UNAUTHORIZED, Json(UrlHealthResponse {
            url: payload.url,
            reachable: false,
            status_code: None,
            response_time_ms: None,
            error: Some("Unauthorized".to_string()),
        })).into_response();
    }

    // Validate URL first
    if validate_url(&payload.url).is_err() {
        return (StatusCode::BAD_REQUEST, Json(UrlHealthResponse {
            url: payload.url,
            reachable: false,
            status_code: None,
            response_time_ms: None,
            error: Some("Invalid URL format".to_string()),
        })).into_response();
    }

    let start = std::time::Instant::now();

    // SSRF-guarded HEAD: the host and every redirect hop are validated against
    // private/internal address ranges before any request is sent.
    match ssrf_guarded_fetch(reqwest::Method::HEAD, &payload.url, None).await {
        Ok(response) => {
            let elapsed = start.elapsed().as_millis() as u64;
            let status = response.status().as_u16();
            
            (StatusCode::OK, Json(UrlHealthResponse {
                url: payload.url,
                reachable: response.status().is_success() || response.status().is_redirection(),
                status_code: Some(status),
                response_time_ms: Some(elapsed),
                error: if response.status().is_client_error() || response.status().is_server_error() {
                    Some(format!("HTTP {}", status))
                } else {
                    None
                },
            })).into_response()
        }
        Err(e) => {
            let elapsed = start.elapsed().as_millis() as u64;
            (StatusCode::OK, Json(UrlHealthResponse {
                url: payload.url,
                reachable: false,
                status_code: None,
                response_time_ms: Some(elapsed),
                error: Some(e.to_string()),
            })).into_response()
        }
    }
}

// ============= New Feature: Build UTM URL =============

#[derive(Deserialize, ToSchema)]
pub struct BuildUtmRequest {
    pub url: String,
    pub utm_source: Option<String>,
    pub utm_medium: Option<String>,
    pub utm_campaign: Option<String>,
    pub utm_term: Option<String>,
    pub utm_content: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct BuildUtmResponse {
    pub original_url: String,
    pub url_with_utm: String,
    pub utm_params: std::collections::HashMap<String, String>,
}

/// Build URL with UTM parameters
#[utoipa::path(
    post,
    path = "/links/build-utm",
    request_body = BuildUtmRequest,
    responses(
        (status = 200, description = "UTM URL built", body = BuildUtmResponse),
        (status = 400, description = "Invalid URL"),
    ),
    tag = "Links"
)]
pub async fn build_utm_url(
    Json(payload): Json<BuildUtmRequest>,
) -> impl IntoResponse {
    // Parse the original URL
    let mut parsed = match url::Url::parse(&payload.url) {
        Ok(u) => u,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Invalid URL format".to_string() })).into_response();
        }
    };

    let mut utm_params = std::collections::HashMap::new();

    // Add UTM parameters
    {
        let mut query_pairs = parsed.query_pairs_mut();
        
        if let Some(ref source) = payload.utm_source {
            if !source.is_empty() {
                query_pairs.append_pair("utm_source", source);
                utm_params.insert("utm_source".to_string(), source.clone());
            }
        }
        if let Some(ref medium) = payload.utm_medium {
            if !medium.is_empty() {
                query_pairs.append_pair("utm_medium", medium);
                utm_params.insert("utm_medium".to_string(), medium.clone());
            }
        }
        if let Some(ref campaign) = payload.utm_campaign {
            if !campaign.is_empty() {
                query_pairs.append_pair("utm_campaign", campaign);
                utm_params.insert("utm_campaign".to_string(), campaign.clone());
            }
        }
        if let Some(ref term) = payload.utm_term {
            if !term.is_empty() {
                query_pairs.append_pair("utm_term", term);
                utm_params.insert("utm_term".to_string(), term.clone());
            }
        }
        if let Some(ref content) = payload.utm_content {
            if !content.is_empty() {
                query_pairs.append_pair("utm_content", content);
                utm_params.insert("utm_content".to_string(), content.clone());
            }
        }
    }

    (StatusCode::OK, Json(BuildUtmResponse {
        original_url: payload.url,
        url_with_utm: parsed.to_string(),
        utm_params,
    })).into_response()
}

// ============= New Feature: Sparkline Data =============

#[derive(Serialize, ToSchema)]
pub struct SparklineData {
    pub link_id: i32,
    pub data: Vec<i64>,  // Click counts for each day
    pub labels: Vec<String>,  // Date labels
    pub total: i64,
}

#[derive(Serialize, ToSchema)]
pub struct SparklineResponse {
    pub sparklines: Vec<SparklineData>,
}

/// Get sparkline data (last 7 days) for multiple links
#[utoipa::path(
    get,
    path = "/links/sparklines",
    params(
        ("ids" = String, Query, description = "Comma-separated link IDs")
    ),
    responses(
        (status = 200, description = "Sparkline data", body = SparklineResponse),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Links"
)]
pub async fn get_sparklines(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Unauthorized"}))).into_response(),
    };

    // Parse link IDs
    let ids_str = params.get("ids").cloned().unwrap_or_default();
    let link_ids: Vec<i32> = ids_str
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    if link_ids.is_empty() {
        return (StatusCode::OK, Json(SparklineResponse { sparklines: vec![] })).into_response();
    }

    // Verify user owns these links
    let user_links = links::Entity::find()
        .filter(links::Column::Id.is_in(link_ids.clone()))
        .filter(links::Column::UserId.eq(user_id))
        .filter(links::Column::DeletedAt.is_null())
        .all(&state.db)
        .await
        .unwrap_or_default();

    let owned_ids: Vec<i32> = user_links.iter().map(|l| l.id).collect();

    // Get last 7 days
    let now = chrono::Utc::now().naive_utc();
    let seven_days_ago = now - chrono::Duration::days(7);

    // Generate date labels
    let mut labels: Vec<String> = Vec::new();
    for i in (0..7).rev() {
        let date = now - chrono::Duration::days(i);
        labels.push(date.format("%m/%d").to_string());
    }

    // Fetch click events
    let events = click_events::Entity::find()
        .filter(click_events::Column::LinkId.is_in(owned_ids.clone()))
        .filter(click_events::Column::CreatedAt.gte(seven_days_ago))
        .all(&state.db)
        .await
        .unwrap_or_default();

    // Group clicks by link and day
    let mut sparklines: Vec<SparklineData> = Vec::new();
    
    for link_id in owned_ids {
        let mut daily_counts: Vec<i64> = vec![0; 7];
        let mut total: i64 = 0;
        
        for event in &events {
            if event.link_id == link_id {
                let duration = now.signed_duration_since(event.created_at);
                let days_ago = duration.num_days();
                if (0..7).contains(&days_ago) {
                    let idx = (6 - days_ago) as usize;
                    daily_counts[idx] += 1;
                    total += 1;
                }
            }
        }
        
        sparklines.push(SparklineData {
            link_id,
            data: daily_counts,
            labels: labels.clone(),
            total,
        });
    }

    (StatusCode::OK, Json(SparklineResponse { sparklines })).into_response()
}

// ============= New Feature: Link Preview (OG Metadata) =============

#[derive(Serialize, ToSchema)]
pub struct LinkPreviewData {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    pub site_name: Option<String>,
    pub favicon: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct PreviewMetadataRequest {
    pub url: String,
}

#[derive(Deserialize)]
pub struct AvatarProxyQuery {
    pub url: String,
}

/// Map an upstream `Content-Type` to a safe, canonical avatar type, or `None` if
/// it is not an allowed raster image.
///
/// This endpoint is public and reachable by top-level navigation, so whatever we
/// return is rendered by the browser *in this origin*. Reflecting the upstream
/// content-type verbatim (the old `starts_with("image/")` check) let two things
/// through that are XSS, not images:
///   * `image/svg+xml` — an SVG can carry `<script>`, which runs on navigation.
///   * a spoofed `image/png` header on an HTML/JS body — a sniffing browser
///     could execute it.
/// So we allow only inert raster types and return a fixed canonical string for
/// each (never the raw upstream header). SVG is deliberately excluded. The
/// handler additionally sends `X-Content-Type-Options: nosniff` and a locked-down
/// CSP so even a mislabeled body cannot execute.
fn canonical_avatar_content_type(raw: &str) -> Option<&'static str> {
    // Strip any `; charset=…` parameter and normalize case/whitespace.
    let base = raw.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    match base.as_str() {
        "image/png" => Some("image/png"),
        "image/jpeg" | "image/jpg" => Some("image/jpeg"),
        "image/gif" => Some("image/gif"),
        "image/webp" => Some("image/webp"),
        "image/avif" => Some("image/avif"),
        "image/bmp" => Some("image/bmp"),
        "image/x-icon" | "image/vnd.microsoft.icon" => Some("image/x-icon"),
        _ => None,
    }
}

/// Proxy a public-bio avatar image through the server so a bio VISITOR's browser
/// only ever connects to this origin — the external avatar host never learns the
/// visitor's IP (the link-in-bio privacy leak). The fetch is SSRF-guarded
/// (validated + DNS-pinned, redirects re-validated), restricted to successful
/// http(s) responses carrying an allowed raster image type, and size-capped.
pub async fn proxy_bio_avatar(
    axum::extract::Query(query): axum::extract::Query<AvatarProxyQuery>,
) -> axum::response::Response {
    use axum::http::header;
    const MAX_AVATAR_BYTES: usize = 2 * 1024 * 1024;

    if validate_url(&query.url).is_err() {
        return (StatusCode::BAD_REQUEST, "Invalid avatar URL").into_response();
    }

    let response = match ssrf_guarded_fetch(reqwest::Method::GET, &query.url, None).await {
        Ok(r) if r.status().is_success() => r,
        _ => return (StatusCode::BAD_GATEWAY, "Could not fetch avatar").into_response(),
    };

    let raw_content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    // Only inert raster images pass, and we serve a fixed canonical type — never
    // the upstream header, and never SVG (which can execute script).
    let Some(safe_content_type) = canonical_avatar_content_type(raw_content_type) else {
        return (StatusCode::UNSUPPORTED_MEDIA_TYPE, "Avatar is not a supported image type").into_response();
    };

    use futures_util::StreamExt;
    let mut stream = response.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(c) => {
                buf.extend_from_slice(&c);
                if buf.len() >= MAX_AVATAR_BYTES {
                    buf.truncate(MAX_AVATAR_BYTES);
                    break;
                }
            }
            Err(_) => return (StatusCode::BAD_GATEWAY, "Failed to read avatar").into_response(),
        }
    }

    (
        [
            (header::CONTENT_TYPE, safe_content_type.to_string()),
            (header::CACHE_CONTROL, "public, max-age=86400".to_string()),
            // Defense in depth: never sniff the body past the declared type, and
            // forbid any active content from executing even if one slips through.
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
            (header::CONTENT_SECURITY_POLICY, "default-src 'none'; sandbox".to_string()),
            (header::CONTENT_DISPOSITION, "inline".to_string()),
        ],
        buf,
    )
        .into_response()
}

/// Fetch Open Graph preview data for a URL
#[utoipa::path(
    post,
    path = "/links/preview-metadata",
    request_body = PreviewMetadataRequest,
    responses(
        (status = 200, description = "Link preview data", body = LinkPreviewData),
        (status = 400, description = "Invalid URL"),
    ),
    tag = "Links"
)]
pub async fn get_link_preview_metadata(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<PreviewMetadataRequest>,
) -> impl IntoResponse {
    // Require authentication: this performs a server-side fetch of a user-supplied URL.
    if get_user_id_from_header(&state.db, &headers).await.is_none() {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Unauthorized"}))).into_response();
    }

    // Validate URL
    let parsed = match url::Url::parse(&payload.url) {
        Ok(u) => u,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid URL"}))).into_response(),
    };

    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Only HTTP/HTTPS URLs supported"}))).into_response();
    }

    // SSRF-guarded GET: the host and every redirect hop are validated against
    // private/internal ranges before any request is sent.
    let response = match ssrf_guarded_fetch(
        reqwest::Method::GET,
        &payload.url,
        Some("Mozilla/5.0 (compatible; OPN.ONL LinkPreview/1.0)"),
    ).await {
        Ok(r) => r,
        Err(_) => return (StatusCode::OK, Json(LinkPreviewData {
            url: payload.url.clone(),
            title: None,
            description: None,
            image: None,
            site_name: None,
            favicon: None,
        })).into_response(),
    };

    if !response.status().is_success() {
        return (StatusCode::OK, Json(LinkPreviewData {
            url: payload.url.clone(),
            title: None,
            description: None,
            image: None,
            site_name: None,
            favicon: None,
        })).into_response();
    }

    // Read at most 512 KiB of the body to bound memory (avoids preview-fetch DoS).
    const MAX_PREVIEW_BYTES: usize = 512 * 1024;
    use futures_util::StreamExt;
    let mut stream = response.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(c) => {
                buf.extend_from_slice(&c);
                if buf.len() >= MAX_PREVIEW_BYTES {
                    buf.truncate(MAX_PREVIEW_BYTES);
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let html = String::from_utf8_lossy(&buf).to_string();

            // Parse OG tags and meta tags
            let title = extract_meta_content(&html, "og:title")
                .or_else(|| extract_meta_content(&html, "twitter:title"))
                .or_else(|| extract_title_tag(&html));
            
            let description = extract_meta_content(&html, "og:description")
                .or_else(|| extract_meta_content(&html, "twitter:description"))
                .or_else(|| extract_meta_content(&html, "description"));
            
            let image = extract_meta_content(&html, "og:image")
                .or_else(|| extract_meta_content(&html, "twitter:image"))
                .map(|img| resolve_url(&payload.url, &img));
            
            let site_name = extract_meta_content(&html, "og:site_name");
            
            let favicon = extract_favicon(&html)
                .map(|fav| resolve_url(&payload.url, &fav))
                .or_else(|| Some(format!("{}://{}/favicon.ico", parsed.scheme(), parsed.host_str().unwrap_or(""))));

    (StatusCode::OK, Json(LinkPreviewData {
        url: payload.url,
        title,
        description,
        image,
        site_name,
        favicon,
    })).into_response()
}

// Helper functions for HTML parsing
fn extract_meta_content(html: &str, property: &str) -> Option<String> {
    // Try property attribute (og: tags)
    let property_pattern = format!(r#"<meta[^>]*property=["']{}["'][^>]*content=["']([^"']+)["']"#, regex::escape(property));
    if let Ok(re) = regex::Regex::new(&property_pattern) {
        if let Some(caps) = re.captures(html) {
            return caps.get(1).map(|m| html_decode(m.as_str()));
        }
    }
    
    // Try content before property
    let property_pattern2 = format!(r#"<meta[^>]*content=["']([^"']+)["'][^>]*property=["']{}["']"#, regex::escape(property));
    if let Ok(re) = regex::Regex::new(&property_pattern2) {
        if let Some(caps) = re.captures(html) {
            return caps.get(1).map(|m| html_decode(m.as_str()));
        }
    }
    
    // Try name attribute (description, etc.)
    let name_pattern = format!(r#"<meta[^>]*name=["']{}["'][^>]*content=["']([^"']+)["']"#, regex::escape(property));
    if let Ok(re) = regex::Regex::new(&name_pattern) {
        if let Some(caps) = re.captures(html) {
            return caps.get(1).map(|m| html_decode(m.as_str()));
        }
    }
    
    // Try content before name
    let name_pattern2 = format!(r#"<meta[^>]*content=["']([^"']+)["'][^>]*name=["']{}["']"#, regex::escape(property));
    if let Ok(re) = regex::Regex::new(&name_pattern2) {
        if let Some(caps) = re.captures(html) {
            return caps.get(1).map(|m| html_decode(m.as_str()));
        }
    }
    
    None
}

fn extract_title_tag(html: &str) -> Option<String> {
    let re = regex::Regex::new(r"<title[^>]*>([^<]+)</title>").ok()?;
    re.captures(html).and_then(|caps| caps.get(1).map(|m| html_decode(m.as_str().trim())))
}

fn extract_favicon(html: &str) -> Option<String> {
    // Try various favicon link patterns
    let patterns = [
        r#"<link[^>]*rel=["'](?:shortcut )?icon["'][^>]*href=["']([^"']+)["']"#,
        r#"<link[^>]*href=["']([^"']+)["'][^>]*rel=["'](?:shortcut )?icon["']"#,
        r#"<link[^>]*rel=["']apple-touch-icon["'][^>]*href=["']([^"']+)["']"#,
    ];
    
    for pattern in patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            if let Some(caps) = re.captures(html) {
                return caps.get(1).map(|m| m.as_str().to_string());
            }
        }
    }
    None
}

fn resolve_url(base: &str, relative: &str) -> String {
    if relative.starts_with("http://") || relative.starts_with("https://") {
        return relative.to_string();
    }
    
    if let Ok(base_url) = url::Url::parse(base) {
        if let Ok(resolved) = base_url.join(relative) {
            return resolved.to_string();
        }
    }
    
    relative.to_string()
}

fn html_decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}
