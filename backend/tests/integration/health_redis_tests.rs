//! `/health` reports Redis by asking it.
//!
//! It used to say `"redis": "connected"` whenever a cache had been built at
//! startup, so a Redis that went away afterwards still showed as connected.
//! It now sends a PING with a short timeout. Redis is optional, so a Redis that
//! does not answer makes the instance `degraded` and never fails the probe;
//! only the database does that.
//!
//! The Redis here is a TCP stand-in, so the tests do not depend on a server
//! being installed and can make it misbehave on purpose.

use crate::common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use opn_onl_backend::utils::cache::RedisCache;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// What the stand-in does when a PING arrives. Every behaviour answers the
/// `CLIENT SETINFO` the client sends on connecting, so the cache can be built.
#[derive(Clone, Copy)]
enum Behaviour {
    /// Answers with `+PONG`.
    Answers,
    /// Reads the PING and never replies.
    Silent,
    /// Closes the connection when the PING arrives.
    HangsUp,
}

/// Start the stand-in on a free port and return its URL. It knows just enough
/// of the protocol for that: one `+OK` per `CLIENT` command, then the PING.
async fn fake_redis(behaviour: Behaviour) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("address").port();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = [0u8; 512];
                while let Ok(read) = socket.read(&mut buf).await {
                    if read == 0 {
                        break;
                    }
                    let commands = String::from_utf8_lossy(&buf[..read]).into_owned();
                    for _ in 0..commands.matches("CLIENT").count() {
                        let _ = socket.write_all(b"+OK\r\n").await;
                    }
                    if commands.contains("PING") {
                        match behaviour {
                            Behaviour::Answers => {
                                let _ = socket.write_all(b"+PONG\r\n").await;
                            }
                            Behaviour::Silent => {}
                            Behaviour::HangsUp => break,
                        }
                    }
                }
            });
        }
    });
    format!("redis://127.0.0.1:{port}")
}

/// `GET /health` on the real router, with `redis` as its cache (if any).
async fn health(redis: Option<RedisCache>) -> (u16, Value) {
    common::set_env("FORCE_HTTPS", "false");
    common::set_env("TRUST_PROXY_HEADERS", "false");
    let db = common::setup_test_db().await;
    let mut state = opn_onl_backend::AppState::for_tests(db).await;
    state.redis_cache = redis.map(Arc::new);
    let server = axum_test::TestServer::new(opn_onl_backend::build_router(state))
        .expect("failed to start test server");
    let res = server.get("/health").await;
    (res.status_code().as_u16(), res.json())
}

async fn cache_on(url: &str) -> RedisCache {
    RedisCache::connect(url, 300)
        .await
        .expect("the stand-in accepts the first connection")
}

#[tokio::test]
async fn a_redis_that_answers_is_connected() {
    let redis = cache_on(&fake_redis(Behaviour::Answers).await).await;

    let (status, body) = health(Some(redis)).await;

    assert_eq!(status, 200, "{body}");
    assert_eq!(body["redis"], "connected", "{body}");
    assert_eq!(body["status"], "healthy", "{body}");
}

#[tokio::test]
async fn a_redis_that_stopped_answering_is_disconnected_and_the_instance_degraded() {
    let redis = cache_on(&fake_redis(Behaviour::Silent).await).await;

    let started = Instant::now();
    let (status, body) = health(Some(redis)).await;

    assert_eq!(
        status, 200,
        "Redis is optional, the probe must not fail: {body}"
    );
    assert_eq!(body["redis"], "disconnected", "{body}");
    assert_eq!(body["status"], "degraded", "{body}");
    assert_eq!(body["database"], "connected", "{body}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the PING is bounded by a short timeout, took {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn a_redis_that_hung_up_is_disconnected_and_the_instance_degraded() {
    let redis = cache_on(&fake_redis(Behaviour::HangsUp).await).await;

    let (status, body) = health(Some(redis)).await;

    assert_eq!(status, 200, "{body}");
    assert_eq!(body["redis"], "disconnected", "{body}");
    assert_eq!(body["status"], "degraded", "{body}");
}

/// Control: no Redis configured is a choice, not a fault.
#[tokio::test]
async fn no_redis_is_disabled_and_the_instance_healthy() {
    let (status, body) = health(None).await;

    assert_eq!(status, 200, "{body}");
    assert_eq!(body["redis"], "disabled", "{body}");
    assert_eq!(body["status"], "healthy", "{body}");
}
