use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, Method, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use chrono::{DateTime, Utc};
use rand::distributions::Alphanumeric;
use rand::{Rng, thread_rng};
use sea_orm::sea_query::extension::postgres::PgExpr;
use sea_orm::sea_query::{Alias, Expr};
use sea_orm::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;
use validator::Validate;

use crate::AppState;
use crate::entity::{blocked_domains, blocked_links, click_events, link_tags, links, tags, users};
use crate::handlers::link_access::{
    CallerAccess, LinkAction, listed_links, live_link_for, readable_links,
};
use crate::handlers::websocket::ClickEvent;
use crate::handlers::{json_error, verification_satisfied};
use crate::utils::env::env_flag;
use crate::utils::geoip::{GeoLocation, UserAgentInfo, lookup_ip, parse_user_agent};
use crate::utils::jwt::{PASSWORD_TOO_LONG, decode_jwt, password_exceeds_bcrypt_limit};
use crate::utils::link_owner::link_owner_is_active;
use crate::utils::rate_limiter::too_many_requests;
use crate::utils::retired_codes;
use crate::utils::time::utc_rfc3339;
use crate::utils::validation::ilike_pattern;

/// Check if URL or its domain is blocked. Database failures fail closed: a cache
/// hit must never become an unchecked redirect because the blocklist query died.
async fn check_blocked<C: ConnectionTrait>(db: &C, url: &str) -> Result<(), String> {
    validate_url(url)?;

    let parsed_url = url::Url::parse(url).map_err(|_| "Invalid URL".to_string())?;
    // Same host spelling the blocked domains are stored in (lower-case punycode,
    // no trailing dot), so casing and FQDN-dot variants cannot dodge a block.
    let host = parsed_url
        .host_str()
        .and_then(crate::utils::url_policy::canonical_host)
        .unwrap_or_default();

    // Exact-URL block. Entries are stored in canonical form (see
    // `url_policy::canonical_blocked_url`), so compare the canonical form of
    // this URL: scheme/host case, default port, fragment, trailing slash and an
    // empty query cannot dodge a block. An entry without a query blocks the URL
    // with any query; an entry with one blocks only that exact query.
    let url_candidates = crate::utils::url_policy::blocked_url_candidates(url);
    if url_candidates.is_empty() {
        return Err("Invalid URL".to_string());
    }
    let blocked_url = blocked_links::Entity::find()
        .filter(blocked_links::Column::Url.is_in(url_candidates))
        .one(db)
        .await
        .map_err(|_| "Unable to verify link safety".to_string())?;
    if let Some(blocked) = blocked_url {
        return Err(format!(
            "This URL is blocked: {}",
            blocked
                .reason
                .unwrap_or_else(|| "Policy violation".to_string())
        ));
    }

    // Domain block (host + subdomains). A block on "evil.com" must also block
    // "sub.evil.com", so the set of blocked_domains rows that could match this
    // host is exactly the host itself plus each of its parent domains. Query only
    // those candidates against the indexed, normalized `domain` column instead of
    // loading and scanning the whole table on every uncached redirect. Stored
    // domains are normalized on write (see admin::block_domain) and by migration
    // m20220101_000028 and m20220101_000039, so the candidates compare directly.
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
            .map_err(|_| "Unable to verify link safety".to_string())?;
        if let Some(bd) = hit {
            return Err(format!(
                "This domain is blocked: {}",
                bd.reason.unwrap_or_else(|| "Policy violation".to_string())
            ));
        }
    }

    Ok(())
}

/// Confirm that a cached redirect still represents the active database row.
///
/// Generation checks prevent stale writes after successful invalidation. This
/// read is the fail-closed backstop for a security-state invalidation that could
/// not reach Redis (delete, password/cap/interstitial change, or routing rules).
async fn cached_link_is_still_plain(
    db: &DatabaseConnection,
    code: &str,
    cached: &crate::utils::cache::CachedLink,
) -> Result<bool, DbErr> {
    let Some(current) = links::Entity::find_by_id(cached.id)
        .filter(links::Column::Code.eq(code))
        .filter(links::Column::DeletedAt.is_null())
        .one(db)
        .await?
    else {
        return Ok(false);
    };

    if !current.is_active()
        || current.original_url != cached.original_url
        || current.password_hash.is_some()
        || current.max_clicks.is_some()
        || current.burn_after_reading
        || current.safe_link_interstitial
        || current.user_id != cached.user_id
        || current.org_id != cached.org_id
        || current.expires_at.map(|value| value.and_utc().timestamp()) != cached.expires_at
        || current.starts_at.map(|value| value.and_utc().timestamp()) != cached.starts_at
        || !link_owner_is_active(db, &current).await?
    {
        return Ok(false);
    }

    let routing_rule_count = crate::entity::routing_rules::Entity::find()
        .filter(crate::entity::routing_rules::Column::LinkId.eq(current.id))
        .count(db)
        .await?;
    Ok(routing_rule_count == 0)
}

/// Validate organization membership plus exact folder/tag ownership scope.
///
/// Call this inside the same transaction that inserts or updates the link. Row
/// locks prevent a concurrent membership/resource deletion from slipping between
/// authorization and persistence.
pub(crate) async fn validate_link_resource_scope<C: ConnectionTrait>(
    db: &C,
    user_id: i32,
    org_id: Option<i32>,
    folder_id: Option<i32>,
    tag_ids: &[i32],
) -> Result<bool, DbErr> {
    use crate::entity::{folders, org_members};

    if let Some(org_id) = org_id {
        let member = org_members::Entity::find()
            .filter(org_members::Column::OrgId.eq(org_id))
            .filter(org_members::Column::UserId.eq(user_id))
            .lock_shared()
            .one(db)
            .await?;
        if !member.as_ref().is_some_and(|member| member.can_edit()) {
            return Ok(false);
        }
    }

    if let Some(folder_id) = folder_id {
        let Some(folder) = folders::Entity::find_by_id(folder_id)
            .lock_shared()
            .one(db)
            .await?
        else {
            return Ok(false);
        };
        let matches = match org_id {
            Some(org_id) => folder.org_id == Some(org_id),
            None => folder.org_id.is_none() && folder.user_id == Some(user_id),
        };
        if !matches {
            return Ok(false);
        }
    }

    let mut unique_tag_ids = tag_ids.to_vec();
    unique_tag_ids.sort_unstable();
    unique_tag_ids.dedup();
    if !unique_tag_ids.is_empty() {
        let found = tags::Entity::find()
            .filter(tags::Column::Id.is_in(unique_tag_ids.clone()))
            .lock_shared()
            .all(db)
            .await?;
        if found.len() != unique_tag_ids.len() {
            return Ok(false);
        }
        if found.iter().any(|tag| match org_id {
            Some(org_id) => tag.org_id != Some(org_id),
            None => tag.org_id.is_some() || tag.user_id != Some(user_id),
        }) {
            return Ok(false);
        }
    }

    Ok(true)
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
pub(crate) fn get_max_links_per_user() -> Option<u64> {
    std::env::var("MAX_LINKS_PER_USER")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&n| n > 0)
}

/// Check if URL sanitization is enabled (default: true)
fn is_url_sanitization_enabled() -> bool {
    env_flag("ENABLE_URL_SANITIZATION", true)
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
    if env_flag("BLOCK_DANGEROUS_FILE_EXTENSIONS", true)
        && let Some(ext) = crate::utils::url_policy::dangerous_extension(url)
    {
        return Err(format!(
            "Links to .{ext} files are not allowed (potentially executable content)"
        ));
    }
    if env_flag("BLOCK_RAW_IP_URLS", true) && crate::utils::url_policy::host_is_raw_ip(url) {
        return Err("Links to raw IP addresses are not allowed".to_string());
    }
    Ok(())
}

// ============= URL Validation =============

/// `data:` right after a query or fragment delimiter, where a destination's
/// redirect parameter would treat it as a URL. Not `Wikidata:` or a `/Data:`
/// wiki namespace in a path.
fn contains_nested_data_url(url_lower: &str) -> bool {
    url_lower
        .match_indices("data:")
        .any(|(i, _)| i > 0 && matches!(url_lower.as_bytes()[i - 1], b'=' | b'?' | b'&' | b'#'))
}

/// Longest title and notes a link may carry. Both were unbounded, so one
/// request could store megabytes of text per link.
const MAX_TITLE_CHARS: usize = 500;
const MAX_NOTES_CHARS: usize = 5000;

fn check_link_text(title: Option<&str>, notes: Option<&str>) -> Result<(), String> {
    if title.is_some_and(|t| t.chars().count() > MAX_TITLE_CHARS) {
        return Err(format!(
            "Title must be at most {MAX_TITLE_CHARS} characters"
        ));
    }
    if notes.is_some_and(|n| n.chars().count() > MAX_NOTES_CHARS) {
        return Err(format!(
            "Notes must be at most {MAX_NOTES_CHARS} characters"
        ));
    }
    Ok(())
}

