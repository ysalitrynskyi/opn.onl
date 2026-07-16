//! Regression tests for the max_clicks / burn-after-reading overshoot race.
//!
//! `redirect_link` used to enforce the click cap as check-then-act against the
//! in-memory click buffer, so N concurrent requests could all pass the guard
//! before any click was recorded — a "one-time" burn link could be opened by
//! every concurrent request (observed 20/20 redirects pre-fix). The fix makes
//! capped links consume their click with a single atomic conditional UPDATE
//! (`click_count < max_clicks`), so at most `max_clicks` requests can ever win.
//!
//! These are black-box tests against a REAL running backend (the crate has no
//! lib target, so the handler cannot be driven in-process). They are gated on
//! `E2E_BASE_URL` and skip silently when it is unset, e.g.:
//!
//! ```sh
//! E2E_BASE_URL=http://localhost:3105 cargo test --test click_cap_race_tests -- --nocapture
//! ```
//!
//! Requests in a round are released together via a Barrier — sequential curl
//! does NOT reproduce the race; simultaneous arrival does, reliably.

use std::sync::Arc;
use tokio::sync::Barrier;

/// The tests hammer the same backend from one IP; running them concurrently
/// trips the server's per-IP rate limits and pollutes each other's counts.
/// Each test holds this lock for its whole body so they run serially even
/// under cargo's default parallel test runner.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn base_url() -> Option<String> {
    std::env::var("E2E_BASE_URL").ok().filter(|s| !s.is_empty())
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client")
}

async fn create_link(base: &str, body: serde_json::Value) -> String {
    // Retry on 429: the backend's per-IP limiter may still be draining from a
    // previous test's burst; that's environmental, not the behavior under test.
    for _ in 0..10 {
        let resp = client()
            .post(format!("{base}/links"))
            .json(&body)
            .send()
            .await
            .expect("create link request");
        if resp.status() == 429 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            continue;
        }
        assert_eq!(resp.status(), 201, "link creation failed");
        let v: serde_json::Value = resp.json().await.expect("create link json");
        return v["code"].as_str().expect("code").to_string();
    }
    panic!("link creation still rate-limited after 10 retries");
}

/// Fire `n` GET /{code} requests that all start at the same instant.
/// Returns (redirects, gone, other).
async fn slam(base: &str, code: &str, n: usize) -> (usize, usize, usize) {
    let barrier = Arc::new(Barrier::new(n));
    let mut handles = Vec::with_capacity(n);
    for _ in 0..n {
        let barrier = barrier.clone();
        let url = format!("{base}/{code}");
        let client = client();
        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            match client.get(&url).send().await {
                Ok(resp) => resp.status().as_u16(),
                Err(_) => 0,
            }
        }));
    }
    let mut redirects = 0;
    let mut gone = 0;
    let mut other = 0;
    for h in handles {
        match h.await.expect("task") {
            301 | 302 | 307 | 308 => redirects += 1,
            410 => gone += 1,
            _ => other += 1,
        }
    }
    (redirects, gone, other)
}

