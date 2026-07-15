use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{HeaderMap, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use dashmap::DashMap;
use parking_lot::Mutex;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

/// Rate limiter configuration
#[derive(Clone, Debug)]
pub struct RateLimitConfig {
    /// Maximum requests allowed in the window
    pub max_requests: u32,
    /// Time window duration
    pub window_duration: Duration,
}

impl RateLimitConfig {
    pub fn new(max_requests: u32, window_seconds: u64) -> Self {
        Self {
            max_requests,
            window_duration: Duration::from_secs(window_seconds),
        }
    }
}

/// Rate limiter entry for tracking requests
#[derive(Debug)]
struct RateLimitEntry {
    count: u32,
    window_start: Instant,
}

/// In-memory rate limiter state
#[derive(Debug)]
pub struct RateLimiter {
    entries: DashMap<String, Mutex<RateLimitEntry>>,
    config: RateLimitConfig,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            entries: DashMap::new(),
            config,
        }
    }

    /// Check if a request is allowed and increment counter
    pub fn check(&self, key: &str) -> RateLimitResult {
        let now = Instant::now();

        // Get or create entry
        let entry = self.entries.entry(key.to_string()).or_insert_with(|| {
            Mutex::new(RateLimitEntry {
                count: 0,
                window_start: now,
            })
        });

        let mut entry = entry.lock();

        // Reset window if expired
        if now.duration_since(entry.window_start) >= self.config.window_duration {
            entry.count = 0;
            entry.window_start = now;
        }

        // Check limit
        if entry.count >= self.config.max_requests {
            let retry_after = self
                .config
                .window_duration
                .checked_sub(now.duration_since(entry.window_start))
                .unwrap_or(Duration::ZERO);

            return RateLimitResult::Limited {
                retry_after_secs: retry_after.as_secs(),
                limit: self.config.max_requests,
                remaining: 0,
            };
        }

        // Increment and allow
        entry.count += 1;
        let remaining = self.config.max_requests.saturating_sub(entry.count);

        RateLimitResult::Allowed {
            limit: self.config.max_requests,
            remaining,
        }
    }

    /// Clean up old entries periodically
    pub fn cleanup(&self) {
        let now = Instant::now();
        self.entries.retain(|_, v| {
            let entry = v.lock();
            now.duration_since(entry.window_start) < self.config.window_duration * 2
        });
    }
}

/// Result of rate limit check
#[derive(Debug)]
pub enum RateLimitResult {
    Allowed {
        limit: u32,
        remaining: u32,
    },
    Limited {
        retry_after_secs: u64,
        limit: u32,
        remaining: u32,
    },
}

/// Different rate limit tiers
#[derive(Clone)]
pub struct RateLimiters {
    /// Strict per-IP rate limiter (1 request per second)
    pub per_second: Arc<RateLimiter>,
    /// General API rate limiter (100 requests per minute)
    pub general: Arc<RateLimiter>,
    /// Link creation rate limiter (50 per hour)
    pub link_creation: Arc<RateLimiter>,
    /// Authentication rate limiter (10 per minute)
    pub auth: Arc<RateLimiter>,
    /// Link redirect rate limiter (100 per second per IP - more relaxed for redirects)
    pub redirect: Arc<RateLimiter>,
    /// Password verification rate limiter (5 per minute per IP+code - anti-bruteforce)
    pub password_verify: Arc<RateLimiter>,
    /// Password verification CPU budget shared across every code for one IP.
    /// Prevents bypassing the bcrypt limit by rotating through many short codes.
    pub password_verify_ip: Arc<RateLimiter>,
    /// Contact form limiter (a few per hour per IP). The contact endpoint sends
    /// email, so it must be strict regardless of the general API tier.
    pub contact: Arc<RateLimiter>,
}

