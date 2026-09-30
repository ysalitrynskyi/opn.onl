use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::MultiPart,
    transport::smtp::authentication::Credentials,
};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};

/// Global email rate limiter to prevent abuse and control costs
/// Uses a sliding window approach: tracks emails sent in the current hour
struct GlobalEmailRateLimiter {
    /// Number of emails sent in the current hour window
    count: AtomicU64,
    /// Start of the current hour window (Unix timestamp)
    window_start: Mutex<u64>,
    /// Maximum emails per hour (configurable via EMAIL_RATE_LIMIT_PER_HOUR)
    limit: u64,
}

impl GlobalEmailRateLimiter {
    fn new() -> Self {
        let limit = std::env::var("EMAIL_RATE_LIMIT_PER_HOUR")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(500); // Default: 500 emails per hour

        info!("Email rate limit configured: {} emails/hour", limit);

        Self {
            count: AtomicU64::new(0),
            window_start: Mutex::new(Self::current_hour()),
            limit,
        }
    }

    fn current_hour() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            / 3600
    }

    /// Try to acquire a permit to send an email
    /// Returns Ok(()) if allowed, Err with message if rate limited
    fn try_acquire(&self) -> Result<(), String> {
        let current_hour = Self::current_hour();

        // Check if we need to reset the window
        {
            let mut window = self.window_start.lock();
            if *window < current_hour {
                // New hour, reset counter
                *window = current_hour;
                self.count.store(0, Ordering::SeqCst);
                info!("Email rate limit window reset");
            }
        }

        // Try to increment counter
        let current = self.count.fetch_add(1, Ordering::SeqCst);

        if current >= self.limit {
            // We're over the limit, decrement back
            self.count.fetch_sub(1, Ordering::SeqCst);
            warn!(
                "Email rate limit exceeded: {}/{} emails this hour",
                current, self.limit
            );
            Err(format!(
                "Email rate limit exceeded ({}/hour). Please try again later.",
                self.limit
            ))
        } else {
            Ok(())
        }
    }

    /// Get current usage stats
    fn stats(&self) -> (u64, u64) {
        (self.count.load(Ordering::SeqCst), self.limit)
    }
}

/// Global singleton for email rate limiting
static EMAIL_RATE_LIMITER: once_cell::sync::Lazy<GlobalEmailRateLimiter> =
    once_cell::sync::Lazy::new(GlobalEmailRateLimiter::new);

pub struct EmailService {
    mailer: Option<AsyncSmtpTransport<Tokio1Executor>>,
    from_email: String,
    from_name: String,
    frontend_url: String,
}

impl Default for EmailService {
    fn default() -> Self {
        Self::new()
    }
}

