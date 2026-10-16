use lettre::{
    Address, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, MultiPart},
    transport::smtp::authentication::Credentials,
};
use tracing::{error, info};

use super::email_limits::{MailDenied, MailKind, Pools, TriggerLimiter, loggable_domain};
use super::env::env_nonempty;

/// Hourly mail budget when `EMAIL_RATE_LIMIT_PER_HOUR` is unset or invalid.
const DEFAULT_HOURLY_LIMIT: u64 = 500;

/// How the password changed, which decides what the security notice can say
/// was revoked. A reset is account recovery, so it also removes the API keys
/// and passkeys; a change made while signed in leaves them in place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasswordChange {
    Reset,
    Change,
}

pub struct EmailService {
    mailer: Option<AsyncSmtpTransport<Tokio1Executor>>,
    from_email: String,
    from_name: String,
    frontend_url: String,
    /// Instance-wide hourly budget, split by kind of mail.
    pools: Pools,
    /// Per-recipient and per-client limits on mail a request can trigger.
    triggers: TriggerLimiter,
}

impl Default for EmailService {
    fn default() -> Self {
        Self::new()
    }
}

impl EmailService {
    pub fn new() -> Self {
        // Compose passes SMTP_HOST/USER/PASS as empty strings when they are not
        // set, and an empty host is not a configured server.
        let smtp_host = env_nonempty("SMTP_HOST");
        let smtp_port = std::env::var("SMTP_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(587);
        let smtp_user = env_nonempty("SMTP_USER");
        let smtp_pass = env_nonempty("SMTP_PASS");
        let from_email =
            env_nonempty("SMTP_FROM_EMAIL").unwrap_or_else(|| "noreply@opn.onl".to_string());
        // A blank SMTP_FROM_NAME is kept: it means "send the bare address".
        let from_name = std::env::var("SMTP_FROM_NAME").unwrap_or_else(|_| "opn.onl".to_string());
        // A trailing slash would turn every button into `https://host//verify-email`.
        let frontend_url = super::env::frontend_base_url();

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

        let hourly_limit = std::env::var("EMAIL_RATE_LIMIT_PER_HOUR")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_HOURLY_LIMIT);
        let pools = Pools::from_total(hourly_limit);
        let (account, security, contact) = pools.sizes();
        info!(
            "Email rate limit configured: {} emails/hour (account {}, security notices {}, contact form {})",
            hourly_limit, account, security, contact
        );

        Self {
            mailer,
            from_email,
            from_name,
            frontend_url,
            pools,
            triggers: TriggerLimiter::new(),
        }
    }

    /// Replace the hourly budget (`EMAIL_RATE_LIMIT_PER_HOUR`) of this service.
    pub fn with_hourly_limit(mut self, total: u64) -> Self {
        self.pools = Pools::from_total(total);
        self
    }

    pub fn is_configured(&self) -> bool {
        self.mailer.is_some()
    }

    /// A service that reports itself configured, aimed at a port that accepts
    /// nothing. Integration tests use it so they can set `email_configured`
    /// without writing `SMTP_*` into the shared process environment. Sends fail.
    pub fn configured_for_tests() -> Self {
        let mut service = Self::new();
        let mailer = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous("127.0.0.1")
            .port(1)
            .tls(lettre::transport::smtp::client::Tls::None)
            .build();
        service.mailer = Some(mailer);
        service
    }

    /// Ask to send one mail of `kind` to `recipient` because of a request from
    /// `client` (its [`ClientKey`](super::rate_limiter::ClientKey)). `Ok`
    /// spends the recipient's and the client's allowance; the caller then
    /// sends. `Err` means send nothing, and leave any token already mailed to
    /// that address as it is.
    ///
    /// Also refuses while the pool the mail would draw on is spent, so a
    /// request that cannot be served does not replace a working link.
    pub fn reserve(&self, kind: MailKind, recipient: &str, client: &str) -> Result<(), MailDenied> {
        self.pools.check(kind)?;
        self.triggers.try_acquire(kind, recipient, client)
    }

    async fn send_email(
        &self,
        kind: MailKind,
        to: &str,
        subject: &str,
        body: &EmailBody,
    ) -> Result<(), String> {
        self.send_email_internal(kind, to, subject, body, None)
            .await
    }

    /// Mail a contact-form message, with the visitor's address as Reply-To.
    pub async fn send_contact_email(
        &self,
        to: &str,
        subject: &str,
        body: &EmailBody,
        reply_to: &str,
    ) -> Result<(), String> {
        self.send_email_internal(MailKind::Contact, to, subject, body, Some(reply_to))
            .await
    }