/// Validate URL is http/https only and sanitize if enabled
fn validate_url(url: &str) -> Result<String, String> {
    // Raw CR/LF (and other controls) survive `Url::parse` and then break the
    // redirect response, so one visit can be retried into several clicks.
    if url.chars().any(|c| c.is_ascii_control()) {
        return Err("URL contains control characters".to_string());
    }

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
    if crate::utils::url_policy::is_reserved_hostname(host) {
        return Err("Links to reserved example domains are not allowed".to_string());
    }
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

        // Block a data: URL nested where a redirector would follow it
        // (`?next=data:…`, `#data:…`). A top-level data: URL already failed the
        // scheme check above; matching the bare substring also refused ordinary
        // pages such as https://www.wikidata.org/wiki/Wikidata:Main_Page.
        if contains_nested_data_url(&url_lower) {
            return Err("Data URLs are not allowed".to_string());
        }

        // Block common XSS patterns in URL
        let xss_patterns = [
            "<script",
            "</script>",
            "onerror=",
            "onload=",
            "onclick=",
            "onmouseover=",
            "onfocus=",
            "onblur=",
            "eval(",
            "alert(",
            "document.cookie",
            "document.location",
            "window.location",
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
                || v4.octets()[0] >= 240 // 240.0.0.0/4 reserved
        }
        std::net::IpAddr::V6(v6) => {
            // to_ipv4() unwraps both IPv4-mapped (::ffff:a.b.c.d) and the
            // deprecated IPv4-compatible form (::a.b.c.d). to_ipv4_mapped()
            // misses the latter, so 192.168.1.1 / 169.254.169.254 encoded that
            // way would otherwise pass this guard.
            if let Some(v4) = v6.to_ipv4() {
                return is_disallowed_ip(&std::net::IpAddr::V4(v4));
            }
            v6.is_loopback()
                || v6.is_unspecified()
                || (v6.segments()[0] & 0xfe00) == 0xfc00   // fc00::/7 unique local
                || (v6.segments()[0] & 0xffc0) == 0xfe80 // fe80::/10 link-local
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
    let host = parsed
        .host_str()
        .ok_or_else(|| "URL has no host".to_string())?;
    let port = parsed.port_or_known_default().unwrap_or(80);

    let (addrs, is_literal_ip): (Vec<std::net::SocketAddr>, bool) =
        if let Ok(ip) = host.parse::<std::net::IpAddr>() {
            (vec![std::net::SocketAddr::new(ip, port)], true)
        } else {
            let resolved: Vec<std::net::SocketAddr> = tokio::time::timeout(
                DESTINATION_DNS_TIMEOUT,
                tokio::net::lookup_host((host, port)),
            )
            .await
            .map_err(|_| "Could not resolve host".to_string())?
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

/// Same address check as `/links/health-check`, applied when a destination is
/// stored (create, update, bulk, clone, routing rules). Literal private IPs
/// are already refused by `validate_url`. A name that resolves to any private
/// or internal address is refused too (`127.0.0.1.nip.io` and friends).
///
/// A lookup that fails is not a refusal. NXDOMAIN and a resolver blip share
/// one error, and refusing both would reject not-yet-published hostnames and
/// make every create depend on DNS being up. The click redirect is not a
/// server-side fetch and is not re-resolved.
///
/// The lookup is bounded by `DESTINATION_DNS_TIMEOUT`, so a slow resolver
/// delays a write by at most that long instead of hanging it.
async fn ensure_public_destination(url: &str) -> Result<(), String> {
    match destination_host(url) {
        Some((host, port)) if host_resolves_private(&host, port).await == Some(true) => {
            Err(PRIVATE_DESTINATION.to_string())
        }
        _ => Ok(()),
    }
}

/// How long a destination check waits for DNS before letting the write through.
const DESTINATION_DNS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

const PRIVATE_DESTINATION: &str = "URL resolves to a disallowed (internal/private) address";

/// The DNS name and port a destination connects to. `None` for a URL without a
/// host or with a literal IP, which `validate_url` already checks.
fn destination_host(url: &str) -> Option<(String, u16)> {
    let parsed = url::Url::parse(url).ok()?;
    let port = parsed.port_or_known_default()?;
    match parsed.host()? {
        url::Host::Domain(domain) => Some((domain.to_string(), port)),
        url::Host::Ipv4(_) | url::Host::Ipv6(_) => None,
    }
}

/// Whether any address `host` resolves to is private or internal. `None` when
/// the lookup fails, returns nothing, or takes longer than
/// `DESTINATION_DNS_TIMEOUT`.
async fn host_resolves_private(host: &str, port: u16) -> Option<bool> {
    let lookup = tokio::time::timeout(
        DESTINATION_DNS_TIMEOUT,
        tokio::net::lookup_host((host, port)),
    )
    .await;
    let addrs: Vec<std::net::SocketAddr> = lookup.ok()?.ok()?.collect();
    if addrs.is_empty() {
        return None;
    }
    Some(addrs.iter().any(|sa| is_disallowed_ip(&sa.ip())))
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

        if resp.status().is_redirection()
            && let Some(location) = resp
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
    if !alias
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(
            "Alias can only contain letters, numbers, hyphens, and underscores".to_string(),
        );
    }

    // Cannot start or end with hyphen/underscore
    if alias.starts_with('-')
        || alias.starts_with('_')
        || alias.ends_with('-')
        || alias.ends_with('_')
    {
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
        "health",
        "links",
        "link",
        "auth",
        "admin",
        "orgs",
        "org",
        "organizations",
        "folders",
        "tags",
        "analytics",
        "contact",
        "ws",
        "sse",
        "api",
        "api-docs",
        "swagger-ui",
        "password",
        "verify",
        "preview",
        "me",
        "profile",
        "robots.txt",
        "favicon.ico",
        "sitemap.xml",
        "404",
        // frontend SPA routes (opn.onl/<route>)
        "features",
        "pricing",
        "about",
        "privacy",
        "terms",
        "faq",
        "docs",
        "developers",
        "login",
        "register",
        "dashboard",
        "settings",
        "forgot-password",
        "reset-password",
        "verify-email",
        "r",
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
    /// An empty string clears the title.
    pub title: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub password: Option<String>,
    pub remove_password: Option<bool>,
    pub remove_expiration: Option<bool>,
    /// An empty string clears the notes.
    pub notes: Option<String>,
    /// Accepted so a client that echoes the current code is not ignored
    /// silently. A different value is rejected: the code is not editable.
    pub custom_alias: Option<String>,
    /// Move the link into this folder. `null` is the same as leaving it out;
    /// use `remove_folder` to take the link out of its folder.
    pub folder_id: Option<i32>,
    /// Take the link out of its folder. Wins over `folder_id`.
    pub remove_folder: Option<bool>,
    pub starts_at: Option<DateTime<Utc>>,
    /// Maximum number of clicks. A burn-after-reading link always has a cap:
    /// it is 1 unless this names another.
    pub max_clicks: Option<i32>,
    /// A one-time link. Turning it off also removes the cap it implied
    /// (unless `max_clicks` is set in the same request).
    pub burn_after_reading: Option<bool>,
    pub safe_link_interstitial: Option<bool>,
    pub bio_visible: Option<bool>,
    pub remove_starts_at: Option<bool>,
    /// Remove the click cap. On a burn-after-reading link this also turns
    /// burn-after-reading off, because a one-time link cannot be uncapped.
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
    /// Move every link into this folder. `null` is the same as leaving it out;
    /// use `remove_folder` to take the links out of their folders.
    pub folder_id: Option<i32>,
    /// Take every link out of its folder. Wins over `folder_id`.
    pub remove_folder: Option<bool>,
    pub expires_at: Option<DateTime<Utc>>,
    pub remove_expiration: Option<bool>,
}

/// Scripts and API clients call GET /links with no `limit` and expect the
/// whole list back, so omitting it still returns up to 1000 rows. That is a
/// safety cap, not "every row the user owns": `X-Total-Count` says how many
/// more there are and `offset` reaches them.
const DEFAULT_LINKS_LIMIT: u64 = 1000;
const MAX_LINKS_LIMIT: u64 = 1000;

/// How many rows matched a link list request, ignoring `limit` and `offset`.
/// The body stays a bare JSON array (API clients depend on that), so the
/// total travels in a header; `build_cors` exposes it to the browser.
pub const X_TOTAL_COUNT: axum::http::HeaderName =
    axum::http::HeaderName::from_static("x-total-count");

/// One page of a link list: the rows, and `X-Total-Count`.
pub type LinkPage = (
    [(axum::http::HeaderName, String); 1],
    Json<Vec<LinkResponse>>,
);

pub fn link_page(total: u64, rows: Vec<LinkResponse>) -> LinkPage {
    ([(X_TOTAL_COUNT, total.to_string())], Json(rows))
}

pub(crate) fn clamp_links_limit(limit: Option<u64>) -> u64 {
    limit
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_LINKS_LIMIT)
        .min(MAX_LINKS_LIMIT)
}

/// Apply a link list request's `limit` and `offset` to an already filtered and
/// ordered query.
pub(crate) fn page_links(
    select: Select<links::Entity>,
    limit: Option<u64>,
    offset: Option<u64>,
) -> Select<links::Entity> {
    // Postgres takes OFFSET as a signed 64-bit number, and the driver panics
    // instead of failing when handed a larger one.
    let offset = offset.map(|n| n.min(i64::MAX as u64));
    select.limit(clamp_links_limit(limit)).offset(offset)
}

#[cfg(test)]
mod links_limit_tests {
    use super::{DEFAULT_LINKS_LIMIT, MAX_LINKS_LIMIT, clamp_links_limit};

    #[test]
    fn omitted_or_zero_uses_default() {
        assert_eq!(clamp_links_limit(None), DEFAULT_LINKS_LIMIT);
        assert_eq!(clamp_links_limit(Some(0)), DEFAULT_LINKS_LIMIT);
    }

    #[test]
    fn values_inside_the_range_pass_through() {
        assert_eq!(clamp_links_limit(Some(1)), 1);
        assert_eq!(clamp_links_limit(Some(25)), 25);
        assert_eq!(clamp_links_limit(Some(MAX_LINKS_LIMIT)), MAX_LINKS_LIMIT);
    }

    #[test]
    fn values_above_the_maximum_are_clamped() {
        assert_eq!(
            clamp_links_limit(Some(MAX_LINKS_LIMIT + 1)),
            MAX_LINKS_LIMIT
        );
        assert_eq!(clamp_links_limit(Some(u64::MAX)), MAX_LINKS_LIMIT);
    }
}

#[derive(Deserialize, ToSchema, utoipa::IntoParams)]
pub struct LinksQuery {
    pub folder_id: Option<i32>,
    /// List every link of this organization instead of the caller's own.
    /// Any current member may; others get an empty list.
    pub org_id: Option<i32>,
    pub tag_id: Option<i32>,
    pub search: Option<String>,
    /// Page size. Omitted or 0 defaults to 1000. Maximum 1000.
    pub limit: Option<u64>,
    /// Number of links to skip, in the order the list is returned.
    pub offset: Option<u64>,
}

/// Paging of the folder and tag link lists, the same as `GET /links`.
#[derive(Deserialize, ToSchema, utoipa::IntoParams)]
pub struct LinkPageQuery {
    /// Page size. Omitted or 0 defaults to 1000. Maximum 1000.
    pub limit: Option<u64>,
    /// Number of links to skip, in the order the list is returned.
    pub offset: Option<u64>,
}

#[derive(Serialize, ToSchema)]
pub struct BulkCreateLinkResponse {
    pub links: Vec<CreateLinkResponse>,
    /// One message per URL that was not shortened, e.g. `<url>: <reason>`.
    pub errors: Vec<String>,
    /// The same failures with the submitted URL as its own field, so a client
    /// can tell which lines to keep without parsing the messages.
    pub failed: Vec<BulkCreateFailure>,
}

#[derive(Serialize, ToSchema)]
pub struct BulkCreateFailure {
    /// The URL exactly as submitted.
    pub url: String,
    /// Same text as the matching entry in `errors`.
    pub error: String,
}

/// Record a URL that bulk create turned down, in both response lists.
fn record_bulk_failure(
    errors: &mut Vec<String>,
    failed: &mut Vec<BulkCreateFailure>,
    url: &str,
    message: String,
) {
    failed.push(BulkCreateFailure {
        url: url.to_string(),
        error: message.clone(),
    });
    errors.push(message);
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
    pub click_count: i64,
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

impl LinkResponse {
    /// A stored link as the API shows it. `GET /links` and `GET /links/{id}`
    /// both build their items here, so the two cannot drift apart. The caller
    /// passes the public base URLs because a list computes them once, not per
    /// row.
    fn from_model(l: &links::Model, tags: Vec<TagInfo>, base_url: &str, api_url: &str) -> Self {
        Self {
            id: l.id,
            code: l.code.clone(),
            short_url: format!("{}/{}", base_url, l.code),
            api_url: format!("{}/{}", api_url, l.code),
            original_url: l.original_url.clone(),
            title: l.title.clone(),
            click_count: l.click_count,
            created_at: utc_rfc3339(l.created_at),
            expires_at: l.expires_at.map(utc_rfc3339),
            has_password: l.password_hash.is_some(),
            notes: l.notes.clone(),
            folder_id: l.folder_id,
            org_id: l.org_id,
            starts_at: l.starts_at.map(utc_rfc3339),
            max_clicks: l.max_clicks,
            burn_after_reading: l.burn_after_reading,
            burned_at: l.burned_at.map(utc_rfc3339),
            safe_link_interstitial: l.safe_link_interstitial,
            bio_visible: l.bio_visible,
            is_active: l.is_active(),
            is_pinned: l.is_pinned,
            tags,
        }
    }
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

/// What a correct link password buys: a short-lived proof, carried in the URL
/// the browser is sent to next.
#[derive(Serialize, ToSchema)]
pub struct VerifyPasswordResponse {
    /// `GET /{code}?unlock=...`. Open it to be redirected; it is the normal
    /// redirect, so caps, blocks and routing still apply.
    pub redirect_url: String,
    /// How long the `unlock` proof in `redirect_url` stays valid.
    pub expires_in_seconds: i64,
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

/// How many candidates a random short code may try. The same bound bulk create
/// already uses when an insert loses a race on `links.code`.
const CODE_ATTEMPTS: usize = 8;

/// Whether a candidate is free to hand out.
///
/// `Ok(true)` only when both lookups say it is unused. A lookup that says the
/// code is taken wins over the other lookup failing, so a database error cannot
/// hide a row that is there. `Err` when a lookup failed and nothing has shown
/// the code is taken: that must not be treated as free. A purged code lives
/// only in `retired_codes`, and the UNIQUE constraint on `links.code` does not
/// see it.
pub(crate) fn code_availability<E>(
    in_use: Result<bool, E>,
    retired: Result<bool, E>,
) -> Result<bool, E> {
    match (in_use, retired) {
        (Ok(true), _) | (_, Ok(true)) => Ok(false),
        (Ok(false), Ok(false)) => Ok(true),
        (Ok(false), Err(error)) | (Err(error), Ok(false)) | (Err(_), Err(error)) => Err(error),
    }
}

async fn code_is_free<C: ConnectionTrait>(db: &C, code: &str) -> Result<bool, DbErr> {
    let in_use = links::Entity::find()
        .filter(links::Column::Code.eq(code))
        .one(db)
        .await
        .map(|row| row.is_some());
    // A row already holds the code. The retired lookup cannot change that, and
    // a failure there must not turn a taken code into an error.
    let retired = if matches!(in_use, Ok(true)) {
        Ok(false)
    } else {
        retired_codes::is_retired(db, code).await
    };
    code_availability(in_use, retired)
}

/// Why `allocate_code` stopped without a code.
#[derive(Debug, PartialEq, Eq)]
#[cfg(test)]
enum AllocateError<E> {
    /// A lookup failed, and no later candidate was known to be free.
    Lookup(E),
    /// Every candidate was taken. No lookup failed.
    Exhausted,
}

/// The first code `next` produces that `free` reports as unused, within
/// `CODE_ATTEMPTS`. A failed check spends an attempt and is never treated as
/// free. The last lookup error is the one returned.
///
/// Production calls the same loop inside `unused_code`. This stays a test
/// helper because a closure borrowing the database connection is not `Send`.
#[cfg(test)]
async fn allocate_code<E>(
    mut next: impl FnMut() -> String,
    mut free: impl AsyncFnMut(String) -> Result<bool, E>,
) -> Result<String, AllocateError<E>> {
    let mut last_error = None;
    for _ in 0..CODE_ATTEMPTS {
        let code = next();
        match free(code.clone()).await {
            Ok(true) => return Ok(code),
            Ok(false) => {}
            Err(error) => last_error = Some(error),
        }
    }
    Err(match last_error {
        Some(error) => AllocateError::Lookup(error),
        None => AllocateError::Exhausted,
    })
}

/// The first code `next` produces that no link row holds (live or soft-deleted:
/// the column is globally unique) and that was not retired. Callers pass the
/// random generator; a test passes a scripted sequence. A failed lookup is not
/// a free code: the UNIQUE constraint does not cover a code that only
/// `retired_codes` still remembers.
///
/// The attempt loop is inlined, rather than going through `allocate_code`,
/// because a closure that borrows `db` is not `Send` for every lifetime and
/// the handlers that call this have to be.
pub async fn unused_code<C: ConnectionTrait>(
    db: &C,
    mut next: impl FnMut() -> String,
) -> Result<String, DbErr> {
    let mut last_error = None;
    for _ in 0..CODE_ATTEMPTS {
        let code = next();
        match code_is_free(db, &code).await {
            Ok(true) => return Ok(code),
            Ok(false) => {}
            Err(error) => last_error = Some(error),
        }
    }
    Err(match last_error {
        Some(error) => error,
        None => DbErr::Custom("Could not allocate a short code".to_string()),
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JwtAuthentication {
    pub user_id: i32,
    pub token_version: i32,
}

/// What an `opn_` key is allowed to do. Stored as text on `api_keys.scope`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApiKeyScope {
    Full,
    ReadOnly,
}

impl ApiKeyScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::ReadOnly => "read_only",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "full" => Some(Self::Full),
            "read_only" => Some(Self::ReadOnly),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Authentication {
    Jwt(JwtAuthentication),
    ApiKey { user_id: i32, scope: ApiKeyScope },
}

impl Authentication {
    pub fn user_id(self) -> i32 {
        match self {
            Self::Jwt(auth) => auth.user_id,
            Self::ApiKey { user_id, .. } => user_id,
        }
    }
}

/// Authenticate a bearer credential and preserve its type. JWTs are checked
/// against the user's current `token_version`; API keys are resolved separately.
/// Security-sensitive credential-management endpoints must explicitly require
/// [`Authentication::Jwt`] rather than flattening both credential types to a
/// user id.
fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("Authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

pub async fn authenticate_from_header(
    db: &sea_orm::DatabaseConnection,
    headers: &HeaderMap,
) -> Option<Authentication> {
    let token = bearer_token(headers)?;

    // API key path: tokens prefixed `opn_` are personal access tokens (used by the
    // MCP server / external API clients), looked up by their sha256 hash.
    if token.starts_with("opn_") {
        return resolve_api_key(db, token)
            .await
            .map(|(user_id, scope)| Authentication::ApiKey { user_id, scope });
    }

    let claims = decode_jwt(token).ok()?;
    let user = users::Entity::find_by_id(claims.user_id)
        .filter(users::Column::DeletedAt.is_null())
        .filter(users::Column::DisabledAt.is_null())
        .one(db)
        .await
        .ok()??;
    if user.token_version == claims.token_version {
        Some(Authentication::Jwt(JwtAuthentication {
            user_id: user.id,
            token_version: claims.token_version,
        }))
    } else {
        None
    }
}

/// Authenticate either a JWT or API key for ordinary API operations.
pub async fn get_user_id_from_header(
    db: &sea_orm::DatabaseConnection,
    headers: &HeaderMap,
) -> Option<i32> {
    authenticate_from_header(db, headers)
        .await
        .map(Authentication::user_id)
}

/// A request sent an `Authorization` header that did not authenticate.
pub struct RejectedCredential;

impl IntoResponse for RejectedCredential {
    fn into_response(self) -> axum::response::Response {
        json_error(StatusCode::UNAUTHORIZED, "Invalid or expired credentials")
    }
}

/// Authenticate for an endpoint where credentials are optional. No
/// `Authorization` header is an anonymous caller (`Ok(None)`). A header that
/// is present but does not authenticate (expired or revoked token, unknown
/// key, wrong scheme, garbage) is a 401, never a quiet downgrade to
/// anonymous: the caller believes they are signed in, and would otherwise get
/// back a success that is not attached to their account.
pub async fn optional_user_id_from_header(
    db: &sea_orm::DatabaseConnection,
    headers: &HeaderMap,
) -> Result<Option<i32>, RejectedCredential> {
    if !headers.contains_key(axum::http::header::AUTHORIZATION) {
        return Ok(None);
    }
    get_user_id_from_header(db, headers)
        .await
        .map(Some)
        .ok_or(RejectedCredential)
}

/// Require a revocable JWT. API keys are intentionally rejected even when they
/// belong to the same user.
pub async fn get_jwt_auth_from_header(
    db: &sea_orm::DatabaseConnection,
    headers: &HeaderMap,
) -> Option<JwtAuthentication> {
    match authenticate_from_header(db, headers).await? {
        Authentication::Jwt(auth) => Some(auth),
        Authentication::ApiKey { .. } => None,
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

async fn invalidate_cached_code_required(state: &AppState, code: &str) -> Result<(), String> {
    if let Some(cache) = &state.redis_cache {
        cache
            .invalidate_link(code)
            .await
            .map_err(|error| format!("Cache invalidation failed: {error}"))?;
    }
    Ok(())
}

async fn invalidate_cached_codes_required(
    state: &AppState,
    codes: &[String],
) -> Result<(), String> {
    let mut first_error = None;
    for code in codes {
        if let Err(error) = invalidate_cached_code_required(state, code).await {
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
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
    use base64::Engine as _;
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(key.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(digest)
}

/// How stale `api_keys.last_used_at` may get before a request rewrites it. The
/// value is a hint for the key's owner, not an audit trail; stamping every
/// request put a write on the hottest authenticated path (and one every 30 s per
/// open WebSocket/SSE connection signed in by key).
const API_KEY_LAST_USED_REFRESH_MINUTES: i64 = 5;

struct LiveApiKey {
    id: i32,
    user_id: i32,
    scope: ApiKeyScope,
    last_used_at: Option<chrono::NaiveDateTime>,
}

/// An `opn_` key that would authenticate right now. Does not stamp
/// `last_used_at`. Expired keys return `None` before that stamp, same as an
/// unknown or revoked key.
async fn live_api_key(db: &sea_orm::DatabaseConnection, key: &str) -> Option<LiveApiKey> {
    use crate::entity::api_keys;
    // Instance kill-switch: when ENABLE_API_KEYS=false, keys stop authenticating.
    if !env_flag("ENABLE_API_KEYS", true) {
        return None;
    }
    let hash = hash_api_key(key);
    let rec = api_keys::Entity::find()
        .filter(api_keys::Column::KeyHash.eq(hash))
        .one(db)
        .await
        .ok()??;
    let scope = ApiKeyScope::parse(&rec.scope)?;
    let now = chrono::Utc::now().naive_utc();
    // Checked before the owner lookup and before `last_used_at` is written:
    // an expired key must look the same as one that does not exist.
    if rec.expires_at.is_some_and(|expires| expires <= now) {
        return None;
    }
    let user = users::Entity::find_by_id(rec.user_id)
        .filter(users::Column::DeletedAt.is_null())
        .filter(users::Column::DisabledAt.is_null())
        .one(db)
        .await
        .ok()??;
    Some(LiveApiKey {
        id: rec.id,
        user_id: user.id,
        scope,
        last_used_at: rec.last_used_at,
    })
}

/// Resolve an `opn_` API key to its (non-deleted) owner; best-effort stamps
/// `last_used_at` (at most once per [`API_KEY_LAST_USED_REFRESH_MINUTES`]).
/// Returns None for unknown, revoked, expired keys or deleted owners.
async fn resolve_api_key(
    db: &sea_orm::DatabaseConnection,
    key: &str,
) -> Option<(i32, ApiKeyScope)> {
    use crate::entity::api_keys;
    let live = live_api_key(db, key).await?;
    let now = chrono::Utc::now().naive_utc();
    let refresh_after = chrono::Duration::minutes(API_KEY_LAST_USED_REFRESH_MINUTES);
    if live
        .last_used_at
        .is_none_or(|last| now - last >= refresh_after)
    {
        let am = api_keys::ActiveModel {
            id: Set(live.id),
            last_used_at: Set(Some(now)),
            ..Default::default()
        };
        let _ = am.update(db).await;
    }
    Some((live.user_id, live.scope))
}

/// Scope of an `opn_` bearer token that would authenticate. No database
/// lookup unless the token really is an `opn_` key, and `last_used_at` is
/// left alone: the handler stamps it when the request is allowed through.
async fn authentic_api_key_scope(
    db: &sea_orm::DatabaseConnection,
    headers: &HeaderMap,
) -> Option<ApiKeyScope> {
    let token = bearer_token(headers)?;
    if !token.starts_with("opn_") {
        return None;
    }
    live_api_key(db, token).await.map(|key| key.scope)
}

/// Refuse every non-read method that presents a live read-only API key.
///
/// Handlers authenticate themselves, so a per-handler check would be missed
/// by the next route. GET, HEAD and OPTIONS are not looked up here. An
/// expired or unknown key falls through and the handler answers 401.
/// POST routes that only compute (`/links/build-utm`, `/links/health-check`,
/// `/links/preview-metadata`, `/{code}/verify`) are refused as well.
pub async fn reject_read_only_api_key_writes(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS) {
        return next.run(req).await;
    }
    if authentic_api_key_scope(&state.db, req.headers()).await == Some(ApiKeyScope::ReadOnly) {
        return json_error(StatusCode::FORBIDDEN, "This API key is read-only");
    }
    next.run(req).await
}

fn get_base_url() -> String {
    // Use FRONTEND_URL for short links (e.g., https://opn.onl)
    std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:5173".to_string())
}

fn get_api_url() -> String {
    // Use BASE_URL for direct API/redirect links (e.g., https://l.opn.onl)
    std::env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
}

fn burn_feature_enabled() -> bool {
    env_flag("ENABLE_BURN_AFTER_READING", true)
}

fn interstitial_feature_enabled() -> bool {
    env_flag("ENABLE_SAFE_LINK_INTERSTITIAL", true)
}

fn redirect_confirmed(confirm: Option<&str>) -> bool {
    confirm
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn frontend_interstitial_redirect(code: &str, unlock: Option<&str>) -> axum::response::Response {
    // Send to the SPA interstitial route (/r/<code>), NOT the bare short link —
    // the bare path is proxied straight back here and would loop. The SPA shows
    // the "you're leaving" screen and re-hits this endpoint with ?confirm=1.
    let frontend = get_base_url().trim_end_matches('/').to_string();
    let mut location = format!("{frontend}/r/{code}");
    if let Some(unlock) = unlock {
        location.push_str("?unlock=");
        location.push_str(&urlencoding::encode(unlock));
    }
    Redirect::temporary(&location).into_response()
}

/// Redirect to a destination without leaking a short-lived unlock query in the
/// next request's Referer header.
fn destination_redirect(url: &str) -> axum::response::Response {
    let mut response = Redirect::temporary(url).into_response();
    response.headers_mut().insert(
        axum::http::header::REFERRER_POLICY,
        axum::http::HeaderValue::from_static("no-referrer"),
    );
    response
}

/// The link's smart-routing rules in priority order, or none when conditional
/// routing is switched off (the link then degrades to a plain redirect).
async fn active_routing_rules(
    db: &DatabaseConnection,
    link_id: i32,
) -> Vec<crate::entity::routing_rules::Model> {
    let routing_enabled = env_flag("ENABLE_CONDITIONAL_ROUTING", true);
    if !routing_enabled {
        return Vec::new();
    }
    crate::entity::routing_rules::Entity::find()
        .filter(crate::entity::routing_rules::Column::LinkId.eq(link_id))
        .order_by_asc(crate::entity::routing_rules::Column::Priority)
        .all(db)
        .await
        .unwrap_or_default()
}

/// What routing rules match on: device/OS from the User-Agent, country from the
/// client IP, and the Accept-Language header.
fn routing_signals(headers: &HeaderMap) -> (UserAgentInfo, GeoLocation, Option<&str>) {
    let ip = crate::utils::rate_limiter::client_ip_from_headers(headers);
    let geo = ip.as_ref().map(|ip| lookup_ip(ip)).unwrap_or_default();
    let ua_info = headers
        .get("user-agent")
        .and_then(|h| h.to_str().ok())
        .map(parse_user_agent)
        .unwrap_or_default();
    let accept_language = headers.get("accept-language").and_then(|h| h.to_str().ok());
    (ua_info, geo, accept_language)
}

/// Most link ids bound into one `IN (...)` when loading tags. A tag can sit on
/// more links than a statement has bind parameters for (65,535), so a long
/// list is read in slices.
const TAG_LOOKUP_CHUNK: usize = 10_000;

/// Tags for many links: the `link_tags` rows for the ids, then the distinct
/// `tags` rows. Soft-deleted links are excluded by the caller — this table has
/// no `deleted_at`, so a leftover assignment on a deleted link is only dropped
/// when that link is not in `link_ids`. A database error is returned, never an
/// empty map: a list built from it would show tagged links as untagged.
pub(crate) async fn get_tags_by_link_ids(
    db: &DatabaseConnection,
    link_ids: &[i32],
) -> Result<HashMap<i32, Vec<TagInfo>>, DbErr> {
    let mut by_link: HashMap<i32, Vec<TagInfo>> = HashMap::new();
    if link_ids.is_empty() {
        return Ok(by_link);
    }

    let mut assignments = Vec::new();
    for chunk in link_ids.chunks(TAG_LOOKUP_CHUNK) {
        assignments.extend(
            link_tags::Entity::find()
                .filter(link_tags::Column::LinkId.is_in(chunk.to_vec()))
                .all(db)
                .await?,
        );
    }
    if assignments.is_empty() {
        return Ok(by_link);
    }

    let mut tag_ids: Vec<i32> = assignments.iter().map(|lt| lt.tag_id).collect();
    tag_ids.sort_unstable();
    tag_ids.dedup();
    let tags_list = tags::Entity::find()
        .filter(tags::Column::Id.is_in(tag_ids))
        .all(db)
        .await?;
    let tags_by_id: HashMap<i32, TagInfo> = tags_list
        .into_iter()
        .map(|t| {
            (
                t.id,
                TagInfo {
                    id: t.id,
                    name: t.name,
                    color: t.color,
                },
            )
        })
        .collect();

    for assignment in assignments {
        if let Some(tag) = tags_by_id.get(&assignment.tag_id) {
            by_link
                .entry(assignment.link_id)
                .or_default()
                .push(tag.clone());
        }
    }
    Ok(by_link)
}

/// Tags of a link that was just written. The write has already happened, so a
/// failed read leaves the tags out of the response instead of reporting an
/// error for a change that went through.
async fn get_link_tags(db: &DatabaseConnection, link_id: i32) -> Vec<TagInfo> {
    match get_tags_by_link_ids(db, &[link_id]).await {
        Ok(mut by_link) => by_link.remove(&link_id).unwrap_or_default(),
        Err(error) => {
            tracing::error!("Failed to load tags for link {}: {}", link_id, error);
            Vec::new()
        }
    }
}

// ============= Handlers =============

/// How many times one account may shorten one URL inside [`SAME_URL_WINDOW_MINUTES`].
const SAME_URL_LIMIT: u32 = 10;
const SAME_URL_WINDOW_MINUTES: i64 = 10;

/// Seconds until `opens_at`, rounded up, and never zero. Zero would tell the
/// client to retry immediately, into a window this request just found full.
fn secs_until(opens_at: chrono::NaiveDateTime, now: chrono::NaiveDateTime) -> u64 {
    let remaining = opens_at.signed_duration_since(now);
    if remaining <= chrono::Duration::zero() {
        return 1;
    }
    let secs = u64::try_from(remaining.num_seconds()).unwrap_or(u64::MAX);
    let round_up = u64::from(remaining.subsec_nanos() > 0);
    secs.saturating_add(round_up).max(1)
}

/// Create a new shortened link
///
/// Credentials are optional. Without them the link is anonymous and cannot be
/// put in a folder, tagged, or owned by an organization. Credentials that are
/// sent but do not authenticate (expired, revoked, unknown key) are refused
/// with 401 instead of creating an anonymous link; omit the `Authorization`
/// header to create one. An account can shorten the same URL at most 10 times
/// in 10 minutes. The next request is a 429 in the rate-limit shape, and
/// `Retry-After` is when one of those ten leaves the window.
#[utoipa::path(
    post,
    path = "/links",
    request_body = CreateLinkRequest,
    responses(
        (status = 201, description = "Link created", body = LinkResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Credentials were sent but are invalid, expired or revoked"),
        (status = 403, description = "Email unverified, link cap reached, URL blocked, or custom aliases disabled"),
        (status = 409, description = "Alias already exists"),
    ),
    tag = "Links",
    security((), ("bearer_auth" = []), ("api_key" = []))
)]
pub async fn create_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateLinkRequest>,
) -> impl IntoResponse {
    // Resolve the caller first, so rejected credentials are refused before any
    // validation or DNS lookup is spent on the request.
    let user_id = match optional_user_id_from_header(&state.db, &headers).await {
        Ok(user_id) => user_id,
        Err(rejected) => return rejected.into_response(),
    };

    // Validate URL first
    let validated_url = match validate_url(&payload.original_url) {
        Ok(url) => url,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })).into_response();
        }
    };
    if let Err(e) = ensure_public_destination(&validated_url).await {
        return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })).into_response();
    }

    // An empty password is no password. The unlock form will not submit an
    // empty field, so a link "protected" by "" could never be opened from the
    // UI; the dashboard already omits the field instead of sending "".
    let link_password = payload.password.as_deref().filter(|p| !p.is_empty());
    if link_password.is_some_and(password_exceeds_bcrypt_limit) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: PASSWORD_TOO_LONG.to_string(),
            }),
        )
            .into_response();
    }

    // Check email verification for authenticated users
    if let Some(uid) = user_id {
        let user = users::Entity::find_by_id(uid)
            .one(&state.db)
            .await
            .ok()
            .flatten();

        if let Some(u) = user
            && !verification_satisfied(&state, &u)
        {
            return (
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    error: "Please verify your email address before creating links".to_string(),
                }),
            )
                .into_response();
        }
    }

    // Enforce the per-user link cap (MAX_LINKS_PER_USER). This is surfaced in
    // GET /auth/settings; previously it was advertised but never enforced
    // (fail-open). Applies to authenticated users only (anonymous links have no
    // owner to cap). None/0 = unlimited.
    if let Some(uid) = user_id
        && let Some(cap) = get_max_links_per_user()
    {
        let existing = links::Entity::find()
            .filter(links::Column::UserId.eq(uid))
            .filter(links::Column::DeletedAt.is_null())
            .count(&state.db)
            .await
            .unwrap_or(0);
        if existing >= cap {
            return (
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    error: format!(
                        "You have reached the maximum of {} links for this account",
                        cap
                    ),
                }),
            )
                .into_response();
        }
    }

    // Same URL, 10 times in 10 minutes, counted per account. A database error
    // fail-opens, as the old count did: the insert below still has to succeed.
    if let Some(uid) = user_id {
        let now = chrono::Utc::now().naive_utc();
        let window = chrono::Duration::minutes(SAME_URL_WINDOW_MINUTES);
        let recent = links::Entity::find()
            .filter(links::Column::UserId.eq(uid))
            .filter(links::Column::OriginalUrl.eq(&validated_url))
            .filter(links::Column::CreatedAt.gte(now - window))
            .order_by_asc(links::Column::CreatedAt)
            .order_by_asc(links::Column::Id)
            .all(&state.db)
            .await
            .unwrap_or_default();

        if recent.len() >= SAME_URL_LIMIT as usize {
            // Allowed again once fewer than SAME_URL_LIMIT of these rows are
            // still inside the window, which is when this one ages out.
            let gate = &recent[recent.len() - SAME_URL_LIMIT as usize];
            let retry_after_secs = secs_until(gate.created_at + window, now);
            return too_many_requests(
                retry_after_secs,
                SAME_URL_LIMIT,
                0,
                "You have shortened this URL too many times. Please wait a few minutes."
                    .to_string(),
            );
        }
    }

    // Check if URL or domain is blocked (MUST be checked before any link creation)
    if let Err(e) = check_blocked(&state.db, &validated_url).await {
        return (StatusCode::FORBIDDEN, Json(ErrorResponse { error: e })).into_response();
    }

    // Check if custom aliases are enabled
    let custom_aliases_enabled = env_flag("ENABLE_CUSTOM_ALIASES", true);

    let using_custom_alias = payload.custom_alias.is_some();
    let code = if let Some(alias) = payload.custom_alias {
        // Check if custom aliases are enabled
        if !custom_aliases_enabled {
            return (
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    error: "Custom aliases are disabled".to_string(),
                }),
            )
                .into_response();
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
            return (
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    error: "Alias already taken".to_string(),
                }),
            )
                .into_response();
        }

        // An alias previously used by a now-deleted link cannot be reused: the
        // global UNIQUE on links.code still holds that code, so an insert would
        // fail. Reject explicitly with a clear message rather than 500 later.
        // A code whose link row was purged is reserved the same way, but only
        // the retired_codes table remembers it.
        let exists_deleted = links::Entity::find()
            .filter(links::Column::Code.eq(&alias))
            .filter(links::Column::DeletedAt.is_not_null())
            .one(&state.db)
            .await
            .unwrap_or(None);
        let retired = match retired_codes::is_retired(&state.db, &alias).await {
            Ok(retired) => retired,
            Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Database error"),
        };
        if exists_deleted.is_some() || retired {
            return (
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    error: "This alias was previously used and cannot be reused".to_string(),
                }),
            )
                .into_response();
        }

        alias
    } else {
        match unused_code(&state.db, generate_short_code).await {
            Ok(code) => code,
            Err(error) => {
                tracing::error!("Failed to allocate a short code: {error}");
                return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Database error");
            }
        }
    };

    let password_hash = if let Some(password) = link_password {
        match crate::utils::jwt::hash_password(password).await {
            Ok(h) => Some(h),
            Err(_) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to hash password")
                    .into_response();
            }
        }
    } else {
        None
    };

    // Validate scheduling / limit inputs.
    if let Some(max) = payload.max_clicks
        && max <= 0
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "max_clicks must be greater than 0".to_string(),
            }),
        )
            .into_response();
    }
    if let Err(e) = check_link_text(payload.title.as_deref(), payload.notes.as_deref()) {
        return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })).into_response();
    }
    if let (Some(starts), Some(expires)) = (payload.starts_at, payload.expires_at)
        && starts >= expires
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "starts_at must be before expires_at".to_string(),
            }),
        )
            .into_response();
    }

    // Burn-after-reading (gated by ENABLE_BURN_AFTER_READING). A burn link needs a
    // click cap to ride the existing max_clicks enforcement; default to one-time use.
    let burn_enabled = env_flag("ENABLE_BURN_AFTER_READING", true);
    let burn_after_reading = burn_enabled && payload.burn_after_reading.unwrap_or(false);
    let effective_max_clicks = if burn_after_reading && payload.max_clicks.is_none() {
        Some(1)
    } else {
        payload.max_clicks
    };

    // Safe-link interstitial (gated by ENABLE_SAFE_LINK_INTERSTITIAL).
    let interstitial_enabled = env_flag("ENABLE_SAFE_LINK_INTERSTITIAL", true);
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

    let mut tag_ids = payload.tag_ids.clone().unwrap_or_default();
    tag_ids.sort_unstable();
    tag_ids.dedup();

    // Authorize folder/tag/org scope and insert everything in one transaction.
    // This closes both direct cross-tenant IDs and authorization/deletion races.
    let txn = match state.db.begin().await {
        Ok(t) => t,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Database error".to_string(),
                }),
            )
                .into_response();
        }
    };

    if payload.org_id.is_some() || payload.folder_id.is_some() || !tag_ids.is_empty() {
        let Some(uid) = user_id else {
            let _ = txn.rollback().await;
            return (
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    error: "Authentication required for folders, tags, or organization links"
                        .to_string(),
                }),
            )
                .into_response();
        };
        match validate_link_resource_scope(&txn, uid, payload.org_id, payload.folder_id, &tag_ids)
            .await
        {
            Ok(true) => {}
            Ok(false) => {
                let _ = txn.rollback().await;
                return (
                    StatusCode::FORBIDDEN,
                    Json(ErrorResponse {
                        error: "Folder, tag, or organization access denied".to_string(),
                    }),
                )
                    .into_response();
            }
            Err(_) => {
                let _ = txn.rollback().await;
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "Database error".to_string(),
                    }),
                )
                    .into_response();
            }
        }
    }

    let link_id = match links::Entity::insert(link).exec(&txn).await {
        Ok(link_res) => link_res.last_insert_id,
        Err(err) => {
            let _ = txn.rollback().await;
            // The active/deleted alias lookups run before this transaction, so
            // two concurrent custom_alias creates can both pass and one INSERT
            // hits the unique index. Map that to the documented 409 rather than
            // a 500 "Database error" with no retry signal.
            if using_custom_alias && err.to_string().contains("duplicate key value") {
                return (
                    StatusCode::CONFLICT,
                    Json(ErrorResponse {
                        error: "Alias already taken".to_string(),
                    }),
                )
                    .into_response();
            }
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Database error".to_string(),
                }),
            )
                .into_response();
        }
    };

    for tag_id in tag_ids {
        let link_tag = link_tags::ActiveModel {
            link_id: Set(link_id),
            tag_id: Set(tag_id),
            ..Default::default()
        };
        if link_tag.insert(&txn).await.is_err() {
            let _ = txn.rollback().await;
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to attach tags".to_string(),
                }),
            )
                .into_response();
        }
    }

    if txn.commit().await.is_err() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Database error".to_string(),
            }),
        )
            .into_response();
    }

    let tags = get_link_tags(&state.db, link_id).await;
    // Report what the row actually is: a link created with an expiry already
    // past, or a start still ahead, is not active, and the redirect agrees.
    let is_active = links::Entity::find_by_id(link_id)
        .one(&state.db)
        .await
        .ok()
        .flatten()
        .is_none_or(|link| link.is_active());
    let base_url = get_base_url();
    let api_url = get_api_url();
    (
        StatusCode::CREATED,
        Json(LinkResponse {
            id: link_id,
            code: code.clone(),
            short_url: format!("{}/{}", base_url, code),
            api_url: format!("{}/{}", api_url, code),
            original_url: payload.original_url,
            title: payload.title,
            click_count: 0,
            created_at: utc_rfc3339(chrono::Utc::now().naive_utc()),
            expires_at: payload.expires_at.map(|d| utc_rfc3339(d.naive_utc())),
            has_password: password_hash.is_some(),
            notes: payload.notes,
            folder_id: payload.folder_id,
            org_id: payload.org_id,
            starts_at: payload.starts_at.map(|d| utc_rfc3339(d.naive_utc())),
            max_clicks: effective_max_clicks,
            burn_after_reading,
            burned_at: None,
            safe_link_interstitial,
            bio_visible: false,
            is_active,
            is_pinned: false,
            tags,
        }),
    )
        .into_response()
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
    /// Empty while the destination is withheld: the link is not live, is
    /// password-protected and not unlocked, is one-time, or routes to one of
    /// several destinations.
    pub original_url: String,
    /// Host of `original_url`; empty whenever that is.
    pub domain: String,
    pub has_password: bool,
    pub is_expired: bool,
    /// False when the link is not currently live (scheduled, expired, capped,
    /// burned, or blocked). The destination is always withheld then.
    pub is_active: bool,
    /// Why the link is not live: `not_started`, `expired`, `used_up` (a
    /// one-time link that was opened, or a click cap that was reached) or
    /// `blocked`. `null` while it is live.
    pub inactive_reason: Option<String>,
    /// Whether this is a one-time link. Its destination is never shown ahead of
    /// the visit that uses it up.
    pub burn_after_reading: bool,
    pub created_at: String,
    pub click_count: i64,
    /// Destination reputation signal for the safe-link interstitial.
    pub reputation: ReputationInfo,
    /// Whether the instance has the safe-link interstitial feature enabled.
    pub interstitial_enabled: bool,
    /// Whether this link opted into showing the interstitial before redirecting.
    pub safe_link_interstitial: bool,
}