impl Default for RateLimiters {
    fn default() -> Self {
        Self {
            // Increased from 1/sec to 10/sec for better UX
            per_second: Arc::new(RateLimiter::new(RateLimitConfig::new(10, 1))),
            general: Arc::new(RateLimiter::new(RateLimitConfig::new(100, 60))),
            // Increased from 50/hour to 100/hour for link creation
            link_creation: Arc::new(RateLimiter::new(RateLimitConfig::new(100, 3600))),
            auth: Arc::new(RateLimiter::new(RateLimitConfig::new(10, 60))),
            redirect: Arc::new(RateLimiter::new(RateLimitConfig::new(100, 1))),
            // Anti-bruteforce: only 5 password attempts per minute per IP+code
            password_verify: Arc::new(RateLimiter::new(RateLimitConfig::new(5, 60))),
            // Bound total bcrypt work even when an attacker rotates link codes.
            password_verify_ip: Arc::new(RateLimiter::new(RateLimitConfig::new(20, 60))),
            // Contact form sends email: cap at 10 per hour per IP.
            contact: Arc::new(RateLimiter::new(RateLimitConfig::new(10, 3600))),
        }
    }
}

impl RateLimiters {
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawn background cleanup task
    pub fn spawn_cleanup_task(limiters: Arc<RateLimiters>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
            loop {
                interval.tick().await;
                limiters.per_second.cleanup();
                limiters.general.cleanup();
                limiters.link_creation.cleanup();
                limiters.auth.cleanup();
                limiters.redirect.cleanup();
                limiters.password_verify.cleanup();
                limiters.password_verify_ip.cleanup();
                limiters.contact.cleanup();
                tracing::debug!("Rate limiter cleanup completed");
            }
        });
    }
}

/// How the client IP is derived when the app runs behind trusted proxies.
///
/// Read once from the environment:
/// - `TRUST_PROXY_HEADERS` — master switch (default `false`). When off, only
///   the socket peer address is used and every forwarding header is ignored.
///   Note: behind a proxy that means all traffic shares the proxy's bucket,
///   so proxied deployments must turn it on.
/// - `REAL_IP_HEADER` — name of the header the trusted edge sets to the real
///   client IP. Defaults to `cf-connecting-ip`: Cloudflare sets it
///   authoritatively at the edge (a client cannot forge it through
///   Cloudflare), and it survives both production paths (tunnel -> backend
///   and tunnel -> nginx -> backend) verbatim. Set it to an empty string to
///   disable when the edge is not Cloudflare, or to e.g. `x-real-ip` when a
///   trusted nginx overwrites that header with the real peer address.
/// - `TRUSTED_PROXY_HOPS` — `X-Forwarded-For` fallback used when
///   `REAL_IP_HEADER` is unset or absent: how many trailing XFF entries were
///   appended by additional trusted proxies (default 0 = use the right-most
///   entry, i.e. what the proxy directly in front of the app appended). The
///   left-most entries are client-controlled and are never used.
#[derive(Debug, Clone)]
pub struct ClientIpConfig {
    pub trust_proxy_headers: bool,
    pub real_ip_header: Option<String>,
    pub trusted_proxy_hops: usize,
}

impl ClientIpConfig {
    pub fn from_env() -> Self {
        let trust_proxy_headers = std::env::var("TRUST_PROXY_HEADERS")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);
        let real_ip_header = match std::env::var("REAL_IP_HEADER") {
            Ok(v) => {
                let v = v.trim().to_ascii_lowercase();
                if v.is_empty() {
                    None
                } else {
                    Some(v)
                }
            }
            Err(_) => Some("cf-connecting-ip".to_string()),
        };
        let trusted_proxy_hops = std::env::var("TRUSTED_PROXY_HOPS")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(0);
        Self {
            trust_proxy_headers,
            real_ip_header,
            trusted_proxy_hops,
        }
    }
}

fn client_ip_config() -> &'static ClientIpConfig {
    static CONFIG: OnceLock<ClientIpConfig> = OnceLock::new();
    CONFIG.get_or_init(ClientIpConfig::from_env)
}

/// Parse a single header/XFF token into a canonical IP string.
fn parse_ip(token: &str) -> Option<String> {
    token.trim().parse::<IpAddr>().ok().map(|ip| ip.to_string())
}