impl EmailService {
    pub fn new() -> Self {
        let smtp_host = std::env::var("SMTP_HOST").ok();
        let smtp_port = std::env::var("SMTP_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(587);
        let smtp_user = std::env::var("SMTP_USER").ok();
        let smtp_pass = std::env::var("SMTP_PASS").ok();
        let from_email =
            std::env::var("SMTP_FROM_EMAIL").unwrap_or_else(|_| "noreply@opn.onl".to_string());
        let from_name = std::env::var("SMTP_FROM_NAME").unwrap_or_else(|_| "opn.onl".to_string());
        let frontend_url =
            std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:5173".to_string());

        let mailer = if let (Some(host), Some(user), Some(pass)) = (smtp_host, smtp_user, smtp_pass)
        {
            let creds = Credentials::new(user, pass);
            let smtp_tls = std::env::var("SMTP_TLS").unwrap_or_else(|_| "starttls".to_string());

            info!(
                "Configuring SMTP: host={}, port={}, tls={}",
                host, smtp_port, smtp_tls
            );

            let transport_result = match smtp_tls.to_lowercase().as_str() {
                // Port 465 style: TLS from the start (implicit TLS / SMTPS)
                "tls" | "ssl" | "implicit" => {
                    info!("Using implicit TLS/SSL (port 465 style)");
                    match lettre::transport::smtp::client::TlsParameters::new(host.clone()) {
                        Ok(tls_params) => Ok(
                            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&host)
                                .port(smtp_port)
                                .tls(lettre::transport::smtp::client::Tls::Wrapper(tls_params))
                                .credentials(creds)
                                .build(),
                        ),
                        Err(e) => {
                            error!("Failed to create TLS parameters: {}", e);
                            Err(format!("TLS error: {}", e))
                        }
                    }
                }
                // Port 587 style: STARTTLS (start plain, upgrade to TLS)
                "starttls" | "required" => {
                    info!("Using STARTTLS (port 587 style)");
                    match lettre::transport::smtp::client::TlsParameters::new(host.clone()) {
                        Ok(tls_params) => Ok(
                            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&host)
                                .port(smtp_port)
                                .tls(lettre::transport::smtp::client::Tls::Required(tls_params))
                                .credentials(creds)
                                .build(),
                        ),
                        Err(e) => {
                            error!("Failed to create STARTTLS parameters: {}", e);
                            Err(format!("STARTTLS error: {}", e))
                        }
                    }
                }
                // No encryption (not recommended, but useful for local testing)
                "none" | "false" | "off" => {
                    info!("Using no TLS (insecure)");
                    Ok(
                        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&host)
                            .port(smtp_port)
                            .tls(lettre::transport::smtp::client::Tls::None)
                            .credentials(creds)
                            .build(),
                    )
                }
                // Auto-detect based on port
                _ => {
                    if smtp_port == 465 {
                        info!("Auto-detected implicit TLS for port 465");
                        match lettre::transport::smtp::client::TlsParameters::new(host.clone()) {
                            Ok(tls_params) => Ok(
                                AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&host)
                                    .port(smtp_port)
                                    .tls(lettre::transport::smtp::client::Tls::Wrapper(tls_params))
                                    .credentials(creds)
                                    .build(),
                            ),
                            Err(e) => Err(format!("TLS error: {}", e)),
                        }
                    } else {
                        info!("Auto-detected STARTTLS for port {}", smtp_port);
                        match lettre::transport::smtp::client::TlsParameters::new(host.clone()) {
                            Ok(tls_params) => Ok(
                                AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&host)
                                    .port(smtp_port)
                                    .tls(lettre::transport::smtp::client::Tls::Required(tls_params))
                                    .credentials(creds)
                                    .build(),
                            ),
                            Err(e) => Err(format!("STARTTLS error: {}", e)),
                        }
                    }
                }
            };

            match transport_result {
                Ok(transport) => {
                    info!("SMTP email service initialized successfully");
                    Some(transport)
                }
                Err(e) => {
                    error!("Failed to initialize SMTP: {}", e);
                    None
                }
            }
        } else {
            info!("SMTP not configured (missing host/user/pass), email service disabled");
            None
        };

        Self {
            mailer,
            from_email,
            from_name,
            frontend_url,
        }
    }

    pub fn is_configured(&self) -> bool {
        self.mailer.is_some()
    }

    async fn send_email(&self, to: &str, subject: &str, body: &EmailBody) -> Result<(), String> {
        self.send_email_internal(to, subject, body, None).await
    }

    pub async fn send_email_with_reply_to(
        &self,
        to: &str,
        subject: &str,
        body: &EmailBody,
        reply_to: &str,
    ) -> Result<(), String> {
        self.send_email_internal(to, subject, body, Some(reply_to))
            .await
    }

    async fn send_email_internal(
        &self,
        to: &str,
        subject: &str,
        body: &EmailBody,
        reply_to: Option<&str>,
    ) -> Result<(), String> {
        let mailer = self.mailer.as_ref().ok_or("Email service not configured")?;

        // Check global rate limit before sending
        EMAIL_RATE_LIMITER.try_acquire()?;

        let (used, limit) = EMAIL_RATE_LIMITER.stats();
        info!("Sending email to {} ({}/{} this hour)", to, used, limit);

        let email = self.build_message(to, subject, body, reply_to)?;

        mailer
            .send(email)
            .await
            .map_err(|e| format!("Failed to send email: {}", e))?;
        Ok(())
    }

    /// A multipart/alternative message: the plain-text part first, the HTML
    /// part second, so clients that do not render HTML (and spam filters that
    /// score HTML-only mail) get readable text.
    fn build_message(
        &self,
        to: &str,
        subject: &str,
        body: &EmailBody,
        reply_to: Option<&str>,
    ) -> Result<Message, String> {
        let mut builder = Message::builder()
            .from(
                format!("{} <{}>", self.from_name, self.from_email)
                    .parse()
                    .map_err(|e| format!("Invalid from address: {}", e))?,
            )
            .to(to
                .parse()
                .map_err(|e| format!("Invalid to address: {}", e))?)
            .subject(subject);

        if let Some(reply) = reply_to {
            builder = builder.reply_to(
                reply
                    .parse()
                    .map_err(|e| format!("Invalid reply-to address: {}", e))?,
            );
        }

        builder
            .multipart(MultiPart::alternative_plain_html(
                body.text.clone(),
                body.html.clone(),
            ))
            .map_err(|e| format!("Failed to build email: {}", e))
    }

    fn verification_email(&self, token: &str) -> EmailBody {
        let url = format!("{}/verify-email?token={}", self.frontend_url, token);
        EmailBody::new(
            "Verify your email",
            &[
                "Thanks for signing up for opn.onl! Please verify your email address by clicking the button below:",
            ],
            Some(("Verify Email", &url)),
            &["This link expires in 24 hours."],
            "If you didn't create an account on opn.onl, you can safely ignore this email.",
        )
    }

    fn password_reset_email(&self, token: &str) -> EmailBody {
        let url = format!("{}/reset-password?token={}", self.frontend_url, token);
        EmailBody::new(
            "Reset your password",
            &[
                "We received a request to reset your password. Click the button below to choose a new password:",
            ],
            Some(("Reset Password", &url)),
            &["This link expires in 1 hour."],
            "If you didn't request a password reset, you can safely ignore this email.",
        )
    }

    fn welcome_email(&self) -> EmailBody {
        let url = format!("{}/dashboard", self.frontend_url);
        EmailBody::new(
            "Welcome to opn.onl!",
            &[
                "Your email has been verified and your account is now active.",
                "You can now create short links, track analytics, and more.",
            ],
            Some(("Go to Dashboard", &url)),
            &[],
            "",
        )
    }

    fn password_changed_email(&self, changed_at: &str) -> EmailBody {
        let url = format!("{}/forgot-password", self.frontend_url);
        let when = format!("The password for your opn.onl account was changed on {changed_at}.");
        EmailBody::new(
            "Your password was changed",
            &[
                &when,
                "Sessions signed in with the old password have been signed out.",
                "If you made this change, you don't need to do anything. If you didn't, reset your password now:",
            ],
            Some(("Reset Password", &url)),
            &[],
            "You get this email whenever the password on your account changes.",
        )
    }

    pub async fn send_verification_email(&self, to: &str, token: &str) -> Result<(), String> {
        self.send_email(
            to,
            "Verify your email - opn.onl",
            &self.verification_email(token),
        )
        .await
    }

    pub async fn send_password_reset_email(&self, to: &str, token: &str) -> Result<(), String> {
        self.send_email(
            to,
            "Reset your password - opn.onl",
            &self.password_reset_email(token),
        )
        .await
    }

    pub async fn send_welcome_email(&self, to: &str) -> Result<(), String> {
        self.send_email(to, "Welcome to opn.onl!", &self.welcome_email())
            .await
    }

    /// Security notice after a password change or reset. If the owner did not
    /// make the change, this is how they find out.
    pub async fn send_password_changed_email(&self, to: &str) -> Result<(), String> {
        let changed_at = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
        self.send_email(
            to,
            "Your password was changed - opn.onl",
            &self.password_changed_email(&changed_at),
        )
        .await
    }
}