#[derive(Deserialize, Default, utoipa::IntoParams)]
pub struct PreviewQuery {
    /// Short-lived password proof issued by `POST /{code}/verify`. With it, the
    /// preview describes a password-protected link's destination.
    unlock: Option<String>,
}

/// Get link preview (add + to any short link URL to see preview)
#[utoipa::path(
    get,
    path = "/{code}/preview",
    params(
        ("code" = String, Path, description = "Short link code"),
        PreviewQuery
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
    Query(query): Query<PreviewQuery>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Remove trailing + if present (for URL compatibility)
    let clean_code = code.trim_end_matches('+');

    let link = links::Entity::find()
        .filter(links::Column::Code.eq(clean_code))
        .filter(links::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    // The preview shows the destination, so a disabled or deleted owner's link
    // is as good as gone here too.
    let link = match link {
        Some(link)
            if !link_owner_is_active(&state.db, &link)
                .await
                .unwrap_or(false) =>
        {
            None
        }
        link => link,
    };

    match link {
        Some(link) => {
            let is_expired = if let Some(exp) = link.expires_at {
                exp < Utc::now().naive_utc()
            } else {
                false
            };

            let base_url = get_base_url();

            let interstitial_enabled = env_flag("ENABLE_SAFE_LINK_INTERSTITIAL", true);

            let password_unlocked = match (link.password_hash.as_deref(), query.unlock.as_deref()) {
                (Some(password_hash), Some(token)) => {
                    crate::utils::link_unlock::validate_link_unlock_token(
                        token,
                        link.id,
                        &link.code,
                        password_hash,
                    )
                }
                _ => false,
            };

            // The destination is described only while the link can still be
            // used. GET /{code} answers 410 for a link that is used up, expired,
            // not yet live or blocked, so the public preview must not keep
            // saying where it went. A valid unlock may reveal an ordinary
            // password-protected target so the interstitial can describe it.
            // Burn links stay secret until the authoritative redirect consumes
            // their one-time click.
            let mut blocked = check_blocked(&state.db, &link.original_url).await.is_err();
            let mut destination = None;
            if link.is_active()
                && !blocked
                && !link.burn_after_reading
                && (link.password_hash.is_none() || password_unlocked)
            {
                // Describe where this visitor would actually be sent: the
                // routing rule that wins for them, not just the fallback URL. A
                // weighted A/B split has no single answer, so it is withheld.
                let rules = active_routing_rules(&state.db, link.id).await;
                let candidates = if rules.is_empty() {
                    vec![link.original_url.clone()]
                } else {
                    let (ua_info, geo, accept_language) = routing_signals(&headers);
                    crate::utils::routing::possible_destinations(
                        &rules,
                        &ua_info,
                        &geo,
                        accept_language,
                        &link.original_url,
                    )
                };
                if let [only] = candidates.as_slice() {
                    if *only != link.original_url {
                        blocked = check_blocked(&state.db, only).await.is_err();
                    }
                    if !blocked {
                        destination = Some(only.clone());
                    }
                }
            }

            // Reputation: the internal blocklist is the source of truth we have
            // today. Blocked → malicious; plain HTTP can't be vouched for → unknown;
            // otherwise we have nothing bad on record → safe. A destination that
            // is withheld is never vouched for. (Fails open.)
            let verdict = if blocked {
                "malicious"
            } else if destination
                .as_deref()
                .is_some_and(|url| url.starts_with("https://"))
            {
                "safe"
            } else {
                "unknown"
            };

            // Same precedence as GET /{code}: the link's own state is reported
            // before the blocklist.
            let inactive_reason = link
                .inactive_kind()
                .map(|kind| kind.code())
                .or(blocked.then_some("blocked"))
                .map(str::to_string);

            let (shown_url, shown_domain) = match destination {
                Some(url) => {
                    let domain = url::Url::parse(&url)
                        .map(|u| u.host_str().unwrap_or("unknown").to_string())
                        .unwrap_or_else(|_| "unknown".to_string());
                    (url, domain)
                }
                None => (String::new(), String::new()),
            };

            (
                StatusCode::OK,
                Json(LinkPreviewResponse {
                    code: link.code.clone(),
                    short_url: format!("{}/{}", base_url, link.code),
                    original_url: shown_url,
                    domain: shown_domain,
                    has_password: link.password_hash.is_some(),
                    is_expired,
                    is_active: link.is_active() && !blocked,
                    inactive_reason,
                    burn_after_reading: link.burn_after_reading,
                    created_at: utc_rfc3339(link.created_at),
                    click_count: link.click_count,
                    reputation: ReputationInfo {
                        verdict: verdict.to_string(),
                        source: "internal_blocklist".to_string(),
                    },
                    interstitial_enabled,
                    safe_link_interstitial: link.safe_link_interstitial,
                }),
            )
                .into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Link not found".to_string(),
            }),
        )
            .into_response(),
    }
}

#[derive(Deserialize, Default, utoipa::IntoParams)]
pub struct RedirectQuery {
    /// Set to `1` after the visitor confirms the safe-link interstitial in the SPA.
    confirm: Option<String>,
    /// Short-lived password proof issued by POST /{code}/verify.
    unlock: Option<String>,
}

/// Redirect to original URL
/// The answer for a short code that will not redirect: missing (404) or no
/// longer live (410). A browser gets a small branded page instead of a bare
/// line of text; anything that does not ask for HTML (API clients, link
/// unfurlers, curl) still gets the plain reason. Not cached, since a
/// scheduled link becomes live later.
fn dead_link(headers: &HeaderMap, status: StatusCode, reason: &str) -> axum::response::Response {
    use axum::http::header;

    let wants_html = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|accept| accept.contains("text/html"));
    if !wants_html {
        return (
            status,
            [(header::CACHE_CONTROL, "no-store")],
            reason.to_string(),
        )
            .into_response();
    }

    let home = std::env::var("FRONTEND_URL")
        .ok()
        .map(|url| url.trim().trim_end_matches('/').to_string())
        .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
        .unwrap_or_else(|| "/".to_string());
    let site = url::Url::parse(&home)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_else(|| "opn.onl".to_string());
    let title = if status == StatusCode::NOT_FOUND {
        "Link not found"
    } else {
        "Link unavailable"
    };
    let message = if status == StatusCode::NOT_FOUND {
        "There is no short link at this address. Check it for typos, or ask whoever shared it."
            .to_string()
    } else {
        format!("{}.", reason.trim_end_matches('.'))
    };
    let html = format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>{title} · {site}</title>