/// A burn-after-reading link (max_clicks = 1) must serve exactly ONE redirect,
/// no matter how many requests arrive simultaneously. Pre-fix this failed with
/// up to 20 redirects per round.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn burn_link_is_exactly_once_under_concurrency() {
    let Some(base) = base_url() else {
        eprintln!("skipping: E2E_BASE_URL not set (needs a running backend)");
        return;
    };
    let _serial = SERIAL.lock().await;
    const N: usize = 20;
    const ROUNDS: usize = 3;

    for round in 1..=ROUNDS {
        let code = create_link(
            &base,
            serde_json::json!({
                "original_url": "https://example.com/burn-race-secret",
                "burn_after_reading": true,
            }),
        )
        .await;

        let (redirects, gone, other) = slam(&base, &code, N).await;
        println!("round {round}: code={code} redirects={redirects} gone={gone} other={other}");
        assert_eq!(
            redirects, 1,
            "burn link {code} served {redirects} redirects to {N} concurrent requests (round {round}); \
             a one-time link must be opened exactly once"
        );
        // `other` tolerates transport errors / 429s; the invariant is that no
        // loser ever gets the destination.
        assert_eq!(redirects + gone + other, N);
        assert!(gone >= 1, "concurrent losers must get 410 Gone");

        // A follow-up request must also be refused.
        let status = client()
            .get(format!("{base}/{code}"))
            .send()
            .await
            .expect("follow-up")
            .status()
            .as_u16();
        assert_eq!(status, 410, "burned link must stay 410 after the race");

        // Space rounds out so the per-IP redirect rate limit can't interfere.
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

/// A plain max_clicks = N link must serve at most N redirects under
/// concurrency, and the persisted click_count must settle at exactly N after
/// the click-buffer flush window (no double-counting between the atomic
/// consume and the buffer's aggregate flush).
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn capped_link_never_overshoots_and_count_settles_exactly() {
    let Some(base) = base_url() else {
        eprintln!("skipping: E2E_BASE_URL not set (needs a running backend)");
        return;
    };
    let _serial = SERIAL.lock().await;
    const MAX: usize = 3;
    const N: usize = 30;

    let code = create_link(
        &base,
        serde_json::json!({
            "original_url": "https://example.com/capped-race",
            "max_clicks": MAX,
        }),
    )
    .await;

    let (redirects, gone, other) = slam(&base, &code, N).await;
    println!("code={code} redirects={redirects} gone={gone} other={other}");
    assert_eq!(
        redirects, MAX,
        "max_clicks={MAX} link {code} served {redirects} redirects to {N} concurrent requests"
    );

    // Wait past the click-buffer flush interval (CLICK_FLUSH_INTERVAL, default
    // 5s), then check the persisted count through the public preview endpoint.
    // If the atomic consume were also counted in the buffer, this would read
    // 2 * MAX.
    let flush_secs = std::env::var("CLICK_FLUSH_INTERVAL")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(5);
    tokio::time::sleep(std::time::Duration::from_secs(flush_secs + 2)).await;
    let preview: serde_json::Value = client()
        .get(format!("{base}/{code}/preview"))
        .send()
        .await
        .expect("preview")
        .json()
        .await
        .expect("preview json");
    assert_eq!(
        preview["click_count"].as_i64(),
        Some(MAX as i64),
        "persisted click_count must settle at exactly max_clicks (no double count at flush)"
    );
}

/// POST /{code}/verify discloses the destination URL, so it must consume a
/// click slot for capped links exactly like a redirect — including the
/// passwordless form, which previously returned the URL without any counting
/// at all (an unlimited read of a "one-time" secret that never burned it).
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn verify_endpoint_consumes_burn_link_exactly_once() {
    let Some(base) = base_url() else {
        eprintln!("skipping: E2E_BASE_URL not set (needs a running backend)");
        return;
    };
    let _serial = SERIAL.lock().await;
    const N: usize = 10;

    let code = create_link(
        &base,
        serde_json::json!({
            "original_url": "https://example.com/verify-burn-secret",
            "burn_after_reading": true,
        }),
    )
    .await;

    let barrier = Arc::new(Barrier::new(N));
    let mut handles = Vec::with_capacity(N);
    for _ in 0..N {
        let barrier = barrier.clone();
        let url = format!("{base}/{code}/verify");
        let client = client();
        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            match client
                .post(&url)
                .json(&serde_json::json!({ "password": "" }))
                .send()
                .await
            {
                Ok(resp) => resp.status().as_u16(),
                Err(_) => 0,
            }
        }));
    }
    let mut disclosed = 0;
    let mut gone = 0;
    let mut other = 0;
    for h in handles {
        match h.await.expect("task") {
            200 => disclosed += 1,
            410 => gone += 1,
            _ => other += 1,
        }
    }
    println!("code={code} disclosed={disclosed} gone={gone} other={other}");
    assert_eq!(
        disclosed, 1,
        "verify endpoint disclosed burn link {code} to {disclosed} of {N} concurrent callers; \
         a one-time secret must be disclosed exactly once"
    );

    // The link must now be burned for redirects too.
    let status = client()
        .get(format!("{base}/{code}"))
        .send()
        .await
        .expect("follow-up")
        .status()
        .as_u16();
    assert_eq!(
        status, 410,
        "burned link must be 410 after verify consumed it"
    );
}