/// The two renderings of one transactional email.
pub struct EmailBody {
    pub html: String,
    pub text: String,
}

impl EmailBody {
    /// Contact-form mail and other one-off messages that bring their own
    /// markup and text.
    pub fn from_parts(html: String, text: String) -> Self {
        Self { html, text }
    }

    /// The layout every account email shares: a heading, paragraphs, an
    /// optional button (its URL repeated as a plain link), more paragraphs,
    /// and a small footer. Only fixed copy and URLs we built go in here, so
    /// nothing needs HTML escaping.
    fn new(
        heading: &str,
        before: &[&str],
        button: Option<(&str, &str)>,
        after: &[&str],
        footer: &str,
    ) -> Self {
        let mut html = String::from(
            r#"<!DOCTYPE html>
<html>
<head>
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; line-height: 1.6; color: #333; }
        .container { max-width: 600px; margin: 0 auto; padding: 20px; }
        .button { display: inline-block; padding: 12px 24px; background-color: #2563eb; color: white; text-decoration: none; border-radius: 8px; font-weight: 600; }
        .footer { margin-top: 40px; font-size: 12px; color: #666; }
    </style>
</head>
<body>
    <div class="container">
"#,
        );
        let mut text = format!("{heading}\n\n");
        html.push_str(&format!("        <h1>{heading}</h1>\n"));
        for paragraph in before {
            html.push_str(&format!("        <p>{paragraph}</p>\n"));
            text.push_str(&format!("{paragraph}\n\n"));
        }
        if let Some((label, url)) = button {
            html.push_str(&format!(
                "        <p><a href=\"{url}\" class=\"button\">{label}</a></p>\n        <p>Or copy and paste this link into your browser:</p>\n        <p><a href=\"{url}\">{url}</a></p>\n"
            ));
            text.push_str(&format!("{url}\n\n"));
        }
        for paragraph in after {
            html.push_str(&format!("        <p>{paragraph}</p>\n"));
            text.push_str(&format!("{paragraph}\n\n"));
        }
        if !footer.is_empty() {
            html.push_str(&format!(
                "        <div class=\"footer\">\n            <p>{footer}</p>\n        </div>\n"
            ));
            text.push_str(&format!("{footer}\n"));
        }
        html.push_str("    </div>\n</body>\n</html>\n");
        Self {
            html,
            text: text.trim_end().to_string() + "\n",
        }
    }
}

impl Clone for EmailService {
    fn clone(&self) -> Self {
        Self::new()
    }
}

pub fn generate_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..64)
        .map(|_| {
            let idx = rng.gen_range(0..62);
            let chars: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
            chars[idx] as char
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{EmailService, generate_token};

    fn service() -> EmailService {
        EmailService {
            mailer: None,
            from_email: "noreply@opn.example".to_string(),
            from_name: "opn.onl".to_string(),
            frontend_url: "https://opn.example".to_string(),
        }
    }

    /// Every account email carries a text/plain part next to the HTML, and
    /// the text part has the link a reader needs.
    #[test]
    fn account_emails_are_multipart_with_a_usable_text_part() {
        let svc = service();
        let bodies = [
            (
                svc.verification_email("tok123"),
                "https://opn.example/verify-email?token=tok123",
            ),
            (
                svc.password_reset_email("tok456"),
                "https://opn.example/reset-password?token=tok456",
            ),
            (svc.welcome_email(), "https://opn.example/dashboard"),
            (
                svc.password_changed_email("2026-09-30 12:00 UTC"),
                "https://opn.example/forgot-password",
            ),
        ];
        for (body, link) in &bodies {
            assert!(
                body.text.contains(link),
                "text part lacks {link}: {}",
                body.text
            );
            assert!(
                !body.text.contains('<'),
                "text part has markup: {}",
                body.text
            );
            assert!(
                body.html.contains(&format!("href=\"{link}\"")),
                "{}",
                body.html
            );

            let message = svc
                .build_message("user@opn.example", "Subject", body, None)
                .expect("message builds");
            let raw = String::from_utf8(message.formatted()).expect("utf-8");
            assert!(raw.contains("multipart/alternative"), "{raw}");
            assert!(raw.contains("text/plain"), "{raw}");
            assert!(raw.contains("text/html"), "{raw}");
        }
        assert!(bodies[3].0.text.contains("2026-09-30 12:00 UTC"));
    }

    #[test]
    fn generate_token_is_64_alnum_and_unique() {
        let a = generate_token();
        let b = generate_token();
        assert_eq!(a.len(), 64);
        assert_eq!(b.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(a, b);
    }
}
