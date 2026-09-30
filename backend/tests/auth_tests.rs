mod common;

use serde_json::json;

// Integration tests against the REAL router and a real Postgres database
// (see tests/real_integration_tests.rs for the core flows). These replace the
// old stub tests that only ever queried a fake /health route.
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_validation() {
        let (server, _db) = common::spawn_real_app().await;

        // Invalid email must be rejected by the real handler's validator.
        let response = server
            .post("/auth/register")
            .json(&json!({ "email": "not-an-email", "password": "password123" }))
            .await;
        assert!(
            response.status_code().is_client_error(),
            "invalid email accepted: {}",
            response.text()
        );

        // Too-short password must be rejected.
        let response = server
            .post("/auth/register")
            .json(&json!({ "email": common::unique_email(), "password": "short" }))
            .await;
        assert!(
            response.status_code().is_client_error(),
            "weak password accepted: {}",
            response.text()
        );
    }

    #[tokio::test]
    async fn test_login_requires_credentials() {
        let (server, _db) = common::spawn_real_app().await;

        // Unknown user / wrong credentials must not yield a token.
        let response = server
            .post("/auth/login")
            .json(&json!({ "email": common::unique_email(), "password": "password123" }))
            .await;
        assert!(
            response.status_code().is_client_error(),
            "login without an account succeeded: {}",
            response.text()
        );
    }

    /// The account password minimum is 8 characters: 7 (or none) is refused,
    /// exactly 8, punctuation and a space included, registers and signs in.
    #[tokio::test]
    async fn register_enforces_eight_character_password_minimum() {
        let (server, _db) = common::spawn_real_app().await;

        for too_short in ["", "1234567"] {
            let response = server
                .post("/auth/register")
                .json(&json!({ "email": common::unique_email(), "password": too_short }))
                .await;
            assert_eq!(
                response.status_code(),
                400,
                "{too_short:?} accepted: {}",
                response.text()
            );
            assert!(
                response.text().contains("at least 8 characters"),
                "{too_short:?} must be refused by the length rule: {}",
                response.text()
            );
        }

        let email = common::unique_email();
        let password = "P@s w0rd";
        let response = server
            .post("/auth/register")
            .json(&json!({ "email": email, "password": password }))
            .await;
        assert_eq!(
            response.status_code(),
            201,
            "8-character password refused: {}",
            response.text()
        );
        let login = server
            .post("/auth/login")
            .json(&json!({ "email": email, "password": password }))
            .await;
        assert_eq!(
            login.status_code(),
            200,
            "8-character password must sign in: {}",
            login.text()
        );
    }

    /// Login checks the password against the stored hash: the registered one
    /// (100 characters, non-ASCII) signs in and its token works; the same
    /// password with one letter changed is refused and yields no token.
    #[tokio::test]
    async fn login_accepts_the_registered_password_and_rejects_a_wrong_one() {
        let (server, _db) = common::spawn_real_app().await;
        let email = common::unique_email();
        let password = format!("{}!", "Пароль-🔐-".repeat(11));

        let register = server
            .post("/auth/register")
            .json(&json!({ "email": email, "password": password }))
            .await;
        assert_eq!(register.status_code(), 201, "register: {}", register.text());

        let login = server
            .post("/auth/login")
            .json(&json!({ "email": email, "password": password }))
            .await;
        assert_eq!(
            login.status_code(),
            200,
            "the registered password must sign in: {}",
            login.text()
        );
        let token = login.json::<serde_json::Value>()["token"]
            .as_str()
            .expect("token")
            .to_string();
        let me = server.get("/auth/me").authorization_bearer(&token).await;
        assert_eq!(me.status_code(), 200, "login token: {}", me.text());

        let wrong = password.replacen('П', "п", 1);
        let login = server
            .post("/auth/login")
            .json(&json!({ "email": email, "password": wrong }))
            .await;
        assert_eq!(
            login.status_code(),
            401,
            "a different password must be refused: {}",
            login.text()
        );
        assert!(
            login.json::<serde_json::Value>().get("token").is_none(),
            "a refused login must not issue a token"
        );
    }

    /// Registration refuses malformed addresses and keeps plus-addressing:
    /// `name+tag@domain` registers and signs in under exactly that address.
    #[tokio::test]
    async fn register_rejects_malformed_emails_and_keeps_plus_addresses() {
        let (server, _db) = common::spawn_real_app().await;

        for email in ["test.users.opn.onl", "test@", "@users.opn.onl", ""] {
            let response = server
                .post("/auth/register")
                .json(&json!({ "email": email, "password": "password123" }))
                .await;
            assert_eq!(
                response.status_code(),
                400,
                "{email:?} accepted: {}",
                response.text()
            );
        }

        let plus = common::unique_email().replacen('@', "+opn@", 1);
        let response = server
            .post("/auth/register")
            .json(&json!({ "email": plus, "password": "password123" }))
            .await;
        assert_eq!(
            response.status_code(),
            201,
            "plus address refused: {}",
            response.text()
        );
        assert_eq!(
            response.json::<serde_json::Value>()["email"],
            plus,
            "the +tag must be kept"
        );
        let login = server
            .post("/auth/login")
            .json(&json!({ "email": plus, "password": "password123" }))
            .await;
        assert_eq!(
            login.status_code(),
            200,
            "plus address must sign in: {}",
            login.text()
        );
    }
}