<style>
body {{ margin: 0; min-height: 100vh; display: grid; place-items: center; background: #f6f7fb; color: #1d2330; font: 16px/1.6 -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; }}
main {{ box-sizing: border-box; width: calc(100% - 2rem); max-width: 28rem; padding: 2rem; background: #fff; border: 1px solid #e2e5ee; border-radius: 1rem; text-align: center; }}
.brand {{ font-weight: 700; color: #2d58cc; text-decoration: none; }}
h1 {{ font-size: 1.5rem; line-height: 1.25; margin: 1.25rem 0 .5rem; }}
p {{ margin: 0 0 1.5rem; color: #4a5160; }}
.home {{ display: inline-block; padding: .625rem 1.25rem; border-radius: .5rem; background: #2d58cc; color: #fff; text-decoration: none; font-weight: 600; }}
</style>
</head>
<body>
<main>
<a class="brand" href="{home}">{site}</a>
<h1>{title}</h1>
<p>{message}</p>
<a class="home" href="{home}">Go to the home page</a>
</main>
</body>
</html>
"#,
        title = title,
        site = crate::utils::html_escape(&site),
        home = crate::utils::html_escape(&home),
        message = crate::utils::html_escape(&message),
    );
    (
        status,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        html,
    )
        .into_response()
}

/// The answer to a HEAD on a link that can be used up: the link is live, but
/// saying where it goes would defeat the cap, so there is no `Location`.
fn head_without_destination() -> axum::response::Response {
    (
        StatusCode::OK,
        [(axum::http::header::CACHE_CONTROL, "no-store")],
    )
        .into_response()
}

/// What a link unfurler gets for a link that can be used up. It names no
/// destination and carries no Open Graph tags, so the chat card shows just the
/// site name and the link is still unused when the recipient clicks.
fn unfurler_page() -> axum::response::Response {
    use axum::http::header;

    const PAGE: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="robots" content="noindex">
<title>opn.onl</title>
</head>
<body>
<p>This is a short link.</p>
</body>
</html>
"#;
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        PAGE,
    )
        .into_response()
}

#[utoipa::path(
    get,
    path = "/{code}",
    params(
        ("code" = String, Path, description = "Short link code"),
        RedirectQuery
    ),
    responses(
        (status = 410, description = "Checked first. The link is not live, its owner is disabled, its destination is blocked, or a one-time or click-capped link is already used up"),
        (status = 307, description = "Temporary redirect. No X-Link-Password and no valid unlock on a password-protected link goes to the password page at {FRONTEND_URL}/password/{code}. A link behind the safe-link interstitial goes to {FRONTEND_URL}/r/{code} until confirm=1, including a HEAD. Otherwise this is the destination"),
        (status = 401, description = "Only a wrong X-Link-Password. The body is plain text: Invalid password. A missing header is a 307 to the password page, not a 401"),
        (status = 200, description = "A HEAD, or a GET from a known link-preview bot, on a one-time or click-capped link that is neither password-protected nor behind the interstitial: no destination, and nothing is used up"),
        (status = 404, description = "Link not found"),
    ),
    tag = "Links"
)]
pub async fn redirect_link(
    State(state): State<AppState>,
    Path(code): Path<String>,
    Query(query): Query<RedirectQuery>,
    method: Method,
    headers: HeaderMap,
) -> impl IntoResponse {
    use crate::utils::cache::CachedLink;

    let mut cache_generation = None;

    // Try to get from Redis cache first (for non-password-protected links)
    if let Some(cache) = &state.redis_cache {
        match cache.get_link_versioned(&code).await {
            Ok((cached, generation)) => {
                cache_generation = Some(generation);
                if let Some(cached) = cached {
                    // Skip cache for password-protected links, max_clicks links,
                    // and interstitial links (need per-request handling).
                    if !cached.has_password
                        && cached.max_clicks.is_none()
                        && !cached.safe_link_interstitial
                    {
                        let still_plain =
                            match cached_link_is_still_plain(&state.db, &code, &cached).await {
                                Ok(still_plain) => still_plain,
                                Err(error) => {
                                    tracing::error!(
                                        "Failed to verify cached link state for {}: {}",
                                        code,
                                        error
                                    );
                                    return (
                                        StatusCode::SERVICE_UNAVAILABLE,
                                        "Unable to verify link state",
                                    )
                                        .into_response();
                                }
                            };
                        if !still_plain {
                            if let Err(error) = cache.invalidate_link(&code).await {
                                tracing::error!(
                                    "Failed to invalidate stale cached link {}: {}",
                                    code,
                                    error
                                );
                            }
                        } else {
                            // Blocklist checks remain authoritative on cache hits.
                            if check_blocked(&state.db, &cached.original_url)
                                .await
                                .is_err()
                            {
                                if let Err(error) = cache.invalidate_link(&code).await {
                                    tracing::error!(
                                        "Failed to invalidate blocked cached link {}: {}",
                                        code,
                                        error
                                    );
                                }
                                return dead_link(
                                    &headers,
                                    StatusCode::GONE,
                                    "This link has been disabled",
                                );
                            }

                            let now = chrono::Utc::now().timestamp();

                            if let Some(starts_at) = cached.starts_at
                                && now < starts_at
                            {
                                return dead_link(
                                    &headers,
                                    StatusCode::GONE,
                                    "Link is scheduled to activate later",
                                );
                            }

                            if let Some(expires_at) = cached.expires_at
                                && now > expires_at
                            {
                                return dead_link(&headers, StatusCode::GONE, "Link has expired");
                            }

                            // A HEAD gets the GET's answer but is not a visit.
                            if method == Method::HEAD {
                                return destination_redirect(&cached.original_url);
                            }

                            // Record click using buffer (synchronous, non-blocking).
                            // Only uncapped links reach the cache fast-path.
                            record_click_buffered(
                                &state.click_buffer,
                                state.ws_state.as_ref().map(|w| w.as_ref()),
                                cached.id,
                                &code,
                                cached.user_id,
                                cached.org_id,
                                ClickAccounting::Buffered {
                                    db_click_count: cached.click_count,
                                },
                                &headers,
                            );

                            // Invalidate after click so any cached accounting snapshot
                            // is not reused. Failure is non-security-critical here.
                            let cache_clone = state.redis_cache.clone();
                            let code_clone = code.clone();
                            tokio::spawn(async move {
                                if let Some(c) = cache_clone {
                                    let _ = c.invalidate_link(&code_clone).await;
                                }
                            });

                            return destination_redirect(&cached.original_url);
                        }
                    }
                }
            }
            Err(error) => {
                tracing::warn!("Redis cache read failed for {}: {}", code, error);
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
            return dead_link(&headers, StatusCode::GONE, reason);
        }

        // A disabled or deleted owner takes their links down with them. Like the
        // blocklist check, this fails closed and runs before anything is cached.
        if !link_owner_is_active(&state.db, &link)
            .await
            .unwrap_or(false)
        {
            return dead_link(&headers, StatusCode::GONE, "This link has been disabled");
        }

        // Enforce content blocking at redirect time so a block applied after the
        // link was created is retroactive. Runs before the caching block below, so
        // a blocked link is never (re)written to the cache.
        if check_blocked(&state.db, &link.original_url).await.is_err() {
            return dead_link(&headers, StatusCode::GONE, "This link has been disabled");
        }

        // Advisory fast-fail for capped links, e.g. so an exhausted link 410s
        // before prompting for a password or interstitial, and so counts still
        // buffered from before a cap was added are respected. This read is NOT
        // the enforcement point — N concurrent requests can all pass it before
        // any click is recorded. The authoritative check is the atomic
        // conditional UPDATE below (consume_capped_click), which runs once the
        // request is actually going to be served a destination.
        if let Some(max) = link.effective_max_clicks()
            && link.click_count + state.click_buffer.pending_count(link.id) >= i64::from(max)
        {
            let msg = if link.burn_after_reading {
                "This one-time link has already been opened"
            } else {
                "Link has reached maximum clicks"
            };
            return dead_link(&headers, StatusCode::GONE, msg);
        }

        let mut active_unlock = match (link.password_hash.as_deref(), query.unlock.as_deref()) {
            (Some(password_hash), Some(token))
                if crate::utils::link_unlock::validate_link_unlock_token(
                    token,
                    link.id,
                    &link.code,
                    password_hash,
                ) =>
            {
                Some(token.to_string())
            }
            _ => None,
        };

        if let Some(password_hash) = link.password_hash.as_deref()
            && active_unlock.is_none()
        {
            let Some(pwd) = headers
                .get("x-link-password")
                .and_then(|header| header.to_str().ok())
            else {
                let frontend_url = std::env::var("FRONTEND_URL")
                    .unwrap_or_else(|_| "http://localhost:5173".to_string());
                return Redirect::temporary(&format!("{}/password/{}", frontend_url, code))
                    .into_response();
            };

            // Header-based password checks bypass the /verify middleware, so
            // enforce both the per-IP CPU budget and per-IP+code budget here.
            let ip = crate::utils::rate_limiter::client_ip_from_headers(&headers)
                .map(|ip| crate::utils::rate_limiter::rate_limit_bucket(&ip))
                .unwrap_or_else(|| "unknown".to_string());
            for (limiter, key) in [
                (
                    &state.rate_limiters.password_verify_ip,
                    format!("pwverify-ip:{ip}"),
                ),
                (
                    &state.rate_limiters.password_verify,
                    format!("pwverify:{ip}:{code}"),
                ),
            ] {
                if let crate::utils::rate_limiter::RateLimitResult::Limited {
                    retry_after_secs,
                    limit,
                    remaining,
                } = limiter.check(&key)
                {
                    return too_many_requests(
                        retry_after_secs,
                        limit,
                        remaining,
                        format!(
                            "Rate limit exceeded. Please try again in {retry_after_secs} seconds."
                        ),
                    );
                }
            }

            let pwd = pwd.to_string();
            let password_hash_owned = password_hash.to_string();
            let verified = tokio::task::spawn_blocking(move || {
                bcrypt::verify(&pwd, &password_hash_owned).unwrap_or(false)
            })
            .await
            .unwrap_or(false);
            if !verified {
                return (StatusCode::UNAUTHORIZED, "Invalid password").into_response();
            }

            active_unlock = match crate::utils::link_unlock::create_link_unlock_token(
                link.id,
                &link.code,
                password_hash,
            ) {
                Some(token) => Some(token),
                None => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Unable to create password unlock",
                    )
                        .into_response();
                }
            };
        }

        if link.safe_link_interstitial
            && interstitial_feature_enabled()
            && !redirect_confirmed(query.confirm.as_deref())
        {
            return frontend_interstitial_redirect(&code, active_unlock.as_deref());
        }

        // Smart conditional routing. When enabled and this link has rules, resolve a
        // per-request destination from the visitor's device/OS/country/language.
        // Routed links are never cached (resolution is per-request), so they always
        // reach this DB path. When the flag is off, rules are ignored and the link
        // degrades to a plain redirect. Resolved (and blocklist-checked) BEFORE the
        // cap consume below, so a blocked routed destination can't waste a click
        // slot or burn a one-time link without serving anything.
        let routing_rules = active_routing_rules(&state.db, link.id).await;

        let routed_destination = if !routing_rules.is_empty() {
            let (ua_info, geo, accept_language) = routing_signals(&headers);

            let destination = crate::utils::routing::resolve_destination(
                &routing_rules,
                &ua_info,
                &geo,
                accept_language,
                &link.original_url,
            );

            // A routing rule must not be able to bypass the blocklist.
            if check_blocked(&state.db, &destination).await.is_err() {
                return dead_link(&headers, StatusCode::GONE, "This link has been disabled");
            }
            Some(destination)
        } else {
            None
        };

        // A HEAD, and a GET from a link unfurler (a chat app drawing a preview
        // card before the recipient has clicked), is not a visit: it must not
        // use up a click cap or burn a one-time link, and it is not counted.
        // On a link that can be used up it is not told the destination either,
        // which would defeat the cap.
        let can_be_used_up = link.max_clicks.is_some() || link.burn_after_reading;
        if method == Method::HEAD {
            if can_be_used_up {
                return head_without_destination();
            }
            return destination_redirect(
                routed_destination.as_deref().unwrap_or(&link.original_url),
            );
        }
        if can_be_used_up
            && headers
                .get(axum::http::header::USER_AGENT)
                .and_then(|value| value.to_str().ok())
                .is_some_and(crate::utils::unfurler::is_link_unfurler)
        {
            return unfurler_page();
        }

        // Authoritative cap enforcement. From here on this request will be
        // served a destination, so for capped links the click is consumed NOW
        // with a single conditional UPDATE (click_count < max_clicks). Under
        // concurrency exactly max_clicks requests can win this update — a
        // burn-after-reading link is exactly-once. 0 rows updated means a
        // concurrent request took the last slot (or, transiently, that the cap
        // was just removed / the link soft-deleted by a concurrent update — a
        // retry then sees the link's new state).
        let accounting = if link.effective_max_clicks().is_some() {
            match consume_capped_click(&state.db, link.id).await {
                Ok(Some(new_count)) => ClickAccounting::Consumed {
                    new_click_count: new_count,
                },
                Ok(None) => {
                    let msg = if link.burn_after_reading {
                        "This one-time link has already been opened"
                    } else {
                        "Link has reached maximum clicks"
                    };
                    return dead_link(&headers, StatusCode::GONE, msg);
                }
                // Fail closed: a capped (possibly burn) link must never
                // redirect without its click being counted.
                Err(_) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
                }
            }
        } else {
            ClickAccounting::Buffered {
                db_click_count: link.click_count,
            }
        };

        if let Some(destination) = routed_destination {
            record_click_buffered(
                &state.click_buffer,
                state.ws_state.as_ref().map(|w| w.as_ref()),
                link.id,
                &code,
                link.user_id,
                link.org_id,
                accounting,
                &headers,
            );
            return destination_redirect(&destination);
        }

        // Cache the link for future requests (only plain redirects — no password,
        // click cap, or interstitial, which need the DB path).
        if link.password_hash.is_none()
            && link.effective_max_clicks().is_none()
            && !link.safe_link_interstitial
            && let (Some(cache), Some(generation)) = (&state.redis_cache, cache_generation)
        {
            let cached = CachedLink {
                id: link.id,
                original_url: link.original_url.clone(),
                has_password: false,
                expires_at: link.expires_at.map(|e| e.and_utc().timestamp()),
                starts_at: link.starts_at.map(|s| s.and_utc().timestamp()),
                max_clicks: link.max_clicks,
                click_count: link.click_count,
                user_id: link.user_id,
                org_id: link.org_id,
                safe_link_interstitial: link.safe_link_interstitial,
            };
            if let Err(error) = cache
                .set_link_if_generation(&code, generation, &cached)
                .await
            {
                tracing::warn!("Redis cache write failed for {}: {}", code, error);
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
            link.org_id,
            accounting,
            &headers,
        );

        destination_redirect(&link.original_url)
    } else {
        dead_link(&headers, StatusCode::NOT_FOUND, "Link not found")
    }
}

/// How a click's aggregate count is accounted, so it is counted exactly once.
#[derive(Clone, Copy)]
enum ClickAccounting {
    /// Uncapped link: the click buffer owns the count — the per-link counter is
    /// incremented and added to `links.click_count` at flush.
    Buffered { db_click_count: i64 },
    /// Capped (max_clicks) link: the count was already consumed atomically at
    /// the DB by `consume_capped_click`, so only the analytics event row is
    /// buffered — incrementing the counter too would double-count at flush.
    Consumed { new_click_count: i64 },
}

/// Atomically consume one click slot on a capped link.
///
/// The cap is `max_clicks`, or 1 when a one-time link was saved without one
/// (the same rule as `Model::effective_max_clicks`). The statement reads the
/// row's own columns, not the cap the handler saw, so a concurrent edit cannot
/// be overwritten by a stale value. A single conditional UPDATE so concurrent
/// redirects cannot overshoot: only a row still under its cap is incremented,
/// and `burned_at` is stamped in the same statement when this click exhausts a
/// burn-after-reading link. Returns `Ok(Some(new_click_count))` when a slot
/// was consumed, `Ok(None)` when the cap is already exhausted (or the link is
/// not capped).
async fn consume_capped_click(db: &DatabaseConnection, link_id: i32) -> Result<Option<i64>, DbErr> {
    let stmt = Statement::from_sql_and_values(
        DbBackend::Postgres,
        r#"UPDATE links
           SET click_count = click_count + 1,
               burned_at = CASE
                   WHEN burn_after_reading AND burned_at IS NULL
                        AND click_count + 1 >= COALESCE(max_clicks, 1)
                       THEN $2
                   ELSE burned_at
               END
           WHERE id = $1
             AND deleted_at IS NULL
             AND click_count < COALESCE(max_clicks, CASE WHEN burn_after_reading THEN 1 END)
           RETURNING click_count"#,
        [link_id.into(), chrono::Utc::now().naive_utc().into()],
    );
    let row = db.query_one(stmt).await?;
    row.map(|r| r.try_get::<i64>("", "click_count")).transpose()
}

/// Helper function to record a click event using the click buffer
#[allow(clippy::too_many_arguments)]
fn record_click_buffered(
    click_buffer: &crate::utils::ClickBuffer,
    ws_state: Option<&crate::handlers::websocket::WsState>,
    link_id: i32,
    link_code: &str,
    user_id: Option<i32>,
    org_id: Option<i32>,
    accounting: ClickAccounting,
    headers: &HeaderMap,
) {
    use crate::utils::click_buffer::ClickData;

    // Client IP via the same trust rules as the rate limiter (no spoofable
    // first-XFF token in analytics/geo either).
    let ip = crate::utils::rate_limiter::client_ip_from_headers(headers);

    let user_agent = headers
        .get("user-agent")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    // Store only the referring host, never the full URL — its path/query can
    // carry visitor PII we neither need nor want to retain.
    let referer = headers
        .get("referer")
        .and_then(|h| h.to_str().ok())
        .and_then(crate::utils::privacy::anonymize_referer);

    // GeoIP lookup
    let geo = ip.as_ref().map(|ip| lookup_ip(ip)).unwrap_or_default();

    // Parse user agent
    let ua_info = user_agent
        .as_ref()
        .map(|ua| parse_user_agent(ua))
        .unwrap_or_default();

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
        created_at: None,
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
            org_id,
            click_count: new_click_count,
            country: geo.country,
            city: geo.city,
            device: ua_info.device,
            browser: ua_info.browser,
            timestamp: utc_rfc3339(chrono::Utc::now().naive_utc()),
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
        (status = 200, description = "Password verified", body = VerifyPasswordResponse),
        (status = 400, description = "The link does not require a password"),
        (status = 401, description = "Invalid password"),
        (status = 404, description = "Link not found"),
        (status = 410, description = "Link expired"),
    ),
    tag = "Links"
)]
pub async fn verify_link_password(
    State(state): State<AppState>,
    Path(code): Path<String>,
    _headers: HeaderMap,
    Json(payload): Json<VerifyPasswordRequest>,
) -> impl IntoResponse {
    let link = match links::Entity::find()
        .filter(links::Column::Code.eq(&code))
        .filter(links::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
    {
        Ok(Some(link)) => link,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Link not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Database error".to_string(),
                }),
            )
                .into_response();
        }
    };

    if !link.is_active() {
        let reason = link.inactive_reason().unwrap_or("Link is inactive");
        return (
            StatusCode::GONE,
            Json(ErrorResponse {
                error: reason.to_string(),
            }),
        )
            .into_response();
    }

    if let Some(max) = link.effective_max_clicks()
        && link.click_count + state.click_buffer.pending_count(link.id) >= i64::from(max)
    {
        let msg = if link.burn_after_reading {
            "This one-time link has already been opened"
        } else {
            "Link has reached maximum clicks"
        };
        return (
            StatusCode::GONE,
            Json(ErrorResponse {
                error: msg.to_string(),
            }),
        )
            .into_response();
    }

    let Some(password_hash) = link.password_hash.as_deref() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Link does not require a password".to_string(),
            }),
        )
            .into_response();
    };

    // bcrypt is deliberately expensive; never block a Tokio worker thread.
    let supplied_password = payload.password;
    let hash_for_verify = password_hash.to_string();
    let valid = tokio::task::spawn_blocking(move || {
        bcrypt::verify(&supplied_password, &hash_for_verify).unwrap_or(false)
    })
    .await
    .unwrap_or(false);
    if !valid {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Invalid password".to_string(),
            }),
        )
            .into_response();
    }

    let Some(unlock) =
        crate::utils::link_unlock::create_link_unlock_token(link.id, &link.code, password_hash)
    else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Unable to create password unlock".to_string(),
            }),
        )
            .into_response();
    };

    // Verification is only a proof exchange. The browser must re-enter the
    // authoritative GET /{code} pipeline, which performs blocklist,
    // interstitial, conditional routing, cap consumption, and click accounting.
    let redirect_url = format!(
        "{}/{}?unlock={}",
        get_base_url().trim_end_matches('/'),
        urlencoding::encode(&link.code),
        urlencoding::encode(&unlock),
    );
    (
        StatusCode::OK,
        Json(VerifyPasswordResponse {
            redirect_url,
            expires_in_seconds: crate::utils::link_unlock::LINK_UNLOCK_TTL_SECONDS,
        }),
    )
        .into_response()
}