/// Resolve the real client IP from forwarding headers per `config`.
///
/// Returns `None` when proxy headers are not trusted or no trustworthy value
/// is present — callers fall back to the socket peer (or record no IP).
pub fn client_ip_with(headers: &HeaderMap, config: &ClientIpConfig) -> Option<String> {
    if !config.trust_proxy_headers {
        return None;
    }

    if let Some(name) = &config.real_ip_header {
        if let Some(ip) = headers
            .get(name.as_str())
            .and_then(|v| v.to_str().ok())
            .and_then(parse_ip)
        {
            return Some(ip);
        }
    }

    // X-Forwarded-For fallback: count from the RIGHT. Each trusted proxy
    // appends the address of the peer that connected to it, so the trailing
    // entries are trustworthy while the leading ones are whatever the client
    // sent. Reading the first token (the historical behavior here) trusts
    // exactly the spoofable part. If the selected token is not a valid IP the
    // whole chain is rejected rather than scanning further left.
    let mut tokens: Vec<&str> = Vec::new();
    for value in headers.get_all("x-forwarded-for") {
        if let Ok(s) = value.to_str() {
            tokens.extend(s.split(','));
        }
    }
    let idx = tokens.len().checked_sub(1 + config.trusted_proxy_hops)?;
    parse_ip(tokens[idx])
}

/// Resolve the real client IP from forwarding headers using the env-derived
/// config. Used for click analytics / geo / conditional routing as well, so
/// the recorded IP obeys the same trust rules as the rate limiter.
pub fn client_ip_from_headers(headers: &HeaderMap) -> Option<String> {
    client_ip_with(headers, client_ip_config())
}

/// Extract the client IP used as the rate-limit key.
///
/// Trusted forwarding headers first (see [`ClientIpConfig`]), then the real
/// socket peer address (requires serving with `ConnectInfo<SocketAddr>`).
pub fn extract_ip(req: &Request<Body>) -> String {
    if let Some(ip) = client_ip_from_headers(req.headers()) {
        return ip;
    }

    if let Some(ConnectInfo(addr)) = req.extensions().get::<ConnectInfo<SocketAddr>>() {
        return addr.ip().to_string();
    }

    "unknown".to_string()
}

/// Known top-level API path segments. Anything whose first segment is one of
/// these is a normal API request, never a short-code redirect. Keep in sync with
/// the routes registered in `build_router`.
const API_PREFIXES: &[&str] = &[
    "auth",
    "links",
    "orgs",
    "folders",
    "tags",
    "analytics",
    "admin",
    "contact",
    "ws",
    "sse",
    "health",
    "api",
    "swagger-ui",
    "api-docs",
];

/// Classify a request path as a short-code redirect vs a normal API call.
///
/// Redirect routes are `/{code}`, `/{code}/preview` and `/{code}/verify` — their
/// first path segment is the code, which is never one of the known API prefixes.
/// This replaces an earlier first-letter/length heuristic that misfiled ~13% of
/// generated codes and, worse, routed `POST /contact` into the relaxed 100 req/s
/// redirect bucket (an email-flood vector).
fn is_redirect_path(path: &str) -> bool {
    match path.trim_start_matches('/').split('/').next() {
        Some(first) => !first.is_empty() && !API_PREFIXES.contains(&first),
        None => false,
    }
}