    async fn send_email_internal(
        &self,
        kind: MailKind,
        to: &str,
        subject: &str,
        body: &EmailBody,
        reply_to: Option<&str>,
    ) -> Result<(), String> {
        let mailer = self.mailer.as_ref().ok_or("Email service not configured")?;

        // The pool this kind of mail draws on, so a flood of one kind cannot
        // use up what another needs.
        let (used, limit) = self.pools.try_acquire(kind).map_err(|_| {
            format!(
                "Email rate limit exceeded for {} mail ({}/hour in total). Please try again later.",
                kind.label(),
                self.pools.total()
            )
        })?;
        // The domain only: addresses do not belong in logs.
        info!(
            "Sending {} email to a {} address ({}/{} of its pool this hour)",
            kind.label(),
            loggable_domain(to),
            used,
            limit
        );

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
            .from(self.sender()?)
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

    /// The sender as a mailbox. The display name is set on the mailbox rather
    /// than spliced into an address string, so a name such as `Acme, Inc.`
    /// is quoted instead of being rejected by the address grammar.
    fn sender(&self) -> Result<Mailbox, String> {
        let address: Address = self
            .from_email
            .parse()
            .map_err(|e| format!("Invalid from address: {}", e))?;
        let name = self.from_name.trim();
        Ok(Mailbox::new(
            (!name.is_empty()).then(|| name.to_string()),
            address,
        ))
    }

    /// `{frontend}/{path}` with one slash, even if the stored base still has one.
    fn app_url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            super::env::public_base_or(Some(&self.frontend_url), "http://localhost:5173"),
            path
        )
    }

    fn verification_email(&self, token: &str) -> EmailBody {
        let url = self.app_url(&format!("verify-email?token={token}"));
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
        let url = self.app_url(&format!("reset-password?token={token}"));
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
        let url = self.app_url("dashboard");
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

    fn password_changed_email(&self, change: PasswordChange, changed_at: &str) -> EmailBody {
        let url = self.app_url("forgot-password");
        let (verb, revoked, advice) = match change {
            PasswordChange::Reset => (
                "reset",
                "Every signed-in session was signed out, and all API keys and passkeys on the account were removed. Create new ones in Settings if you need them.",
                "If you made this change, you don't need to do anything. If you didn't, reset your password again now:",
            ),
            PasswordChange::Change => (
                "changed",
                "Your other signed-in sessions were signed out. Your API keys and passkeys were not changed.",
                "If you made this change, you don't need to do anything. If you didn't, reset your password now, which also removes all API keys and passkeys:",
            ),
        };
        let when = format!("The password for your opn.onl account was {verb} on {changed_at}.");
        EmailBody::new(
            "Your password was changed",
            &[&when, revoked, advice],
            Some(("Reset Password", &url)),
            &[],
            "You get this email whenever the password on your account changes.",
        )
    }

    pub async fn send_verification_email(&self, to: &str, token: &str) -> Result<(), String> {
        self.send_email(
            MailKind::Verification,
            to,
            "Verify your email - opn.onl",
            &self.verification_email(token),
        )
        .await
    }

    pub async fn send_password_reset_email(&self, to: &str, token: &str) -> Result<(), String> {
        self.send_email(
            MailKind::PasswordReset,
            to,
            "Reset your password - opn.onl",
            &self.password_reset_email(token),
        )
        .await
    }

    pub async fn send_welcome_email(&self, to: &str) -> Result<(), String> {
        self.send_email(
            MailKind::Welcome,
            to,
            "Welcome to opn.onl!",
            &self.welcome_email(),
        )
        .await
    }

    /// Security notice after a password change or reset. If the owner did not
    /// make the change, this is how they find out, so it says what was and
    /// was not revoked for the way the password changed.
    pub async fn send_password_changed_email(
        &self,
        to: &str,
        change: PasswordChange,
    ) -> Result<(), String> {
        let changed_at = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
        self.send_email(
            MailKind::PasswordChanged,
            to,
            "Your password was changed - opn.onl",
            &self.password_changed_email(change, &changed_at),
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

/// Background mails that have not finished yet.
static IN_FLIGHT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Counts one background mail until dropped, so a send that panics is not
/// counted forever.
struct InFlight;

impl InFlight {
    fn begin() -> Self {
        IN_FLIGHT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Self
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        IN_FLIGHT.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Send a mail from a task of its own and return at once. A request that waited
/// for SMTP would take longer for an address that has an account, which is an
/// oracle for the endpoints that must answer every address alike, and it would
/// be only as fast as the mail server (lettre waits up to 60 s per command).
/// `what` names the mail in the log line for a failed send.
pub fn send_in_background<F>(what: &'static str, send: F)
where
    F: std::future::Future<Output = Result<(), String>> + Send + 'static,
{
    let in_flight = InFlight::begin();
    tokio::spawn(async move {
        let _in_flight = in_flight;
        if let Err(e) = send.await {
            error!("Failed to send {what} email: {e}");
        }
    });
}

/// Wait until every mail handed to [`send_in_background`] has finished or
/// `timeout` has passed; `true` means none is left. The server calls this after
/// it stops accepting requests, so a deploy does not drop mail that was already
/// promised; tests call it to see what was really sent.
pub async fn wait_for_background_mail(timeout: std::time::Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    while IN_FLIGHT.load(std::sync::atomic::Ordering::SeqCst) > 0 {
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{EmailService, PasswordChange, Pools, TriggerLimiter, generate_token};

    fn service() -> EmailService {
        EmailService {
            mailer: None,
            from_email: "noreply@opn.example".to_string(),
            from_name: "opn.onl".to_string(),
            frontend_url: "https://opn.example".to_string(),
            pools: Pools::from_total(500),
            triggers: TriggerLimiter::new(),
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
                svc.password_changed_email(PasswordChange::Change, "2026-09-30 12:00 UTC"),
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

    /// A stored base that still ends with `/` must not double the slash.
    #[test]
    fn account_email_links_drop_a_trailing_slash_on_the_frontend_url() {
        let mut svc = service();
        svc.frontend_url = "https://opn.example/".to_string();
        let bodies = [
            svc.verification_email("tok123").text,
            svc.password_reset_email("tok456").text,
            svc.welcome_email().text,
            svc.password_changed_email(PasswordChange::Change, "2026-09-30 12:00 UTC")
                .text,
        ];
        for text in &bodies {
            assert!(
                !text.contains("https://opn.example//"),
                "doubled slash: {text}"
            );
        }
        assert!(bodies[0].contains("https://opn.example/verify-email?token=tok123"));
        assert!(bodies[1].contains("https://opn.example/reset-password?token=tok456"));
        assert!(bodies[2].contains("https://opn.example/dashboard"));
        assert!(bodies[3].contains("https://opn.example/forgot-password"));
    }

    /// The notice is the one place a person learns what a password change
    /// revoked, so it must not claim more or less than the change did.
    #[test]
    fn the_password_notice_matches_what_each_kind_of_change_revoked() {
        let svc = service();
        let reset = svc
            .password_changed_email(PasswordChange::Reset, "2026-09-30 12:00 UTC")
            .text;
        assert!(
            reset.contains("was reset on 2026-09-30 12:00 UTC"),
            "{reset}"
        );
        assert!(
            reset.contains("all API keys and passkeys on the account were removed"),
            "{reset}"
        );
        assert!(!reset.contains("were not changed"), "{reset}");

        let change = svc
            .password_changed_email(PasswordChange::Change, "2026-09-30 12:00 UTC")
            .text;
        assert!(
            change.contains("was changed on 2026-09-30 12:00 UTC"),
            "{change}"
        );
        assert!(
            change.contains("other signed-in sessions were signed out"),
            "{change}"
        );
        assert!(
            change.contains("API keys and passkeys were not changed"),
            "{change}"
        );
        assert!(!change.contains("were removed"), "{change}");
    }

    /// The display name the From header carries, whether the writer left it
    /// plain, quoted it, or sent it as an RFC 2047 encoded word.
    fn from_display_name(raw: &str) -> String {
        use base64::Engine as _;
        let line = raw
            .lines()
            .find_map(|line| line.strip_prefix("From: "))
            .expect("From header");
        let name = line.split(" <").next().expect("name part");
        match name
            .strip_prefix("=?utf-8?b?")
            .and_then(|rest| rest.strip_suffix("?="))
        {
            Some(encoded) => String::from_utf8(
                base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .expect("base64"),
            )
            .expect("utf-8"),
            None => name
                .strip_prefix('"')
                .and_then(|quoted| quoted.strip_suffix('"'))
                .unwrap_or(name)
                .replace("\\\"", "\""),
        }
    }

    /// `SMTP_FROM_NAME="Acme, Inc."` used to fail the address grammar, and
    /// every email (password reset included) then failed with "Invalid from
    /// address".
    #[test]
    fn a_sender_name_with_special_characters_is_quoted_not_rejected() {
        for name in [
            "Acme, Inc.",
            "Acme <Support>",
            "Say \"hi\"",
            "Caf\u{e9}, Cr\u{e8}me",
            "opn.onl",
        ] {
            let mut svc = service();
            svc.from_name = name.to_string();
            let message = svc
                .build_message("user@opn.example", "Subject", &svc.welcome_email(), None)
                .unwrap_or_else(|e| panic!("sender name {name:?}: {e}"));
            let raw = String::from_utf8(message.formatted()).expect("utf-8");
            assert!(
                raw.contains("<noreply@opn.example>"),
                "sender name {name:?}: {raw}"
            );
            assert_eq!(from_display_name(&raw), name, "{raw}");
        }
    }

    #[test]
    fn an_empty_sender_name_sends_the_bare_address() {
        let mut svc = service();
        svc.from_name = "  ".to_string();
        let message = svc
            .build_message("user@opn.example", "Subject", &svc.welcome_email(), None)
            .expect("message builds");
        let raw = String::from_utf8(message.formatted()).expect("utf-8");
        assert!(raw.contains("From: noreply@opn.example"), "{raw}");
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