/// Get QR code for a link
#[utoipa::path(
    get,
    path = "/links/{id}/qr",
    params(
        ("id" = i32, Path, description = "Link ID"),
        QrOptions
    ),
    responses(
        (status = 200, description = "QR code image: PNG, or SVG with `format=svg`", body = Vec<u8>, content_type = ["image/png", "image/svg+xml"]),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden, also for a link that does not exist"),
    ),
    tag = "Links",
    security(("bearer_auth" = []), ("api_key" = []))
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
        None => return json_error(StatusCode::UNAUTHORIZED, "Unauthorized"),
    };

    let link = match live_link_for(&state.db, id, user_id, LinkAction::Read).await {
        Ok(link) => link,
        Err(denied) => return denied.into_response(),
    };

    let url = format!("{}/{}", get_base_url(), link.code);

    // QR branding is a non-destructive kill-switch (default ON). When disabled,
    // all options are ignored and we serve the plain black/white PNG, which is
    // byte-identical to the legacy behavior.
    let branding_enabled = env_flag("ENABLE_QR_BRANDING", true);
    let effective = if branding_enabled {
        opts
    } else {
        QrOptions::default()
    };

    match build_qr_image(&url, &effective) {
        Some((bytes, content_type)) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, content_type)],
            bytes,
        )
            .into_response(),
        None => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to generate QR code",
        ),
    }
}

/// Query options for branded QR rendering. All optional — an empty set renders
/// the plain black/white PNG (byte-identical to the legacy behavior).
#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
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
    // `len` counts bytes and the slices below cut at byte offsets, so anything
    // but six ASCII hex digits (multibyte text included) is rejected up front.
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
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
        let alpha = if a < 24 || (r > 232 && g > 232 && b > 232) {
            0
        } else {
            a
        };
        out.put_pixel(x, y, image::Rgba([fg[0], fg[1], fg[2], alpha]));
    }
    Some(out)
}

/// Linear blend of `over` onto `base` by `alpha` in [0,1].
fn blend_channel(base: u8, over: u8, alpha: f32) -> u8 {
    (over as f32 * alpha + base as f32 * (1.0 - alpha))
        .round()
        .clamp(0.0, 255.0) as u8
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
            img.put_pixel(
                xx,
                yy,
                image::Rgba([
                    blend_channel(cur[0], bg[0], coverage),
                    blend_channel(cur[1], bg[1], coverage),
                    blend_channel(cur[2], bg[2], coverage),
                    255,
                ]),
            );
        }
    }
    let resized =
        image::imageops::resize(&logo, target, target, image::imageops::FilterType::Lanczos3);
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
    let bg = opts
        .bg
        .as_deref()
        .and_then(parse_hex)
        .unwrap_or([255, 255, 255]);
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
        if want_logo
            && let (Some(uri), Some(dim)) =
                (qr_logo_data_uri_tinted(dark), parse_svg_width(&svg_xml))
        {
            let logo_sz = ((dim as f32) * 0.22) as u32;
            let pos = (dim - logo_sz) / 2;
            let center = dim as f32 / 2.0;
            // Circular backplate (matches the PNG's 1.22× plate) so the logo
            // reads cleanly over the modules instead of sitting bare on them.
            let plate_r = (logo_sz as f32 * 1.22) / 2.0;
            let backplate = format!(
                "<circle cx=\"{c}\" cy=\"{c}\" r=\"{r:.2}\" fill=\"{bg}\"/>",
                c = center,
                r = plate_r,
                bg = bg_hex
            );
            let img_tag = format!(
                "<image x=\"{x}\" y=\"{y}\" width=\"{s}\" height=\"{s}\" href=\"{href}\" preserveAspectRatio=\"xMidYMid meet\"/>",
                x = pos,
                y = pos,
                s = logo_sz,
                href = uri
            );
            svg_xml = svg_xml.replace("</svg>", &format!("{}{}</svg>", backplate, img_tag));
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
    use super::{QrOptions, build_qr_image, parse_hex};

    const PNG_MAGIC: &[u8] = &[0x89, b'P', b'N', b'G'];

    fn opts(color: Option<&str>, logo: Option<bool>, format: Option<&str>) -> QrOptions {
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
        let (bytes, ct) = build_qr_image(
            "https://opn.onl/abc123",
            &opts(Some("2f37d8"), Some(true), None),
        )
        .unwrap();
        assert_eq!(ct, "image/png");
        assert!(image::load_from_memory(&bytes).is_ok());
    }

    #[test]
    fn logo_color_follows_foreground() {
        let (blue, _) = build_qr_image(
            "https://opn.onl/abc123",
            &opts(Some("2f37d8"), Some(true), None),
        )
        .unwrap();
        let (rose, _) = build_qr_image(
            "https://opn.onl/abc123",
            &opts(Some("e11d48"), Some(true), None),
        )
        .unwrap();
        assert_ne!(
            blue, rose,
            "tinted logo should change with foreground color"
        );
    }

    #[test]
    fn svg_with_logo_has_backplate() {
        let (bytes, _) = build_qr_image(
            "https://opn.onl/abc123",
            &opts(None, Some(true), Some("svg")),
        )
        .unwrap();
        let s = String::from_utf8(bytes).unwrap();
        // The logo must sit on a circular backplate, not bare on the modules.
        assert!(
            s.contains("<circle"),
            "branded SVG should draw a backplate circle"
        );
    }

    #[test]
    fn svg_with_logo_embeds_image() {
        let (bytes, _) = build_qr_image(
            "https://opn.onl/abc123",
            &opts(None, Some(true), Some("svg")),
        )
        .unwrap();
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

    #[test]
    fn parse_hex_rejects_multibyte_and_signed_input() {
        // Six bytes, four chars: a byte slice at 0..2 would split the euro sign.
        assert_eq!(parse_hex("a\u{20ac}bb"), None);
        assert_eq!(parse_hex("\u{e9}\u{e9}\u{e9}"), None);
        // `from_str_radix` alone would accept a leading sign.
        assert_eq!(parse_hex("+f+f+f"), None);
    }
}

#[cfg(test)]
mod api_key_tests {
    use super::hash_api_key;

    #[test]
    fn hash_is_deterministic_distinct_and_opaque() {
        let a = hash_api_key("opn_abc123");
        assert_eq!(a, hash_api_key("opn_abc123"), "same key → same hash");
        assert_ne!(
            a,
            hash_api_key("opn_different"),
            "different keys → different hashes"
        );
        assert_eq!(a.len(), 44, "sha256 base64 is 44 chars");
        assert!(
            !a.contains("opn_abc123"),
            "hash must not contain the raw key"
        );
    }
}

#[cfg(test)]
mod ssrf_tests {
    use super::{
        PRIVATE_DESTINATION, ValidatedTarget, build_pinned_client, ensure_public_destination,
        host_resolves_private, is_disallowed_ip, resolve_and_validate,
    };
    use std::net::{IpAddr, SocketAddr};

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

    #[test]
    fn ipv4_compatible_private_and_metadata_are_disallowed() {
        for s in ["::192.168.1.1", "::169.254.169.254", "::c0a8:1"] {
            let ip: IpAddr = s.parse().expect(s);
            assert!(
                is_disallowed_ip(&ip),
                "{s} encodes a private/metadata IPv4 and must be refused"
            );
        }
    }

    #[tokio::test]
    async fn literal_private_and_metadata_ips_are_refused() {
        for url in [
            "http://127.0.0.1/",
            "http://127.0.0.1:8080/admin",
            "http://169.254.169.254/latest/meta-data/", // cloud metadata
            "http://10.0.0.5/",
            "http://192.168.1.1/",
            "http://100.64.0.1/", // CGNAT
            "http://[::1]/",      // IPv6 loopback
            "http://[fd00::1]/",  // IPv6 ULA
            "http://0.0.0.0/",
            "http://[::ffff:127.0.0.1]/",  // IPv4-mapped loopback
            "http://[::192.168.1.1]/",     // IPv4-compatible private
            "http://[::169.254.169.254]/", // IPv4-compatible cloud metadata
        ] {
            let err = resolve_and_validate(url)
                .await
                .expect_err(&format!("{url} must be refused"));
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

    /// Storing a destination whose name resolves to loopback is refused. The
    /// hosts file answers for `localhost`, so this needs no network, unlike the
    /// integration tests' public rebinding name.
    #[tokio::test]
    async fn stored_destination_resolving_to_loopback_is_refused() {
        assert_eq!(host_resolves_private("localhost", 80).await, Some(true));
        assert_eq!(
            ensure_public_destination("http://localhost/x").await,
            Err(PRIVATE_DESTINATION.to_string())
        );
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
        assert_eq!(
            target.addrs,
            vec!["93.184.216.34:80".parse::<SocketAddr>().unwrap()]
        );
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
            assert_eq!(
                classify(bad),
                None,
                "{bad:?} must not be served as an avatar"
            );
        }
    }
}

/// A single routing rule as accepted from the API.
#[derive(Deserialize, ToSchema)]
pub struct RoutingRuleInput {
    /// 0-1000; the lowest priority with a matching rule wins. Rules that share
    /// a priority split traffic by `weight`. Defaults to the rule's position in
    /// the list, so rules sent without one are "first match wins".
    pub priority: Option<i32>,
    /// One of Mobile, Tablet, Desktop, Bot. Blank matches any device.
    pub match_device: Option<String>,
    /// Operating-system prefix such as iOS or Windows. Blank matches any.
    pub match_os: Option<String>,
    /// ISO 3166-1 alpha-2 country code. Blank matches any.
    pub match_country: Option<String>,
    /// Primary language code such as en. Blank matches any.
    pub match_lang: Option<String>,
    pub destination_url: String,
    /// 1-1000, default 1. Relative share among rules with the same priority.
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
const MAX_ROUTING_PRIORITY: i32 = 1000;
const MAX_ROUTING_WEIGHT: i32 = 1000;

/// A rule's non-URL fields, checked and in the shape that is stored.
struct ValidRoutingRule {
    priority: i32,
    weight: i32,
    device: Option<String>,
    os: Option<String>,
    country: Option<String>,
    lang: Option<String>,
}

/// `position` is the rule's index in the request: an omitted priority defaults
/// to it so that the list order is the evaluation order.
fn validate_routing_rule(
    position: usize,
    rule: &RoutingRuleInput,
) -> Result<ValidRoutingRule, String> {
    use crate::utils::routing::{
        normalize_country, normalize_device, normalize_lang, normalize_os,
    };

    let n = position + 1;
    let priority = rule.priority.unwrap_or(position as i32);
    if !(0..=MAX_ROUTING_PRIORITY).contains(&priority) {
        return Err(format!(
            "Rule {n}: priority must be between 0 and {MAX_ROUTING_PRIORITY}"
        ));
    }
    let weight = rule.weight.unwrap_or(1);
    if !(1..=MAX_ROUTING_WEIGHT).contains(&weight) {
        return Err(format!(
            "Rule {n}: weight must be between 1 and {MAX_ROUTING_WEIGHT}"
        ));
    }
    let field =
        |checked: Result<Option<String>, String>| checked.map_err(|e| format!("Rule {n}: {e}"));
    Ok(ValidRoutingRule {
        priority,
        weight,
        device: field(normalize_device(rule.match_device.as_deref()))?,
        os: field(normalize_os(rule.match_os.as_deref()))?,
        country: field(normalize_country(rule.match_country.as_deref()))?,
        lang: field(normalize_lang(rule.match_lang.as_deref()))?,
    })
}

/// List the routing rules for a link.
#[utoipa::path(
    get,
    path = "/links/{id}/rules",
    params(("id" = i32, Path, description = "Link ID")),
    responses(
        (status = 200, description = "Routing rules for the link", body = Vec<RoutingRuleResponse>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "Links",
    security(("bearer_auth" = []), ("api_key" = []))
)]
pub async fn get_routing_rules(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return json_error(StatusCode::UNAUTHORIZED, "Unauthorized"),
    };
    if let Err(denied) = live_link_for(&state.db, id, user_id, LinkAction::Read).await {
        return denied.into_response();
    }
    // Rules that share a priority come back in the order they were saved, so
    // the editor shows the list the way it was written.
    let rules = crate::entity::routing_rules::Entity::find()
        .filter(crate::entity::routing_rules::Column::LinkId.eq(id))
        .order_by_asc(crate::entity::routing_rules::Column::Priority)
        .order_by_asc(crate::entity::routing_rules::Column::Id)
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
#[utoipa::path(
    put,
    path = "/links/{id}/rules",
    params(("id" = i32, Path, description = "Link ID")),
    request_body = ReplaceRoutingRulesRequest,
    responses(
        (status = 200, description = "Routing rules replaced", body = RoutingRulesSavedResponse),
        (status = 400, description = "Too many rules, an invalid priority, weight or match condition, or a destination URL that is invalid or blocked"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "Links",
    security(("bearer_auth" = []), ("api_key" = []))
)]
pub async fn replace_routing_rules(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
    Json(payload): Json<ReplaceRoutingRulesRequest>,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return json_error(StatusCode::UNAUTHORIZED, "Unauthorized"),
    };
    // Viewers can read the rules (get_routing_rules) but must not rewrite where
    // the link redirects.
    let link = match live_link_for(&state.db, id, user_id, LinkAction::Write).await {
        Ok(link) => link,
        Err(denied) => return denied.into_response(),
    };

    if payload.rules.len() > MAX_ROUTING_RULES {
        return json_error(
            StatusCode::BAD_REQUEST,
            format!(
                "A link can have at most {} routing rules",
                MAX_ROUTING_RULES
            ),
        );
    }

    // Validate every rule (fields, then destination format + blocklist) before
    // persisting anything.
    let mut validated: Vec<(String, ValidRoutingRule)> = Vec::with_capacity(payload.rules.len());
    for (position, rule) in payload.rules.iter().enumerate() {
        let fields = match validate_routing_rule(position, rule) {
            Ok(fields) => fields,
            Err(e) => return json_error(StatusCode::BAD_REQUEST, e),
        };
        let url = match validate_url(&rule.destination_url) {
            Ok(u) => u,
            Err(e) => return json_error(StatusCode::BAD_REQUEST, e),
        };
        if let Err(e) = ensure_public_destination(&url).await {
            return json_error(StatusCode::BAD_REQUEST, e);
        }
        if check_blocked(&state.db, &url).await.is_err() {
            return json_error(
                StatusCode::BAD_REQUEST,
                "A destination URL is blocked".to_string(),
            );
        }
        validated.push((url, fields));
    }

    let txn = match state.db.begin().await {
        Ok(t) => t,
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Database error"),
    };
    if crate::entity::routing_rules::Entity::delete_many()
        .filter(crate::entity::routing_rules::Column::LinkId.eq(id))
        .exec(&txn)
        .await
        .is_err()
    {
        let _ = txn.rollback().await;
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Database error");
    }
    for (url, rule) in &validated {
        let am = crate::entity::routing_rules::ActiveModel {
            link_id: Set(id),
            priority: Set(rule.priority),
            match_device: Set(rule.device.clone()),
            match_os: Set(rule.os.clone()),
            match_country: Set(rule.country.clone()),
            match_lang: Set(rule.lang.clone()),
            destination_url: Set(url.clone()),
            weight: Set(rule.weight),
            ..Default::default()
        };
        if am.insert(&txn).await.is_err() {
            let _ = txn.rollback().await;
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Database error");
        }
    }
    if txn.commit().await.is_err() {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Database error");
    }

    // A stale fast-path entry would bypass the newly saved routing rules.
    if invalidate_cached_code_required(&state, &link.code)
        .await
        .is_err()
    {
        return json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Routing rules saved, but cache invalidation failed",
        );
    }

    (
        StatusCode::OK,
        Json(RoutingRulesSavedResponse {
            count: validated.len(),
        }),
    )
        .into_response()
}

/// Get user's links with filtering
#[utoipa::path(
    get,
    path = "/links",
    params(LinksQuery),
    responses(
        (
            status = 200,
            description = "The caller's non-deleted links, or every non-deleted link of the organization given by `org_id` when the caller is a member. Pinned links first, then newest first (limit defaults to 1000, maximum 1000). `search` is a case-insensitive substring match on URL, code, title and notes",
            body = Vec<LinkResponse>,
            headers(("X-Total-Count" = u64, description = "How many links match the filters, ignoring `limit` and `offset`"))
        ),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "The links could not be read"),
    ),
    tag = "Links",
    security(("bearer_auth" = []), ("api_key" = []))
)]
pub async fn get_user_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LinksQuery>,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Unauthorized".to_string(),
                }),
            )
                .into_response();
        }
    };

    let mut link_query = links::Entity::find()
        .filter(listed_links(user_id, query.org_id))
        .filter(links::Column::DeletedAt.is_null());

    // Filter by folder
    if let Some(folder_id) = query.folder_id {
        link_query = link_query.filter(links::Column::FolderId.eq(folder_id));
    }

    // Search by URL, code, title, or notes: a case-insensitive substring match
    // in which `%` and `_` in the text are ordinary characters. ILIKE '%…%'
    // cannot use btree idx_links_original_url (or any btree). A pg_trgm GIN
    // index is the real answer if this filter becomes hot; do not add another
    // btree expecting it to serve a leading wildcard.
    if let Some(search) = query.search {
        let pattern = ilike_pattern(&search);
        link_query = link_query.filter(
            Condition::any()
                .add(Expr::col((links::Entity, links::Column::OriginalUrl)).ilike(pattern.clone()))
                .add(Expr::col((links::Entity, links::Column::Code)).ilike(pattern.clone()))
                .add(Expr::col((links::Entity, links::Column::Title)).ilike(pattern.clone()))
                .add(Expr::col((links::Entity, links::Column::Notes)).ilike(pattern)),
        );
    }

    // Filter by tag
    if let Some(tag_id) = query.tag_id {
        link_query = link_query.filter(
            links::Column::Id.in_subquery(crate::handlers::tags::link_ids_with_tag(tag_id)),
        );
    }

    let total = match link_query.clone().count(&state.db).await {
        Ok(total) => total,
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Database error"),
    };

    // Pinned links lead, then newest first. `id` breaks ties: a bulk create
    // stamps many rows with the same instant, and without it the database may
    // order them differently for each page, so paging repeats and skips rows.
    let link_query = link_query
        .order_by_desc(links::Column::IsPinned)
        .order_by_desc(links::Column::CreatedAt)
        .order_by_desc(links::Column::Id);
    let link_query = page_links(link_query, query.limit, query.offset);

    // A failed read is a 500: an empty 200 would show the dashboard's "no links
    // yet" state for an account that has links.
    let user_links = match link_query.all(&state.db).await {
        Ok(rows) => rows,
        Err(error) => {
            tracing::error!("Failed to list links: {}", error);
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Database error");
        }
    };
    let page_ids: Vec<i32> = user_links.iter().map(|l| l.id).collect();
    let mut tags_by_link = match get_tags_by_link_ids(&state.db, &page_ids).await {
        Ok(tags) => tags,
        Err(error) => {
            tracing::error!("Failed to load tags for the link list: {}", error);
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Database error");
        }
    };

    let base_url = get_base_url();
    let api_url = get_api_url();
    let response: Vec<LinkResponse> = user_links
        .iter()
        .map(|l| {
            let tags = tags_by_link.remove(&l.id).unwrap_or_default();
            LinkResponse::from_model(l, tags, &base_url, &api_url)
        })
        .collect();

    link_page(total, response).into_response()
}