/// Rate limit middleware for general API endpoints
pub async fn rate_limit_middleware(
    State(limiters): State<Arc<RateLimiters>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let ip = extract_ip(&req);
    let path = req.uri().path();

    // Redirects are high-volume and skip the strict per-second gate; everything
    // else (including /contact) is classified by its route, not a path heuristic.
    let is_redirect = is_redirect_path(path);

    if !is_redirect {
        if let RateLimitResult::Limited {
            retry_after_secs,
            limit,
            remaining,
        } = limiters.per_second.check(&format!("sec:{}", ip))
        {
            let mut response = (
                StatusCode::TOO_MANY_REQUESTS,
                serde_json::json!({
                    "error": "Too many requests",
                    "retry_after": retry_after_secs,
                    "message": format!("Rate limit: maximum {} requests per second", limit)
                })
                .to_string(),
            )
                .into_response();

            let headers = response.headers_mut();
            headers.insert("X-RateLimit-Limit", limit.to_string().parse().unwrap());
            headers.insert(
                "X-RateLimit-Remaining",
                remaining.to_string().parse().unwrap(),
            );
            headers.insert("Retry-After", retry_after_secs.to_string().parse().unwrap());
            headers.insert("Content-Type", "application/json".parse().unwrap());

            return response;
        }
    }

    // Choose appropriate limiter based on path
    let result = if path.ends_with("/verify") && req.method() == axum::http::Method::POST {
        // Enforce both a per-code guessing budget and a total per-IP bcrypt
        // budget. Without the latter, rotating codes creates unlimited buckets.
        match limiters
            .password_verify_ip
            .check(&format!("pwverify-ip:{}", ip))
        {
            limited @ RateLimitResult::Limited { .. } => limited,
            RateLimitResult::Allowed { .. } => {
                let code = path.split('/').find(|s| !s.is_empty()).unwrap_or("unknown");
                limiters
                    .password_verify
                    .check(&format!("pwverify:{}:{}", ip, code))
            }
        }
    } else if path.starts_with("/auth") {
        limiters.auth.check(&format!("auth:{}", ip))
    } else if path.starts_with("/links") && req.method() == axum::http::Method::POST {
        limiters.link_creation.check(&format!("create:{}", ip))
    } else if path.starts_with("/contact") && req.method() == axum::http::Method::POST {
        limiters.contact.check(&format!("contact:{}", ip))
    } else if is_redirect {
        // Short code redirect - more relaxed
        limiters.redirect.check(&format!("redirect:{}", ip))
    } else {
        limiters.general.check(&format!("general:{}", ip))
    };

    match result {
        RateLimitResult::Allowed { limit, remaining } => {
            let mut response = next.run(req).await;
            let headers = response.headers_mut();
            headers.insert("X-RateLimit-Limit", limit.to_string().parse().unwrap());
            headers.insert(
                "X-RateLimit-Remaining",
                remaining.to_string().parse().unwrap(),
            );
            response
        }
        RateLimitResult::Limited {
            retry_after_secs,
            limit,
            remaining,
        } => {
            let mut response = (
                StatusCode::TOO_MANY_REQUESTS,
                serde_json::json!({
                    "error": "Too many requests",
                    "retry_after": retry_after_secs,
                    "message": format!("Rate limit exceeded. Please try again in {} seconds.", retry_after_secs)
                }).to_string(),
            ).into_response();

            let headers = response.headers_mut();
            headers.insert("X-RateLimit-Limit", limit.to_string().parse().unwrap());
            headers.insert(
                "X-RateLimit-Remaining",
                remaining.to_string().parse().unwrap(),
            );
            headers.insert("Retry-After", retry_after_secs.to_string().parse().unwrap());
            headers.insert("Content-Type", "application/json".parse().unwrap());

            response
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirect_classifier_separates_codes_from_api_routes() {
        // Short-code redirect routes: first segment is the code.
        assert!(is_redirect_path("/abc123"));
        assert!(is_redirect_path("/abc123/preview"));
        assert!(is_redirect_path("/abc123/verify"));
        // Regression: codes starting with a/l/o/f/t/w/s/h and long custom codes
        // were misclassified by the old first-letter/length heuristic.
        assert!(is_redirect_path("/link42"));
        assert!(is_redirect_path("/awesome-promo-code-2026"));
        // API routes must never be treated as redirects — /contact especially,
        // since the relaxed redirect bucket was an email-flood vector.
        assert!(!is_redirect_path("/contact"));
        assert!(!is_redirect_path("/auth/login"));
        assert!(!is_redirect_path("/links"));
        assert!(!is_redirect_path("/links/bulk"));
        assert!(!is_redirect_path("/admin/stats"));
        assert!(!is_redirect_path("/api/bio/someone"));
        assert!(!is_redirect_path("/health"));
        // Root / empty is not a redirect.
        assert!(!is_redirect_path("/"));
    }

    #[test]
    fn test_rate_limiter_allows_within_limit() {
        let limiter = RateLimiter::new(RateLimitConfig::new(3, 60));

        assert!(matches!(
            limiter.check("test"),
            RateLimitResult::Allowed { remaining: 2, .. }
        ));
        assert!(matches!(
            limiter.check("test"),
            RateLimitResult::Allowed { remaining: 1, .. }
        ));
        assert!(matches!(
            limiter.check("test"),
            RateLimitResult::Allowed { remaining: 0, .. }
        ));
    }

    #[test]
    fn test_rate_limiter_blocks_over_limit() {
        let limiter = RateLimiter::new(RateLimitConfig::new(2, 60));

        limiter.check("test");
        limiter.check("test");

        assert!(matches!(
            limiter.check("test"),
            RateLimitResult::Limited { .. }
        ));
    }

    #[test]
    fn test_rate_limiter_separate_keys() {
        let limiter = RateLimiter::new(RateLimitConfig::new(1, 60));

        assert!(matches!(
            limiter.check("user1"),
            RateLimitResult::Allowed { .. }
        ));
        assert!(matches!(
            limiter.check("user2"),
            RateLimitResult::Allowed { .. }
        ));
        assert!(matches!(
            limiter.check("user1"),
            RateLimitResult::Limited { .. }
        ));
    }

    #[tokio::test]
    async fn password_ip_budget_cannot_be_bypassed_by_rotating_codes() {
        use axum::{middleware, routing::post, Router};

        let limiters = Arc::new(RateLimiters {
            per_second: Arc::new(RateLimiter::new(RateLimitConfig::new(10_000, 1))),
            general: Arc::new(RateLimiter::new(RateLimitConfig::new(10_000, 60))),
            link_creation: Arc::new(RateLimiter::new(RateLimitConfig::new(10_000, 3600))),
            auth: Arc::new(RateLimiter::new(RateLimitConfig::new(10_000, 60))),
            redirect: Arc::new(RateLimiter::new(RateLimitConfig::new(10_000, 1))),
            password_verify: Arc::new(RateLimiter::new(RateLimitConfig::new(100, 60))),
            password_verify_ip: Arc::new(RateLimiter::new(RateLimitConfig::new(2, 60))),
            contact: Arc::new(RateLimiter::new(RateLimitConfig::new(10_000, 3600))),
        });
        let app = Router::new()
            .route("/:code/verify", post(|| async { "ok" }))
            .layer(middleware::from_fn_with_state(
                limiters,
                rate_limit_middleware,
            ));
        let server = axum_test::TestServer::new(app).unwrap();

        assert_eq!(
            server.post("/first/verify").await.status_code(),
            StatusCode::OK
        );
        assert_eq!(
            server.post("/second/verify").await.status_code(),
            StatusCode::OK
        );
        assert_eq!(
            server.post("/third/verify").await.status_code(),
            StatusCode::TOO_MANY_REQUESTS,
            "different codes must still consume one shared per-IP bcrypt budget"
        );
    }

    mod client_ip_resolution {
        use super::*;
        use axum::http::{HeaderMap, HeaderName, HeaderValue};

        fn cfg(trust: bool, header: Option<&str>, hops: usize) -> ClientIpConfig {
            ClientIpConfig {
                trust_proxy_headers: trust,
                real_ip_header: header.map(str::to_string),
                trusted_proxy_hops: hops,
            }
        }

        fn headers_of(pairs: &[(&str, &str)]) -> HeaderMap {
            let mut map = HeaderMap::new();
            for (name, value) in pairs {
                map.append(
                    name.parse::<HeaderName>().unwrap(),
                    HeaderValue::from_str(value).unwrap(),
                );
            }
            map
        }

        #[test]
        fn trust_off_ignores_all_headers() {
            let headers = headers_of(&[
                ("cf-connecting-ip", "203.0.113.7"),
                ("x-forwarded-for", "203.0.113.7"),
                ("x-real-ip", "203.0.113.7"),
            ]);
            assert_eq!(
                client_ip_with(&headers, &cfg(false, Some("cf-connecting-ip"), 0)),
                None
            );
        }

        #[test]
        fn real_ip_header_wins_over_forged_xff() {
            let headers = headers_of(&[
                ("cf-connecting-ip", "203.0.113.7"),
                ("x-forwarded-for", "6.6.6.6, 203.0.113.7"),
            ]);
            assert_eq!(
                client_ip_with(&headers, &cfg(true, Some("cf-connecting-ip"), 0)),
                Some("203.0.113.7".to_string())
            );
        }

        #[test]
        fn xff_fallback_uses_rightmost_not_first() {
            let headers = headers_of(&[("x-forwarded-for", "6.6.6.6, 203.0.113.7")]);
            assert_eq!(
                client_ip_with(&headers, &cfg(true, Some("cf-connecting-ip"), 0)),
                Some("203.0.113.7".to_string())
            );
        }

        #[test]
        fn trusted_proxy_hops_skip_trailing_proxy_entries() {
            let headers = headers_of(&[("x-forwarded-for", "6.6.6.6, 203.0.113.7, 10.0.0.2")]);
            assert_eq!(
                client_ip_with(&headers, &cfg(true, None, 1)),
                Some("203.0.113.7".to_string())
            );
        }

        #[test]
        fn hops_beyond_chain_yield_none() {
            let headers = headers_of(&[("x-forwarded-for", "203.0.113.7")]);
            assert_eq!(client_ip_with(&headers, &cfg(true, None, 5)), None);
        }

        #[test]
        fn x_real_ip_is_not_implicitly_trusted() {
            let headers = headers_of(&[("x-real-ip", "203.0.113.7")]);
            assert_eq!(
                client_ip_with(&headers, &cfg(true, Some("cf-connecting-ip"), 0)),
                None
            );
        }

        #[test]
        fn real_ip_header_is_configurable() {
            let headers = headers_of(&[("x-real-ip", "203.0.113.7")]);
            assert_eq!(
                client_ip_with(&headers, &cfg(true, Some("x-real-ip"), 0)),
                Some("203.0.113.7".to_string())
            );
        }

        #[test]
        fn malformed_values_resolve_to_none_without_panicking() {
            for value in ["", "not-an-ip", "  ,  ,", "999.999.999.999", "1.2.3.4;evil"] {
                let headers =
                    headers_of(&[("cf-connecting-ip", value), ("x-forwarded-for", value)]);
                assert_eq!(
                    client_ip_with(&headers, &cfg(true, Some("cf-connecting-ip"), 0)),
                    None,
                    "value {value:?} must be rejected"
                );
            }

            // Non-UTF-8 header bytes must be skipped, not panic.
            let mut headers = HeaderMap::new();
            headers.append(
                "x-forwarded-for",
                HeaderValue::from_bytes(&[0xff, 0xfe]).unwrap(),
            );
            assert_eq!(client_ip_with(&headers, &cfg(true, None, 0)), None);
        }

        #[test]
        fn multiple_xff_header_lines_use_the_last_appended_token() {
            // Attacker sends their own XFF line; the trusted proxy appends a
            // second one. Wire order is preserved by HeaderMap::append.
            let headers = headers_of(&[
                ("x-forwarded-for", "6.6.6.6, 7.7.7.7"),
                ("x-forwarded-for", "203.0.113.7"),
            ]);
            assert_eq!(
                client_ip_with(&headers, &cfg(true, None, 0)),
                Some("203.0.113.7".to_string())
            );
        }

        #[test]
        fn ipv6_is_canonicalized_for_stable_bucket_keys() {
            let headers = headers_of(&[("cf-connecting-ip", "2001:DB8:0:0:0:0:0:1")]);
            assert_eq!(
                client_ip_with(&headers, &cfg(true, Some("cf-connecting-ip"), 0)),
                Some("2001:db8::1".to_string())
            );
        }
    }

    /// Black-box regression tests for the client identity used by the rate
    /// limiter behind the production proxy chain (Cloudflare -> nginx ->
    /// backend, or Cloudflare tunnel -> backend directly for l.opn.onl).
    ///
    /// The tests simulate what the trusted edge actually delivers: Cloudflare
    /// sets `CF-Connecting-IP` authoritatively and *appends* the real client
    /// IP to any `X-Forwarded-For` the client sent. Everything to the left of
    /// that appended token — and any extra headers like `X-Real-IP` on the
    /// direct tunnel path — is attacker-controlled.
    mod client_identity {
        use super::*;
        use axum::{middleware, routing::post, Router};

        async fn spawn_test_server() -> SocketAddr {
            // Matches the production deployment: proxy headers are trusted.
            // REAL_IP_HEADER is left unset so the cf-connecting-ip default
            // applies.
            std::env::set_var("TRUST_PROXY_HEADERS", "true");

            let limiters = Arc::new(RateLimiters {
                per_second: Arc::new(RateLimiter::new(RateLimitConfig::new(10_000, 1))),
                general: Arc::new(RateLimiter::new(RateLimitConfig::new(10_000, 60))),
                link_creation: Arc::new(RateLimiter::new(RateLimitConfig::new(10_000, 3600))),
                // Tight budget so the tests exhaust it quickly: 2 per minute.
                auth: Arc::new(RateLimiter::new(RateLimitConfig::new(2, 60))),
                redirect: Arc::new(RateLimiter::new(RateLimitConfig::new(10_000, 1))),
                password_verify: Arc::new(RateLimiter::new(RateLimitConfig::new(10_000, 60))),
                password_verify_ip: Arc::new(RateLimiter::new(RateLimitConfig::new(10_000, 60))),
                contact: Arc::new(RateLimiter::new(RateLimitConfig::new(10_000, 3600))),
            });

            let app = Router::new()
                .route("/auth/login", post(|| async { "ok" }))
                .layer(middleware::from_fn_with_state(
                    limiters,
                    rate_limit_middleware,
                ));

            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move {
                axum::serve(
                    listener,
                    app.into_make_service_with_connect_info::<SocketAddr>(),
                )
                .await
                .unwrap();
            });
            addr
        }

        #[tokio::test]
        async fn spoofed_forwarding_headers_do_not_change_bucket() {
            let addr = spawn_test_server().await;
            let client = reqwest::Client::new();

            // One real client (203.0.113.7) sends a different forged XFF
            // prefix and X-Real-IP on every request. All four requests must
            // land in the same bucket: 2 allowed, then 429.
            for i in 0..4u16 {
                let forged = format!("{}.{}.{}.{}", i + 1, i + 2, i + 3, i + 4);
                let res = client
                    .post(format!("http://{addr}/auth/login"))
                    .header("cf-connecting-ip", "203.0.113.7")
                    .header("x-forwarded-for", format!("{forged}, 203.0.113.7"))
                    .header("x-real-ip", forged.clone())
                    .send()
                    .await
                    .unwrap();
                if i < 2 {
                    assert_eq!(res.status().as_u16(), 200, "request {i} should be allowed");
                } else {
                    assert_eq!(
                        res.status().as_u16(),
                        429,
                        "request {i}: a forged X-Forwarded-For/X-Real-IP must not move the client to a fresh bucket"
                    );
                }
            }
        }

        #[tokio::test]
        async fn distinct_clients_get_distinct_buckets() {
            let addr = spawn_test_server().await;
            let client = reqwest::Client::new();

            // Two different real clients behind the same proxy chain. Both
            // happen to send the same (forged) XFF prefix; they must still
            // get independent budgets.
            for real in ["203.0.113.10", "203.0.113.11"] {
                for i in 0..2 {
                    let res = client
                        .post(format!("http://{addr}/auth/login"))
                        .header("cf-connecting-ip", real)
                        .header("x-forwarded-for", format!("9.9.9.9, {real}"))
                        .send()
                        .await
                        .unwrap();
                    assert_eq!(
                        res.status().as_u16(),
                        200,
                        "client {real} request {i} must not share a bucket with the other client"
                    );
                }
            }

            // The first client's budget is genuinely spent.
            let res = client
                .post(format!("http://{addr}/auth/login"))
                .header("cf-connecting-ip", "203.0.113.10")
                .header("x-forwarded-for", "9.9.9.9, 203.0.113.10")
                .send()
                .await
                .unwrap();
            assert_eq!(res.status().as_u16(), 429);
        }
    }
}
