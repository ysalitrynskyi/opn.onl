mod common;

use serde_json::json;

#[cfg(test)]
mod tests {
    use super::*;

    // Real-router check (replaces the old stub that only hit a fake /health):
    // listing links requires authentication.
    #[tokio::test]
    async fn test_links_endpoint_requires_auth() {
        let (server, _db) = common::spawn_real_app().await;

        let response = server.get("/links").await;
        assert_eq!(
            response.status_code(),
            401,
            "unauthenticated /links must be rejected: {}",
            response.text()
        );
    }
}