/// Get one link
///
/// The same object an item of `GET /links` is. An organization link is
/// readable by any current member, whoever created it. A missing, deleted or
/// someone else's link is the same 403, so the answer does not reveal which
/// ids exist.
#[utoipa::path(
    get,
    path = "/links/{id}",
    params(
        ("id" = i32, Path, description = "Link ID")
    ),
    responses(
        (status = 200, description = "The link", body = LinkResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "No such link, or the caller may not read it"),
    ),
    tag = "Links",
    security(("bearer_auth" = []), ("api_key" = []))
)]
pub async fn get_link(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return json_error(StatusCode::UNAUTHORIZED, "Unauthorized"),
    };

    let link = match live_link_for(&state.db, id, user_id, LinkAction::Read).await {
        Ok(link) => link,
        Err(denied) => return denied.into_response(),
    };

    let tags = get_link_tags(&state.db, link.id).await;
    (
        StatusCode::OK,
        Json(LinkResponse::from_model(
            &link,
            tags,
            &get_base_url(),
            &get_api_url(),
        )),
    )
        .into_response()
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
        (status = 403, description = "Forbidden, also for a link that does not exist"),
    ),
    tag = "Links",
    security(("bearer_auth" = []), ("api_key" = []))
)]
pub async fn delete_link(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Unauthorized".to_string(),
                }),
            )
                .into_response();
        }
    };

    // A missing link answers like a forbidden one, so ids cannot be probed.
    let link = match live_link_for(&state.db, id, user_id, LinkAction::Delete).await {
        Ok(link) => link,
        Err(denied) => return denied.into_response(),
    };

    // Soft delete - set deleted_at timestamp instead of actually deleting
    let mut active_link: links::ActiveModel = link.clone().into();
    active_link.deleted_at = Set(Some(chrono::Utc::now().naive_utc()));

    match active_link.update(&state.db).await {
        Ok(_) => {
            if invalidate_cached_code_required(&state, &link.code)
                .await
                .is_err()
            {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "Link deleted, but cache invalidation failed".to_string(),
                    }),
                )
                    .into_response();
            }
            (
                StatusCode::OK,
                Json(SuccessResponse {
                    message: "Link deleted successfully".to_string(),
                }),
            )
                .into_response()
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to delete link".to_string(),
            }),
        )
            .into_response(),
    }
}

/// The burn flag and click cap a link ends up with after an update.
///
/// A burn-after-reading link is enforced through its click cap
/// (`consume_capped_click`), so one without a cap is flagged as one-time but
/// never burns, and a cap left behind by an un-burned link keeps killing it
/// after the first visit. The pair therefore changes together:
///
/// * burn on: there is always a cap, the one named in the request, else the
///   current one, else 1;
/// * burn turned off, or the cap removed from a burn link: the cap goes too,
///   unless the request names a new one;
/// * a link that was never burn: only an explicit cap or `remove` changes it.
///
/// `burn` is `None` when the request leaves the flag alone (or the feature is
/// switched off); `remove_cap` wins over `max_clicks`.
fn burn_and_cap_after_update(
    stored_burn: bool,
    stored_cap: Option<i32>,
    burn: Option<bool>,
    max_clicks: Option<i32>,
    remove_cap: bool,
) -> (bool, Option<i32>) {
    let named = if remove_cap { None } else { max_clicks };
    let kept = if remove_cap { None } else { stored_cap };
    let burn_now = burn.unwrap_or(stored_burn && !remove_cap);
    let cap = if burn_now {
        named.or(kept).or(Some(1))
    } else if stored_burn {
        named
    } else {
        named.or(kept)
    };
    (burn_now, cap)
}

#[cfg(test)]
mod burn_and_cap_tests {
    use super::burn_and_cap_after_update as resolve;

    #[test]
    fn turning_burn_on_gives_a_one_time_cap_unless_one_exists_or_is_named() {
        assert_eq!(
            resolve(false, None, Some(true), None, false),
            (true, Some(1))
        );
        assert_eq!(
            resolve(false, Some(5), Some(true), None, false),
            (true, Some(5))
        );
        assert_eq!(
            resolve(false, None, Some(true), Some(3), false),
            (true, Some(3))
        );
    }

    #[test]
    fn turning_burn_off_removes_the_cap_it_implied() {
        assert_eq!(
            resolve(true, Some(1), Some(false), None, false),
            (false, None)
        );
        // ... unless the request names a cap of its own.
        assert_eq!(
            resolve(true, Some(1), Some(false), Some(10), false),
            (false, Some(10))
        );
    }

    #[test]
    fn removing_the_cap_of_a_burn_link_turns_burn_off() {
        assert_eq!(resolve(true, Some(1), None, None, true), (false, None));
    }

    #[test]
    fn asking_for_burn_and_no_cap_in_one_request_keeps_the_burn() {
        assert_eq!(
            resolve(true, Some(1), Some(true), None, true),
            (true, Some(1))
        );
        assert_eq!(
            resolve(false, Some(5), Some(true), None, true),
            (true, Some(1))
        );
    }

    #[test]
    fn echoing_the_current_flag_changes_nothing() {
        assert_eq!(
            resolve(true, Some(1), Some(true), None, false),
            (true, Some(1))
        );
        assert_eq!(
            resolve(true, Some(4), Some(true), None, false),
            (true, Some(4))
        );
        // A capped link that is not burn keeps its cap when the form says
        // "burn off" again (the dashboard sends the flag on every save).
        assert_eq!(
            resolve(false, Some(5), Some(false), None, false),
            (false, Some(5))
        );
        assert_eq!(
            resolve(false, None, Some(false), None, false),
            (false, None)
        );
    }

    #[test]
    fn a_burn_link_without_a_cap_is_repaired_by_any_burn_update() {
        assert_eq!(
            resolve(true, None, Some(true), None, false),
            (true, Some(1))
        );
    }

    #[test]
    fn plain_caps_behave_as_before() {
        assert_eq!(
            resolve(false, Some(5), None, Some(9), false),
            (false, Some(9))
        );
        assert_eq!(resolve(false, Some(5), None, None, true), (false, None));
        assert_eq!(resolve(false, None, None, None, false), (false, None));
        // The feature switched off leaves a burn link's pair alone.
        assert_eq!(resolve(true, Some(1), None, None, false), (true, Some(1)));
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
        (status = 400, description = "Invalid input, such as a non-positive `max_clicks`, `starts_at` not before `expires_at`, or a different `custom_alias`"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden, also for a link that does not exist"),
        (status = 500, description = "The link could not be updated"),
    ),
    tag = "Links",
    security(("bearer_auth" = []), ("api_key" = []))
)]
pub async fn update_link(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
    Json(payload): Json<UpdateLinkRequest>,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Unauthorized".to_string(),
                }),
            )
                .into_response();
        }
    };

    // A missing link answers like a forbidden one, so ids cannot be probed.
    let link = match live_link_for(&state.db, id, user_id, LinkAction::Write).await {
        Ok(link) => link,
        Err(denied) => return denied.into_response(),
    };

    let mut active_link: links::ActiveModel = link.clone().into();

    // Validate scheduling / limit inputs the same way create_link does, so an
    // update can't leave a link in an invalid state (e.g. max_clicks <= 0
    // bricks the link; starts_at >= expires_at makes it never active).
    if payload.remove_max_clicks != Some(true)
        && let Some(mc) = payload.max_clicks
        && mc <= 0
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "max_clicks must be greater than 0".to_string(),
            }),
        )
            .into_response();
    }
    let eff_starts: Option<DateTime<Utc>> = if payload.remove_starts_at == Some(true) {
        None
    } else {
        payload
            .starts_at
            .or_else(|| link.starts_at.map(|d| d.and_utc()))
    };
    let eff_expires: Option<DateTime<Utc>> = if payload.remove_expiration == Some(true) {
        None
    } else {
        payload
            .expires_at
            .or_else(|| link.expires_at.map(|d| d.and_utc()))
    };
    if let (Some(s), Some(e)) = (eff_starts, eff_expires)
        && s >= e
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "starts_at must be before expires_at".to_string(),
            }),
        )
            .into_response();
    }

    if let Err(e) = check_link_text(payload.title.as_deref(), payload.notes.as_deref()) {
        return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })).into_response();
    }

    if let Some(ref url) = payload.original_url {
        // Validate URL format and sanitize
        let validated_url = match validate_url(url) {
            Ok(u) => u,
            Err(e) => {
                return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })).into_response();
            }
        };
        // Check if new URL is blocked
        if let Err(e) = check_blocked(&state.db, &validated_url).await {
            return (StatusCode::FORBIDDEN, Json(ErrorResponse { error: e })).into_response();
        }
        if let Err(e) = ensure_public_destination(&validated_url).await {
            return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })).into_response();
        }
        active_link.original_url = Set(validated_url);
    }

    if let Some(alias) = payload.custom_alias.as_deref().filter(|a| !a.is_empty())
        && alias != link.code
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "The short code cannot be changed".to_string(),
            }),
        )
            .into_response();
    }

    if payload.remove_expiration == Some(true) {
        active_link.expires_at = Set(None);
    } else if let Some(expires) = payload.expires_at {
        active_link.expires_at = Set(Some(expires.naive_utc()));
    }

    // "" is not a new password (see create_link); clearing one is what
    // remove_password is for, so an empty field leaves it unchanged.
    if payload.remove_password == Some(true) {
        active_link.password_hash = Set(None);
    } else if let Some(password) = payload.password.filter(|p| !p.is_empty()) {
        if password_exceeds_bcrypt_limit(&password) {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: PASSWORD_TOO_LONG.to_string(),
                }),
            )
                .into_response();
        }
        match crate::utils::jwt::hash_password(&password).await {
            Ok(h) => active_link.password_hash = Set(Some(h)),
            Err(_) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "Failed to hash password".to_string(),
                    }),
                )
                    .into_response();
            }
        }
    }

    // An empty string is how a client clears a text field; storing it
    // would leave a blank title/notes that is not the same as none.
    if let Some(title) = payload.title {
        active_link.title = Set(Some(title).filter(|t| !t.is_empty()));
    }

    if let Some(notes) = payload.notes {
        active_link.notes = Set(Some(notes).filter(|n| !n.is_empty()));
    }

    // `folder_id: null` cannot be told from an absent field, so taking a
    // link out of its folder has its own flag, like the other removals.
    let remove_folder = payload.remove_folder == Some(true);
    let new_folder_id = if remove_folder {
        None
    } else {
        payload.folder_id
    };
    if remove_folder {
        active_link.folder_id = Set(None);
    } else if let Some(folder_id) = new_folder_id {
        active_link.folder_id = Set(Some(folder_id));
    }

    if payload.remove_starts_at == Some(true) {
        active_link.starts_at = Set(None);
    } else if let Some(starts_at) = payload.starts_at {
        active_link.starts_at = Set(Some(starts_at.naive_utc()));
    }

    // Burn-after-reading (gated by ENABLE_BURN_AFTER_READING) and the click
    // cap it rides on are decided together.
    let burn_enabled = burn_feature_enabled();
    let (burn_after_reading, max_clicks) = burn_and_cap_after_update(
        link.burn_after_reading,
        link.max_clicks,
        payload.burn_after_reading.filter(|_| burn_enabled),
        payload.max_clicks,
        payload.remove_max_clicks == Some(true),
    );
    if burn_after_reading != link.burn_after_reading {
        active_link.burn_after_reading = Set(burn_after_reading);
    }
    if max_clicks != link.max_clicks {
        active_link.max_clicks = Set(max_clicks);
    }

    // Safe-link interstitial (gated by ENABLE_SAFE_LINK_INTERSTITIAL).
    let interstitial_enabled = env_flag("ENABLE_SAFE_LINK_INTERSTITIAL", true);
    if interstitial_enabled && let Some(interstitial) = payload.safe_link_interstitial {
        active_link.safe_link_interstitial = Set(interstitial);
    }

    // Link-in-bio visibility (gated by ENABLE_LINK_IN_BIO).
    let link_in_bio_enabled = env_flag("ENABLE_LINK_IN_BIO", true);
    if link_in_bio_enabled && let Some(visible) = payload.bio_visible {
        // A bio page lists its owner's links, so only the creator decides
        // whether an organization link appears on theirs. Editors may
        // change everything else; the dashboard echoes the current value
        // on every save, which is not a change.
        if visible != link.bio_visible && link.user_id != Some(user_id) {
            return (
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    error:
                        "Only the link's creator can choose whether it appears on their bio page"
                            .to_string(),
                }),
            )
                .into_response();
        }
        active_link.bio_visible = Set(visible);
    }

    let txn = match state.db.begin().await {
        Ok(txn) => txn,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Database error".to_string(),
                }),
            )
                .into_response();
        }
    };
    if let Some(folder_id) = new_folder_id {
        match validate_link_resource_scope(&txn, user_id, link.org_id, Some(folder_id), &[]).await {
            Ok(true) => {}
            Ok(false) => {
                let _ = txn.rollback().await;
                return (
                    StatusCode::FORBIDDEN,
                    Json(ErrorResponse {
                        error: "Folder not found or access denied".to_string(),
                    }),
                )
                    .into_response();
            }
            Err(_) => {
                let _ = txn.rollback().await;
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "Database error".to_string(),
                    }),
                )
                    .into_response();
            }
        }
    }

    // consume_capped_click compares only links.click_count. Uncapped clicks
    // still sitting in the buffer would otherwise be spent again after a
    // cap is added, then added on flush. Fold them into click_count in the
    // same transaction that sets max_clicks.
    let pending_to_fold = if let ActiveValue::Set(Some(_)) = &active_link.max_clicks {
        state.click_buffer.take_pending_count(link.id).await
    } else {
        0
    };
    // Add to the stored count in SQL. The row was read before the password
    // hash and the DNS check, so writing `link.click_count + pending` back
    // would undo a buffer flush that landed in between.
    if pending_to_fold > 0
        && links::Entity::update_many()
            .col_expr(
                links::Column::ClickCount,
                Expr::col(links::Column::ClickCount).add(pending_to_fold),
            )
            .filter(links::Column::Id.eq(link.id))
            .exec(&txn)
            .await
            .is_err()
    {
        let _ = txn.rollback().await;
        state
            .click_buffer
            .add_pending_count(link.id, pending_to_fold)
            .await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to update link".to_string(),
            }),
        )
            .into_response();
    }

    match active_link.update(&txn).await {
        Ok(updated) => {
            if txn.commit().await.is_err() {
                state
                    .click_buffer
                    .add_pending_count(link.id, pending_to_fold)
                    .await;
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "Failed to update link".to_string(),
                    }),
                )
                    .into_response();
            }
            if invalidate_cached_code_required(&state, &updated.code)
                .await
                .is_err()
            {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "Link updated, but cache invalidation failed".to_string(),
                    }),
                )
                    .into_response();
            }

            let tags = get_link_tags(&state.db, updated.id).await;
            let base_url = get_base_url();
            let api_url = get_api_url();
            (
                StatusCode::OK,
                Json(LinkResponse {
                    id: updated.id,
                    code: updated.code.clone(),
                    short_url: format!("{}/{}", base_url, updated.code),
                    api_url: format!("{}/{}", api_url, updated.code),
                    original_url: updated.original_url.clone(),
                    title: updated.title.clone(),
                    click_count: updated.click_count,
                    created_at: utc_rfc3339(updated.created_at),
                    expires_at: updated.expires_at.map(utc_rfc3339),
                    has_password: updated.password_hash.is_some(),
                    notes: updated.notes.clone(),
                    folder_id: updated.folder_id,
                    org_id: updated.org_id,
                    starts_at: updated.starts_at.map(utc_rfc3339),
                    max_clicks: updated.max_clicks,
                    burn_after_reading: updated.burn_after_reading,
                    burned_at: updated.burned_at.map(utc_rfc3339),
                    safe_link_interstitial: updated.safe_link_interstitial,
                    bio_visible: updated.bio_visible,
                    is_active: updated.is_active(),
                    is_pinned: updated.is_pinned,
                    tags,
                }),
            )
                .into_response()
        }
        Err(_) => {
            let _ = txn.rollback().await;
            state
                .click_buffer
                .add_pending_count(link.id, pending_to_fold)
                .await;
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to update link".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// Bulk create links
#[utoipa::path(
    post,
    path = "/links/bulk",
    request_body = BulkCreateLinkRequest,
    responses(
        (status = 200, description = "Links created", body = BulkCreateLinkResponse),
        (status = 400, description = "Batch too large"),
        (status = 401, description = "Authentication required"),
    ),
    tag = "Links",
    security(("bearer_auth" = []), ("api_key" = []))
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
        return (
            StatusCode::UNAUTHORIZED,
            Json(BulkCreateLinkResponse {
                links: vec![],
                errors: vec!["Authentication required for bulk link creation".to_string()],
                failed: vec![],
            }),
        )
            .into_response();
    }

    // Cap batch size to avoid unbounded per-item work / DoS.
    if payload.urls.len() > 500 {
        return (
            StatusCode::BAD_REQUEST,
            Json(BulkCreateLinkResponse {
                links: vec![],
                errors: vec!["Too many URLs in one request (max 500)".to_string()],
                failed: vec![],
            }),
        )
            .into_response();
    }

    // Check email verification for authenticated users
    if let Some(uid) = user_id {
        let user = users::Entity::find_by_id(uid)
            .one(&state.db)
            .await
            .ok()
            .flatten();

        if let Some(u) = user
            && !verification_satisfied(&state, &u)
        {
            return (
                StatusCode::FORBIDDEN,
                Json(BulkCreateLinkResponse {
                    links: vec![],
                    failed: vec![],
                    errors: vec![
                        "Please verify your email address before creating links".to_string(),
                    ],
                }),
            )
                .into_response();
        }
    }

    let mut result_links = Vec::new();
    let mut errors = Vec::new();
    let mut failed = Vec::new();
    let base_url = get_base_url();
    // Per-link rate key: charged once per URL below so a bulk request cannot
    // create more links than the single-create budget allows.
    let ip = crate::utils::rate_limiter::client_ip_from_headers(&headers)
        .map(|ip| crate::utils::rate_limiter::rate_limit_bucket(&ip))
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

    // Resolve each distinct destination host once, a few at a time, instead of
    // one lookup per URL in sequence: 500 URLs at the DNS timeout each would
    // hold the request for minutes.
    let private_hosts: std::collections::HashSet<(String, u16)> = {
        use futures::StreamExt;
        let hosts: std::collections::HashSet<(String, u16)> = payload
            .urls
            .iter()
            .filter_map(|url| destination_host(url))
            .collect();
        futures::stream::iter(hosts)
            .map(|(host, port)| async move {
                let private = host_resolves_private(&host, port).await == Some(true);
                private.then_some((host, port))
            })
            .buffer_unordered(16)
            .filter_map(std::future::ready)
            .collect()
            .await
    };

    for url in payload.urls {
        // Charge the per-IP create budget per link. A bulk request is not a
        // discount: once the hourly create budget is spent, the remaining URLs
        // are reported as rate-limited instead of silently amplifying past it.
        if let crate::utils::rate_limiter::RateLimitResult::Limited {
            retry_after_secs, ..
        } = state
            .rate_limiters
            .link_creation
            .check(&format!("create:{}", ip))
        {
            record_bulk_failure(
                &mut errors,
                &mut failed,
                &url,
                format!(
                    "{}: rate limit reached, try again in {}s",
                    url, retry_after_secs
                ),
            );
            continue;
        }

        // Validate URL before creating link. Surface the specific reason
        // (bad format, dangerous file type, raw IP, …) rather than a generic
        // message, so a bulk upload tells the user which links were rejected why.
        if let Err(e) = validate_url(&url) {
            record_bulk_failure(&mut errors, &mut failed, &url, format!("{}: {}", url, e));
            continue;
        }
        if destination_host(&url).is_some_and(|host| private_hosts.contains(&host)) {
            record_bulk_failure(
                &mut errors,
                &mut failed,
                &url,
                format!("{}: {}", url, PRIVATE_DESTINATION),
            );
            continue;
        }

        // Check if URL or domain is blocked
        if let Err(e) = check_blocked(&state.db, &url).await {
            record_bulk_failure(&mut errors, &mut failed, &url, format!("{}: {}", url, e));
            continue;
        }

        // Stop creating once the per-user cap is reached.
        if let Some(0) = remaining_budget {
            record_bulk_failure(
                &mut errors,
                &mut failed,
                &url,
                format!("{}: account link limit reached", url),
            );
            continue;
        }

        // create_link / clone_link loop until links.code is free (the column is
        // globally unique, including soft-deleted rows). Bulk used to mint one
        // 6-character code and surface a duplicate-key error on collision.
        let mut created = false;
        let mut last_duplicate: Option<String> = None;
        for _ in 0..8 {
            let code = match unused_code(&state.db, generate_short_code).await {
                Ok(code) => code,
                Err(error) => {
                    tracing::error!("Failed to allocate a short code: {error}");
                    record_bulk_failure(
                        &mut errors,
                        &mut failed,
                        &url,
                        format!("Failed to shorten {url}: Database error"),
                    );
                    break;
                }
            };

            let txn = match state.db.begin().await {
                Ok(txn) => txn,
                Err(error) => {
                    record_bulk_failure(
                        &mut errors,
                        &mut failed,
                        &url,
                        format!("Failed to shorten {}: {}", url, error),
                    );
                    break;
                }
            };

            let scope_allowed = validate_link_resource_scope(
                &txn,
                user_id.expect("bulk create authentication checked above"),
                payload.org_id,
                payload.folder_id,
                &[],
            )
            .await;
            match scope_allowed {
                Ok(true) => {}
                Ok(false) => {
                    let _ = txn.rollback().await;
                    record_bulk_failure(
                        &mut errors,
                        &mut failed,
                        &url,
                        format!("{}: folder or organization access denied", url),
                    );
                    break;
                }
                Err(error) => {
                    let _ = txn.rollback().await;
                    record_bulk_failure(
                        &mut errors,
                        &mut failed,
                        &url,
                        format!("Failed to shorten {}: {}", url, error),
                    );
                    break;
                }
            }

            let link = links::ActiveModel {
                original_url: Set(url.clone()),
                code: Set(code.clone()),
                user_id: Set(user_id),
                folder_id: Set(payload.folder_id),
                org_id: Set(payload.org_id),
                ..Default::default()
            };

            match links::Entity::insert(link).exec(&txn).await {
                Ok(link_res) => match txn.commit().await {
                    Ok(()) => {
                        result_links.push(CreateLinkResponse {
                            id: link_res.last_insert_id,
                            code: code.clone(),
                            short_url: format!("{}/{}", base_url, code),
                        });
                        if let Some(b) = remaining_budget.as_mut() {
                            *b = b.saturating_sub(1);
                        }
                        created = true;
                        break;
                    }
                    Err(e) => {
                        record_bulk_failure(
                            &mut errors,
                            &mut failed,
                            &url,
                            format!("Failed to shorten {}: {}", url, e),
                        );
                        break;
                    }
                },
                Err(e) => {
                    let _ = txn.rollback().await;
                    if e.to_string().contains("duplicate key value") {
                        last_duplicate = Some(format!("Failed to shorten {}: {}", url, e));
                        continue;
                    }
                    record_bulk_failure(
                        &mut errors,
                        &mut failed,
                        &url,
                        format!("Failed to shorten {}: {}", url, e),
                    );
                    break;
                }
            }
        }
        if !created && let Some(msg) = last_duplicate {
            record_bulk_failure(&mut errors, &mut failed, &url, msg);
        }
    }

    (
        StatusCode::OK,
        Json(BulkCreateLinkResponse {
            links: result_links,
            errors,
            failed,
        }),
    )
        .into_response()
}

/// Drop repeated ids so a bulk request cannot count the same row twice.
fn unique_ids(ids: Vec<i32>) -> Vec<i32> {
    let mut seen = std::collections::HashSet::new();
    ids.into_iter().filter(|id| seen.insert(*id)).collect()
}

/// Bulk delete links. Ids the caller may not delete are skipped, and `deleted`
/// counts only the links actually removed.
#[utoipa::path(
    post,
    path = "/links/bulk/delete",
    request_body = BulkDeleteRequest,
    responses(
        (status = 200, description = "Links deleted", body = BulkDeleteResponse),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Links",
    security(("bearer_auth" = []), ("api_key" = []))
)]
pub async fn bulk_delete_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<BulkDeleteRequest>,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Unauthorized".to_string(),
                }),
            )
                .into_response();
        }
    };

    if payload.ids.len() > 500 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Too many items in one request (max 500)".to_string(),
            }),
        )
            .into_response();
    }

    let access = match CallerAccess::load(&state.db, user_id).await {
        Ok(access) => access,
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Database error"),
    };

    let mut deleted = 0u64;
    let mut invalidated: Vec<String> = Vec::new();

    // Ids the caller may not delete are skipped; `deleted` counts rows changed.
    for id in unique_ids(payload.ids) {
        let link = links::Entity::find_by_id(id)
            .one(&state.db)
            .await
            .ok()
            .flatten();

        if let Some(link) = link
            && access.can(&link, LinkAction::Delete)
            && link.deleted_at.is_none()
        {
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

    // Drop cached redirects for the deleted codes so they stop resolving now,
    // not after the cache TTL.
    if invalidate_cached_codes_required(&state, &invalidated)
        .await
        .is_err()
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Links deleted, but cache invalidation failed".to_string(),
            }),
        )
            .into_response();
    }

    (StatusCode::OK, Json(BulkDeleteResponse { deleted })).into_response()
}

/// Bulk update links. Ids the caller may not edit are skipped, and `updated`
/// counts only the links actually changed. A link is also skipped when the new
/// `expires_at` would not be after its own `starts_at`, or when `folder_id` is
/// not a folder it may be moved into.
#[utoipa::path(
    post,
    path = "/links/bulk/update",
    request_body = BulkUpdateRequest,
    responses(
        (status = 200, description = "Links updated", body = BulkUpdateResponse),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Links",
    security(("bearer_auth" = []), ("api_key" = []))
)]
pub async fn bulk_update_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<BulkUpdateRequest>,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Unauthorized".to_string(),
                }),
            )
                .into_response();
        }
    };

    if payload.ids.len() > 500 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Too many items in one request (max 500)".to_string(),
            }),
        )
            .into_response();
    }

    let access = match CallerAccess::load(&state.db, user_id).await {
        Ok(access) => access,
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Database error"),
    };

    let mut updated = 0u64;
    let mut invalidated: Vec<String> = Vec::new();

    // Ids the caller may not edit are skipped; `updated` counts rows changed.
    for id in unique_ids(payload.ids) {
        let link = links::Entity::find_by_id(id)
            .one(&state.db)
            .await
            .ok()
            .flatten();

        if let Some(link) = link
            && access.can(&link, LinkAction::Write)
            && link.deleted_at.is_none()
        {
            // A new expiry is checked against this link's own start, as a single
            // update does; a link that would end before it starts is skipped.
            if payload.remove_expiration != Some(true)
                && let (Some(expires), Some(starts)) = (payload.expires_at, link.starts_at)
                && starts >= expires.naive_utc()
            {
                continue;
            }

            let code = link.code.clone();
            let org_id = link.org_id;
            let mut active_link: links::ActiveModel = link.into();

            let txn = match state.db.begin().await {
                Ok(txn) => txn,
                Err(_) => continue,
            };
            if payload.remove_folder == Some(true) {
                active_link.folder_id = Set(None);
            } else if let Some(folder_id) = payload.folder_id {
                match validate_link_resource_scope(&txn, user_id, org_id, Some(folder_id), &[])
                    .await
                {
                    Ok(true) => {
                        active_link.folder_id = Set(Some(folder_id));
                    }
                    _ => {
                        let _ = txn.rollback().await;
                        continue;
                    }
                }
            }

            if payload.remove_expiration == Some(true) {
                active_link.expires_at = Set(None);
            } else if let Some(expires) = payload.expires_at {
                active_link.expires_at = Set(Some(expires.naive_utc()));
            }

            match active_link.update(&txn).await {
                Ok(_) => {
                    if txn.commit().await.is_ok() {
                        updated += 1;
                        invalidated.push(code);
                    }
                }
                Err(_) => {
                    let _ = txn.rollback().await;
                }
            }
        }
    }

    // An expiry/state change must drop the cached redirect, or the old target
    // keeps serving until the cache TTL.
    if invalidate_cached_codes_required(&state, &invalidated)
        .await
        .is_err()
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Links updated, but cache invalidation failed".to_string(),
            }),
        )
            .into_response();
    }

    (StatusCode::OK, Json(BulkUpdateResponse { updated })).into_response()
}

#[derive(Deserialize, utoipa::IntoParams)]
pub struct ExportLinksQuery {
    /// Export every link of this organization instead of the caller's own.
    /// Any current member may; others get a header-only file.
    pub org_id: Option<i32>,
}

/// Written first so Excel reads the file as UTF-8 instead of the system code
/// page (a link note in Cyrillic or CJK would otherwise open as mojibake).
const CSV_BOM: &str = "\u{feff}";

/// A CSV cell: always quoted, inner quotes doubled. A spreadsheet reads a cell
/// that starts with `=`, `+`, `-`, `@`, TAB or CR as a formula, so such a value
/// gets a leading apostrophe, which turns it back into text.
fn csv_field(value: &str) -> String {
    let mut escaped = value.replace('"', "\"\"");
    if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        escaped.insert(0, '\'');
    }
    format!("\"{}\"", escaped)
}

/// Export links to CSV
#[utoipa::path(
    get,
    path = "/links/export",
    params(ExportLinksQuery),
    responses(
        (status = 200, description = "UTF-8 CSV file with a byte order mark; timestamps are RFC 3339 in UTC", content_type = "text/csv"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "The links could not be read; no file is produced"),
    ),
    tag = "Links",
    security(("bearer_auth" = []), ("api_key" = []))
)]
pub async fn export_links_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ExportLinksQuery>,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return json_error(StatusCode::UNAUTHORIZED, "Unauthorized"),
    };

    // An export is what a user keeps as a backup, so a failed read must not
    // come back as a file that holds only the header row.
    let user_links = match links::Entity::find()
        .filter(listed_links(user_id, query.org_id))
        .filter(links::Column::DeletedAt.is_null())
        .order_by_desc(links::Column::CreatedAt)
        .all(&state.db)
        .await
    {
        Ok(rows) => rows,
        Err(error) => {
            tracing::error!("Failed to export links: {}", error);
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Database error");
        }
    };

    let base_url = get_base_url();

    let mut csv_content = String::from(CSV_BOM);
    csv_content.push_str(
        "ID,Code,Original URL,Short URL,Click Count,Created At,Expires At,Has Password,Notes,Folder ID,Max Clicks,Starts At\n",
    );

    for link in user_links {
        csv_content.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{}\n",
            link.id,
            csv_field(&link.code),
            csv_field(&link.original_url),
            csv_field(&format!("{}/{}", base_url, link.code)),
            link.click_count,
            csv_field(&utc_rfc3339(link.created_at)),
            csv_field(&link.expires_at.map(utc_rfc3339).unwrap_or_default()),
            link.password_hash.is_some(),
            csv_field(&link.notes.clone().unwrap_or_default()),
            link.folder_id.map(|f| f.to_string()).unwrap_or_default(),
            link.max_clicks.map(|m| m.to_string()).unwrap_or_default(),
            csv_field(&link.starts_at.map(utc_rfc3339).unwrap_or_default()),
        ));
    }

    (
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=\"links.csv\"",
            ),
        ],
        csv_content,
    )
        .into_response()
}

#[cfg(test)]
mod csv_tests {
    use super::csv_field;

    #[test]
    fn cells_a_spreadsheet_would_run_as_formulas_become_text() {
        for risky in ["=1+1", "+1", "-1", "@SUM(A1)", "\tcmd", "\rcmd"] {
            let cell = csv_field(risky);
            assert!(
                cell.starts_with("\"'"),
                "{risky:?} must be prefixed with an apostrophe, got {cell}"
            );
        }
    }

    #[test]
    fn ordinary_cells_are_only_quoted() {
        assert_eq!(csv_field("plain"), "\"plain\"");
        assert_eq!(csv_field(""), "\"\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        // Only the first character decides.
        assert_eq!(csv_field("a=1"), "\"a=1\"");
        assert_eq!(
            csv_field("2026-10-11T10:00:00Z"),
            "\"2026-10-11T10:00:00Z\""
        );
    }
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
///
/// The copy is a fresh link with the same settings: password (the stored
/// hash), schedule, click cap, burn-after-reading, safe-link interstitial,
/// folder, organization, tags and routing rules. Its clicks start at zero and
/// it is not pinned. The link, its tags and its rules are written in one
/// transaction, and the destinations are checked against the blocklist again.
#[utoipa::path(
    post,
    path = "/links/{id}/clone",
    params(
        ("id" = i32, Path, description = "Link ID to clone")
    ),
    responses(
        (status = 201, description = "Link cloned", body = CloneLinkResponse),
        (status = 400, description = "The destination is no longer acceptable"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden (also for a link that does not exist), link cap reached, or a destination is blocked"),
        (status = 500, description = "The link could not be cloned; nothing was created"),
    ),
    tag = "Links",
    security(("bearer_auth" = []), ("api_key" = []))
)]
pub async fn clone_link(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Unauthorized".to_string(),
                }),
            )
                .into_response();
        }
    };

    // The clone is created in the source link's organization, so the caller
    // needs the same standing there as for creating a link in it.
    let link = match live_link_for(&state.db, id, user_id, LinkAction::Clone).await {
        Ok(link) => link,
        Err(denied) => return denied.into_response(),
    };

    // Re-check the destination. A link stored before this rule, or written
    // straight into the database, must not be cloned into a fresh code.
    if let Err(e) = validate_url(&link.original_url) {
        return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })).into_response();
    }
    if let Err(e) = ensure_public_destination(&link.original_url).await {
        return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })).into_response();
    }

    // So is the blocklist: a destination blocked after the link was made
    // must not come back under a new code, and neither must a rule's.
    let rules = match crate::entity::routing_rules::Entity::find()
        .filter(crate::entity::routing_rules::Column::LinkId.eq(id))
        .order_by_asc(crate::entity::routing_rules::Column::Priority)
        .all(&state.db)
        .await
    {
        Ok(rules) => rules,
        Err(error) => {
            tracing::error!(
                "Failed to load routing rules for clone of {}: {}",
                id,
                error
            );
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Database error");
        }
    };
    let destinations = std::iter::once(link.original_url.as_str())
        .chain(rules.iter().map(|r| r.destination_url.as_str()));
    for destination in destinations {
        if let Err(e) = check_blocked(&state.db, destination).await {
            return (StatusCode::FORBIDDEN, Json(ErrorResponse { error: e })).into_response();
        }
    }

    // Clone inserts a new row, so it must honour the same per-user cap as
    // create_link / bulk_create_links. Skipping it lets a user at the
    // advertised MAX_LINKS_PER_USER mint extra active links.
    if let Some(cap) = get_max_links_per_user() {
        let existing = links::Entity::find()
            .filter(links::Column::UserId.eq(user_id))
            .filter(links::Column::DeletedAt.is_null())
            .count(&state.db)
            .await
            .unwrap_or(0);
        if existing >= cap {
            return (
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    error: format!(
                        "You have reached the maximum of {} links for this account",
                        cap
                    ),
                }),
            )
                .into_response();
        }
    }

    // Generate new short code
    let code = match unused_code(&state.db, generate_short_code).await {
        Ok(code) => code,
        Err(error) => {
            tracing::error!("Failed to allocate a short code: {error}");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Database error");
        }
    };

    // A burn link is its cap: the clone starts with the same cap and no
    // clicks, so it is a fresh one-time link. The two features follow the
    // same switches as on create.
    let burn_after_reading = link.burn_after_reading && burn_feature_enabled();
    let max_clicks = if burn_after_reading {
        link.max_clicks.or(Some(1))
    } else {
        link.max_clicks
    };

    // Create new link with same settings but new code
    let new_link = links::ActiveModel {
        original_url: Set(link.original_url.clone()),
        code: Set(code.clone()),
        user_id: Set(Some(user_id)),
        expires_at: Set(link.expires_at),
        password_hash: Set(link.password_hash.clone()),
        title: Set(link
            .title
            .clone()
            .filter(|t| !t.is_empty())
            .map(|t| format!("{} (copy)", t))),
        notes: Set(link.notes.clone()),
        folder_id: Set(link.folder_id),
        org_id: Set(link.org_id),
        starts_at: Set(link.starts_at),
        max_clicks: Set(max_clicks),
        burn_after_reading: Set(burn_after_reading),
        safe_link_interstitial: Set(link.safe_link_interstitial && interstitial_feature_enabled()),
        is_pinned: Set(false), // Don't copy pin status
        ..Default::default()
    };

    // The link, its tags and its rules are one change: a clone that came
    // back without its tags or rules would be a different link.
    let txn = match state.db.begin().await {
        Ok(txn) => txn,
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Database error"),
    };
    let new_id = match links::Entity::insert(new_link).exec(&txn).await {
        Ok(res) => res.last_insert_id,
        Err(_) => {
            let _ = txn.rollback().await;
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to clone link");
        }
    };

    // Tags are copied inside the database, so a link with any number of
    // them is one statement.
    let copy_tags = match sea_orm::sea_query::Query::insert()
        .into_table(link_tags::Entity)
        .columns([link_tags::Column::LinkId, link_tags::Column::TagId])
        .select_from(
            sea_orm::sea_query::Query::select()
                .expr(Expr::val(new_id))
                .column(link_tags::Column::TagId)
                .from(link_tags::Entity)
                .and_where(Expr::col(link_tags::Column::LinkId).eq(id))
                .to_owned(),
        ) {
        Ok(insert) => insert.to_owned(),
        Err(_) => {
            let _ = txn.rollback().await;
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to clone link");
        }
    };
    if txn
        .execute(txn.get_database_backend().build(&copy_tags))
        .await
        .is_err()
    {
        let _ = txn.rollback().await;
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to clone link");
    }

    if !rules.is_empty() {
        let copies = rules
            .into_iter()
            .map(|rule| crate::entity::routing_rules::ActiveModel {
                link_id: Set(new_id),
                priority: Set(rule.priority),
                match_device: Set(rule.match_device),
                match_os: Set(rule.match_os),
                match_country: Set(rule.match_country),
                match_lang: Set(rule.match_lang),
                destination_url: Set(rule.destination_url),
                weight: Set(rule.weight),
                ..Default::default()
            });
        if crate::entity::routing_rules::Entity::insert_many(copies)
            .exec(&txn)
            .await
            .is_err()
        {
            let _ = txn.rollback().await;
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to clone link");
        }
    }

    if txn.commit().await.is_err() {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to clone link");
    }

    let base_url = get_base_url();
    (
        StatusCode::CREATED,
        Json(CloneLinkResponse {
            id: new_id,
            code: code.clone(),
            short_url: format!("{}/{}", base_url, code),
            original_url: link.original_url,
            message: "Link cloned successfully".to_string(),
        }),
    )
        .into_response()
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
        (status = 403, description = "Forbidden, also for a link that does not exist"),
    ),
    tag = "Links",
    security(("bearer_auth" = []), ("api_key" = []))
)]
pub async fn toggle_pin(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Unauthorized".to_string(),
                }),
            )
                .into_response();
        }
    };

    let link = match live_link_for(&state.db, id, user_id, LinkAction::Write).await {
        Ok(link) => link,
        Err(denied) => return denied.into_response(),
    };

    let new_pin_status = !link.is_pinned;
    let mut active_link: links::ActiveModel = link.into();
    active_link.is_pinned = Set(new_pin_status);

    match active_link.update(&state.db).await {
        Ok(_) => (
            StatusCode::OK,
            Json(PinResponse {
                is_pinned: new_pin_status,
                message: if new_pin_status {
                    "Link pinned".to_string()
                } else {
                    "Link unpinned".to_string()
                },
            }),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to update pin status".to_string(),
            }),
        )
            .into_response(),
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
        (status = 500, description = "Database error"),
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
        return (
            StatusCode::OK,
            Json(CheckCodeResponse {
                available: false,
                code: code.to_string(),
                message: e,
            }),
        )
            .into_response();
    }

    // Consider deleted links too: their code is still held by the global UNIQUE
    // constraint on links.code, so it is not actually available for reuse.
    // Purged links' codes are held by retired_codes instead. A failed lookup is
    // not "available": the caller would then save an alias the insert rejects,
    // or one that only the retired-code table still remembers.
    let available = match code_is_free(&state.db, code).await {
        Ok(available) => available,
        Err(error) => {
            tracing::error!("Failed to check alias availability: {error}");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Database error");
        }
    };

    if !available {
        (
            StatusCode::OK,
            Json(CheckCodeResponse {
                available: false,
                code: code.to_string(),
                message: "This alias is already taken".to_string(),
            }),
        )
            .into_response()
    } else {
        (
            StatusCode::OK,
            Json(CheckCodeResponse {
                available: true,
                code: code.to_string(),
                message: "This alias is available".to_string(),
            }),
        )
            .into_response()
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
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Links",
    security(("bearer_auth" = []), ("api_key" = []))
)]
pub async fn check_url_health(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<HealthCheckRequest>,
) -> impl IntoResponse {
    // Require authentication: this performs a server-side fetch of a user-supplied URL.
    if get_user_id_from_header(&state.db, &headers).await.is_none() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(UrlHealthResponse {
                url: payload.url,
                reachable: false,
                status_code: None,
                response_time_ms: None,
                error: Some("Unauthorized".to_string()),
            }),
        )
            .into_response();
    }

    // Validate URL first. Keep the specific reason (raw IP, reserved domain,
    // control characters) instead of collapsing every refusal into one phrase.
    if let Err(e) = validate_url(&payload.url) {
        return (
            StatusCode::BAD_REQUEST,
            Json(UrlHealthResponse {
                url: payload.url,
                reachable: false,
                status_code: None,
                response_time_ms: None,
                error: Some(e),
            }),
        )
            .into_response();
    }

    let start = std::time::Instant::now();

    // SSRF-guarded HEAD: the host and every redirect hop are validated against
    // private/internal address ranges before any request is sent.
    match ssrf_guarded_fetch(reqwest::Method::HEAD, &payload.url, None).await {
        Ok(response) => {
            let elapsed = start.elapsed().as_millis() as u64;
            let status = response.status().as_u16();

            (
                StatusCode::OK,
                Json(UrlHealthResponse {
                    url: payload.url,
                    reachable: response.status().is_success() || response.status().is_redirection(),
                    status_code: Some(status),
                    response_time_ms: Some(elapsed),
                    error: if response.status().is_client_error()
                        || response.status().is_server_error()
                    {
                        Some(format!("HTTP {}", status))
                    } else {
                        None
                    },
                }),
            )
                .into_response()
        }
        Err(e) => {
            let elapsed = start.elapsed().as_millis() as u64;
            (
                StatusCode::OK,
                Json(UrlHealthResponse {
                    url: payload.url,
                    reachable: false,
                    status_code: None,
                    response_time_ms: Some(elapsed),
                    error: Some(e.to_string()),
                }),
            )
                .into_response()
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
pub async fn build_utm_url(Json(payload): Json<BuildUtmRequest>) -> impl IntoResponse {
    // Parse the original URL
    let mut parsed = match url::Url::parse(&payload.url) {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Invalid URL format".to_string(),
                }),
            )
                .into_response();
        }
    };

    // Collect first: `query_pairs_mut` leaves a bare `?` on the URL as soon as
    // it is called, even when nothing is appended.
    let mut utm_params = std::collections::HashMap::new();
    let mut utm_pairs = Vec::new();
    for (name, value) in [
        ("utm_source", &payload.utm_source),
        ("utm_medium", &payload.utm_medium),
        ("utm_campaign", &payload.utm_campaign),
        ("utm_term", &payload.utm_term),
        ("utm_content", &payload.utm_content),
    ] {
        if let Some(value) = value
            && !value.is_empty()
        {
            utm_pairs.push((name, value));
            utm_params.insert(name.to_string(), value.clone());
        }
    }
    if !utm_pairs.is_empty() {
        parsed.query_pairs_mut().extend_pairs(utm_pairs);
    }

    (
        StatusCode::OK,
        Json(BuildUtmResponse {
            original_url: payload.url,
            url_with_utm: parsed.to_string(),
            utm_params,
        }),
    )
        .into_response()
}

// ============= New Feature: Sparkline Data =============

/// Days a sparkline covers: today and the six UTC calendar days before it.
const SPARKLINE_DAYS: i64 = 7;
/// Most ids one sparkline request may carry. The dashboard sends 80 at a time.
const MAX_SPARKLINE_IDS: usize = 500;

#[derive(Serialize, ToSchema)]
pub struct SparklineData {
    pub link_id: i32,
    /// Clicks per UTC calendar day, oldest first; the last entry is today.
    pub data: Vec<i64>,
    /// `MM/DD` (UTC) of each day in `data`.
    pub labels: Vec<String>,
    pub total: i64,
}

#[derive(Serialize, ToSchema)]
pub struct SparklineResponse {
    pub sparklines: Vec<SparklineData>,
}

/// The days a sparkline covers, oldest first, ending with `today`.
fn sparkline_days(today: chrono::NaiveDate) -> Vec<chrono::NaiveDate> {
    (0..SPARKLINE_DAYS)
        .rev()
        .map(|days_ago| today - chrono::Duration::days(days_ago))
        .collect()
}

/// Spread `(link, day, clicks)` rows over one zero-filled series per link. A
/// link with no click in the window comes in as a row without a day.
fn sparkline_series(
    days: &[chrono::NaiveDate],
    rows: Vec<(i32, Option<chrono::NaiveDate>, i64)>,
) -> HashMap<i32, Vec<i64>> {
    let mut series: HashMap<i32, Vec<i64>> = HashMap::new();
    for (link_id, day, clicks) in rows {
        let counts = series.entry(link_id).or_insert_with(|| vec![0; days.len()]);
        if let Some(index) = day.and_then(|day| days.iter().position(|d| *d == day)) {
            counts[index] = clicks;
        }
    }
    series
}

/// Get sparkline data (last 7 UTC days) for multiple links
#[utoipa::path(
    get,
    path = "/links/sparklines",
    params(
        ("ids" = String, Query, description = "Comma-separated link IDs (at most 500). Links the caller cannot read are omitted.")
    ),
    responses(
        (status = 200, description = "Sparkline data", body = SparklineResponse),
        (status = 400, description = "Too many ids"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Database error"),
    ),
    tag = "Links",
    security(("bearer_auth" = []), ("api_key" = []))
)]
pub async fn get_sparklines(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Unauthorized"})),
            )
                .into_response();
        }
    };

    // Parse link IDs
    let ids_str = params.get("ids").cloned().unwrap_or_default();
    let link_ids: Vec<i32> = ids_str
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    if link_ids.len() > MAX_SPARKLINE_IDS {
        return json_error(
            StatusCode::BAD_REQUEST,
            format!("Too many ids in one request (max {MAX_SPARKLINE_IDS})"),
        );
    }
    let link_ids = unique_ids(link_ids);

    if link_ids.is_empty() {
        return (
            StatusCode::OK,
            Json(SparklineResponse { sparklines: vec![] }),
        )
            .into_response();
    }

    // `created_at` is a naive UTC timestamp, so casting it to `date` buckets by
    // UTC calendar day whatever the session time zone.
    let days = sparkline_days(chrono::Utc::now().date_naive());
    let window_start = days[0].and_time(chrono::NaiveTime::MIN);
    let click_day = Expr::col((click_events::Entity, click_events::Column::CreatedAt))
        .cast_as(Alias::new("date"));

    // One row per (link, day with clicks), counted by the database. Joining
    // from `links` keeps a readable link with no clicks in the answer (one row
    // without a day) and drops the ones the caller may not read or that were
    // deleted, in the same statement.
    let rows: Vec<(i32, Option<chrono::NaiveDate>, i64)> = match links::Entity::find()
        .select_only()
        .column(links::Column::Id)
        .column_as(click_day.clone(), "day")
        .column_as(click_events::Column::Id.count(), "clicks")
        .join(
            JoinType::LeftJoin,
            links::Relation::ClickEvents
                .def()
                .on_condition(move |_links, clicks| {
                    Condition::all()
                        .add(Expr::col((clicks, click_events::Column::CreatedAt)).gte(window_start))
                }),
        )
        .filter(links::Column::Id.is_in(link_ids.clone()))
        .filter(readable_links(user_id))
        .filter(links::Column::DeletedAt.is_null())
        .group_by(links::Column::Id)
        .group_by(click_day)
        .into_tuple()
        .all(&state.db)
        .await
    {
        Ok(rows) => rows,
        Err(error) => {
            tracing::error!("Failed to load sparklines: {}", error);
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Database error");
        }
    };

    let mut series = sparkline_series(&days, rows);
    let labels: Vec<String> = days.iter().map(|d| d.format("%m/%d").to_string()).collect();

    // Answer in the order asked, which is also what the client batches by.
    let sparklines: Vec<SparklineData> = link_ids
        .into_iter()
        .filter_map(|link_id| {
            let data = series.remove(&link_id)?;
            let total = data.iter().sum();
            Some(SparklineData {
                link_id,
                data,
                labels: labels.clone(),
                total,
            })
        })
        .collect();

    (StatusCode::OK, Json(SparklineResponse { sparklines })).into_response()
}

#[cfg(test)]
mod sparkline_tests {
    use super::{sparkline_days, sparkline_series};
    use chrono::NaiveDate;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn window_is_seven_utc_days_ending_today() {
        let days = sparkline_days(date(2028, 3, 2));
        assert_eq!(days.first(), Some(&date(2028, 2, 25)));
        assert_eq!(days.last(), Some(&date(2028, 3, 2)));
        assert_eq!(days.len(), 7);
        assert!(days.contains(&date(2028, 2, 29)), "leap day is a bucket");
    }

    #[test]
    fn series_are_zero_filled_and_ignore_days_outside_the_window() {
        let days = sparkline_days(date(2026, 10, 10));
        let series = sparkline_series(
            &days,
            vec![
                (1, Some(date(2026, 10, 4)), 3),
                (1, Some(date(2026, 10, 10)), 5),
                // Outside the window (before it, and after today): not counted.
                (1, Some(date(2026, 10, 3)), 100),
                (1, Some(date(2026, 10, 11)), 100),
                // A link with no clicks arrives as a row without a day.
                (2, None, 0),
            ],
        );
        assert_eq!(series[&1], vec![3, 0, 0, 0, 0, 0, 5]);
        assert_eq!(series[&2], vec![0; 7]);
        assert_eq!(series.len(), 2);
    }
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

#[derive(Deserialize, ToSchema, utoipa::IntoParams)]
pub struct AvatarProxyQuery {
    /// External http(s) avatar URL to fetch through this origin.
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
///
/// So we allow only inert raster types and return a fixed canonical string for
/// each (never the raw upstream header). SVG is deliberately excluded. The
/// handler additionally sends `X-Content-Type-Options: nosniff` and a locked-down
/// CSP so even a mislabeled body cannot execute.
fn canonical_avatar_content_type(raw: &str) -> Option<&'static str> {
    // Strip any `; charset=…` parameter and normalize case/whitespace.
    let base = raw
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
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
///
/// The URL must also be the `avatar_url` of a non-deleted, non-disabled user
/// whose bio is public. Otherwise this route is an open fetch.
#[utoipa::path(
    get,
    path = "/api/bio/avatar",
    params(AvatarProxyQuery),
    responses(
        (status = 200, description = "Proxied raster avatar bytes", content_type = "image/png"),
        (status = 400, description = "Invalid avatar URL"),
        (status = 404, description = "URL is not the avatar of a public bio"),
        (status = 415, description = "Avatar is not a supported image type"),
        (status = 502, description = "Could not fetch avatar"),
    ),
    tag = "Bio"
)]
pub async fn proxy_bio_avatar(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<AvatarProxyQuery>,
) -> axum::response::Response {
    use axum::http::header;
    const MAX_AVATAR_BYTES: usize = 2 * 1024 * 1024;

    if validate_url(&query.url).is_err() {
        return (StatusCode::BAD_REQUEST, "Invalid avatar URL").into_response();
    }

    if !crate::handlers::bio::link_in_bio_enabled() {
        return (StatusCode::NOT_FOUND, "Avatar not found").into_response();
    }

    // Exact column match: the bio page requests the URL it just received.
    let allowed = users::Entity::find()
        .filter(users::Column::DeletedAt.is_null())
        .filter(users::Column::DisabledAt.is_null())
        .filter(users::Column::BioEnabled.eq(true))
        .filter(users::Column::AvatarUrl.eq(&query.url))
        .limit(1)
        .one(&state.db)
        .await;
    match allowed {
        Ok(Some(_)) => {}
        Ok(None) => return (StatusCode::NOT_FOUND, "Avatar not found").into_response(),
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Could not check avatar").into_response();
        }
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
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Avatar is not a supported image type",
        )
            .into_response();
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
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; sandbox".to_string(),
            ),
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
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Links",
    security(("bearer_auth" = []), ("api_key" = []))
)]
pub async fn get_link_preview_metadata(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<PreviewMetadataRequest>,
) -> impl IntoResponse {
    // Require authentication: this performs a server-side fetch of a user-supplied URL.
    if get_user_id_from_header(&state.db, &headers).await.is_none() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Unauthorized"})),
        )
            .into_response();
    }

    // Validate URL
    let parsed = match url::Url::parse(&payload.url) {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid URL"})),
            )
                .into_response();
        }
    };

    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Only HTTP/HTTPS URLs supported"})),
        )
            .into_response();
    }

    // Same policy as create / health-check / avatar proxy: raw IPs, internal
    // hostnames, dangerous extensions, and length limits must apply before any
    // server-side fetch.
    if let Err(msg) = validate_url(&payload.url) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": msg})),
        )
            .into_response();
    }

    // SSRF-guarded GET: the host and every redirect hop are validated against
    // private/internal ranges before any request is sent.
    let response = match ssrf_guarded_fetch(
        reqwest::Method::GET,
        &payload.url,
        Some("Mozilla/5.0 (compatible; OPN.ONL LinkPreview/1.0)"),
    )
    .await
    {
        Ok(r) => r,
        Err(_) => {
            return (
                StatusCode::OK,
                Json(LinkPreviewData {
                    url: payload.url.clone(),
                    title: None,
                    description: None,
                    image: None,
                    site_name: None,
                    favicon: None,
                }),
            )
                .into_response();
        }
    };

    if !response.status().is_success() {
        return (
            StatusCode::OK,
            Json(LinkPreviewData {
                url: payload.url.clone(),
                title: None,
                description: None,
                image: None,
                site_name: None,
                favicon: None,
            }),
        )
            .into_response();
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
        .or_else(|| {
            Some(format!(
                "{}://{}/favicon.ico",
                parsed.scheme(),
                parsed.host_str().unwrap_or("")
            ))
        });

    (
        StatusCode::OK,
        Json(LinkPreviewData {
            url: payload.url,
            title,
            description,
            image,
            site_name,
            favicon,
        }),
    )
        .into_response()
}

// Helper functions for HTML parsing
fn extract_meta_content(html: &str, property: &str) -> Option<String> {
    // Try property attribute (og: tags)
    let property_pattern = format!(
        r#"<meta[^>]*property=["']{}["'][^>]*content=["']([^"']+)["']"#,
        regex::escape(property)
    );
    if let Ok(re) = regex::Regex::new(&property_pattern)
        && let Some(caps) = re.captures(html)
    {
        return caps.get(1).map(|m| html_decode(m.as_str()));
    }

    // Try content before property
    let property_pattern2 = format!(
        r#"<meta[^>]*content=["']([^"']+)["'][^>]*property=["']{}["']"#,
        regex::escape(property)
    );
    if let Ok(re) = regex::Regex::new(&property_pattern2)
        && let Some(caps) = re.captures(html)
    {
        return caps.get(1).map(|m| html_decode(m.as_str()));
    }

    // Try name attribute (description, etc.)
    let name_pattern = format!(
        r#"<meta[^>]*name=["']{}["'][^>]*content=["']([^"']+)["']"#,
        regex::escape(property)
    );
    if let Ok(re) = regex::Regex::new(&name_pattern)
        && let Some(caps) = re.captures(html)
    {
        return caps.get(1).map(|m| html_decode(m.as_str()));
    }

    // Try content before name
    let name_pattern2 = format!(
        r#"<meta[^>]*content=["']([^"']+)["'][^>]*name=["']{}["']"#,
        regex::escape(property)
    );
    if let Ok(re) = regex::Regex::new(&name_pattern2)
        && let Some(caps) = re.captures(html)
    {
        return caps.get(1).map(|m| html_decode(m.as_str()));
    }

    None
}

fn extract_title_tag(html: &str) -> Option<String> {
    let re = regex::Regex::new(r"<title[^>]*>([^<]+)</title>").ok()?;
    re.captures(html)
        .and_then(|caps| caps.get(1).map(|m| html_decode(m.as_str().trim())))
}

fn extract_favicon(html: &str) -> Option<String> {
    // Try various favicon link patterns
    let patterns = [
        r#"<link[^>]*rel=["'](?:shortcut )?icon["'][^>]*href=["']([^"']+)["']"#,
        r#"<link[^>]*href=["']([^"']+)["'][^>]*rel=["'](?:shortcut )?icon["']"#,
        r#"<link[^>]*rel=["']apple-touch-icon["'][^>]*href=["']([^"']+)["']"#,
    ];

    for pattern in patterns {
        if let Ok(re) = regex::Regex::new(pattern)
            && let Some(caps) = re.captures(html)
        {
            return caps.get(1).map(|m| m.as_str().to_string());
        }
    }
    None
}

fn resolve_url(base: &str, relative: &str) -> String {
    if relative.starts_with("http://") || relative.starts_with("https://") {
        return relative.to_string();
    }

    if let Ok(base_url) = url::Url::parse(base)
        && let Ok(resolved) = base_url.join(relative)
    {
        return resolved.to_string();
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

#[cfg(test)]
mod code_allocation_tests {
    use super::*;

    #[test]
    fn code_availability_fails_closed() {
        assert_eq!(code_availability::<&str>(Ok(false), Ok(false)), Ok(true));
        assert_eq!(code_availability::<&str>(Ok(true), Ok(false)), Ok(false));
        assert_eq!(code_availability::<&str>(Ok(false), Ok(true)), Ok(false));
        // Taken wins over the other lookup failing.
        assert_eq!(code_availability(Ok(true), Err("db")), Ok(false));
        assert_eq!(code_availability(Err("db"), Ok(true)), Ok(false));
        // Unknown is not free. The later error is the one reported.
        assert_eq!(code_availability(Ok(false), Err("db")), Err("db"));
        assert_eq!(code_availability(Err("db"), Ok(false)), Err("db"));
        assert_eq!(
            code_availability(Err("links"), Err("retired")),
            Err("retired")
        );
    }

    #[tokio::test]
    async fn a_failed_lookup_is_retried_and_then_refused() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let calls = AtomicUsize::new(0);
        let result = allocate_code(
            || "abc".to_string(),
            async |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                Err::<bool, _>("down")
            },
        )
        .await;
        assert_eq!(result, Err(AllocateError::Lookup("down")));
        assert_eq!(calls.load(Ordering::SeqCst), CODE_ATTEMPTS);
    }

    #[tokio::test]
    async fn a_failed_lookup_does_not_hand_out_that_candidate() {
        let mut answers = [Ok(false), Err("down"), Ok(true)].into_iter();
        let mut codes = ["taken", "err", "free"].into_iter();
        let got = allocate_code(
            || codes.next().unwrap().to_string(),
            async |_| answers.next().unwrap(),
        )
        .await
        .expect("the third candidate is free");
        assert_eq!(got, "free");
    }

    #[tokio::test]
    async fn every_candidate_taken_is_exhausted_not_a_lookup_error() {
        let result = allocate_code(|| "taken".to_string(), async |_| Ok::<bool, &str>(false)).await;
        assert_eq!(result, Err(AllocateError::Exhausted));
    }
}
