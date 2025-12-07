use lettre::{
    message::header::ContentType,
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use tracing::{error, info};

pub struct EmailService {
    mailer: Option<AsyncSmtpTransport<Tokio1Executor>>,
    from_email: String,
    from_name: String,
    frontend_url: String,
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
        let from_email = std::env::var("SMTP_FROM_EMAIL")
            .unwrap_or_else(|_| "noreply@opn.onl".to_string());
        let from_name = std::env::var("SMTP_FROM_NAME")
            .unwrap_or_else(|_| "opn.onl".to_string());
        let frontend_url = std::env::var("FRONTEND_URL")
            .unwrap_or_else(|_| "http://localhost:5173".to_string());

        let mailer = if let (Some(host), Some(user), Some(pass)) = (smtp_host, smtp_user, smtp_pass) {
            let creds = Credentials::new(user, pass);
            let smtp_tls = std::env::var("SMTP_TLS").unwrap_or_else(|_| "starttls".to_string());
            
            info!("Configuring SMTP: host={}, port={}, tls={}", host, smtp_port, smtp_tls);
            
            let transport_result = match smtp_tls.to_lowercase().as_str() {
                // Port 465 style: TLS from the start (implicit TLS / SMTPS)
                "tls" | "ssl" | "implicit" => {
                    info!("Using implicit TLS/SSL (port 465 style)");
                    match lettre::transport::smtp::client::TlsParameters::new(host.clone()) {
                        Ok(tls_params) => {
                            Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&host)
                                .port(smtp_port)
                                .tls(lettre::transport::smtp::client::Tls::Wrapper(tls_params))
                                .credentials(creds)
                                .build())
                        }
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
                        Ok(tls_params) => {
                            Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&host)
                                .port(smtp_port)
                                .tls(lettre::transport::smtp::client::Tls::Required(tls_params))
                                .credentials(creds)
                                .build())
                        }
                        Err(e) => {
                            error!("Failed to create STARTTLS parameters: {}", e);
                            Err(format!("STARTTLS error: {}", e))
                        }
                    }
                }
                // No encryption (not recommended, but useful for local testing)
                "none" | "false" | "off" => {
                    info!("Using no TLS (insecure)");
                    Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&host)
                        .port(smtp_port)
                        .tls(lettre::transport::smtp::client::Tls::None)
                        .credentials(creds)
                        .build())
                }
                // Auto-detect based on port
                _ => {
                    if smtp_port == 465 {
                        info!("Auto-detected implicit TLS for port 465");
                        match lettre::transport::smtp::client::TlsParameters::new(host.clone()) {
                            Ok(tls_params) => {
                                Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&host)
                                    .port(smtp_port)
                                    .tls(lettre::transport::smtp::client::Tls::Wrapper(tls_params))
                                    .credentials(creds)
                                    .build())
                            }
                            Err(e) => Err(format!("TLS error: {}", e))
                        }
                    } else {
                        info!("Auto-detected STARTTLS for port {}", smtp_port);
                        match lettre::transport::smtp::client::TlsParameters::new(host.clone()) {
                            Ok(tls_params) => {
                                Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&host)
                                    .port(smtp_port)
                                    .tls(lettre::transport::smtp::client::Tls::Required(tls_params))
                                    .credentials(creds)
                                    .build())
                            }
                            Err(e) => Err(format!("STARTTLS error: {}", e))
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

    async fn send_email(&self, to: &str, subject: &str, html_body: &str) -> Result<(), String> {
        self.send_email_internal(to, subject, html_body, None).await
    }

    pub async fn send_email_with_reply_to(&self, to: &str, subject: &str, html_body: &str, reply_to: &str) -> Result<(), String> {
        self.send_email_internal(to, subject, html_body, Some(reply_to)).await
    }

    async fn send_email_internal(&self, to: &str, subject: &str, html_body: &str, reply_to: Option<&str>) -> Result<(), String> {
        let mailer = self.mailer.as_ref().ok_or("Email service not configured")?;

        let mut builder = Message::builder()
            .from(format!("{} <{}>", self.from_name, self.from_email).parse().map_err(|e| format!("Invalid from address: {}", e))?)
            .to(to.parse().map_err(|e| format!("Invalid to address: {}", e))?)
            .subject(subject);

        if let Some(reply) = reply_to {
            builder = builder.reply_to(reply.parse().map_err(|e| format!("Invalid reply-to address: {}", e))?);
        }

        let email = builder
            .header(ContentType::TEXT_HTML)
            .body(html_body.to_string())
            .map_err(|e| format!("Failed to build email: {}", e))?;

        mailer.send(email).await.map_err(|e| format!("Failed to send email: {}", e))?;
        Ok(())
    }

    pub async fn send_verification_email(&self, to: &str, token: &str) -> Result<(), String> {
        let verification_url = format!("{}/verify-email?token={}", self.frontend_url, token);
        
        let html = format!(r#"
<!DOCTYPE html>
<html>
<head>
    <style>
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; line-height: 1.6; color: #333; }}
        .container {{ max-width: 600px; margin: 0 auto; padding: 20px; }}
        .button {{ display: inline-block; padding: 12px 24px; background-color: #2563eb; color: white; text-decoration: none; border-radius: 8px; font-weight: 600; }}
        .footer {{ margin-top: 40px; font-size: 12px; color: #666; }}
    </style>
</head>
<body>
    <div class="container">
        <h1>Verify your email</h1>
        <p>Thanks for signing up for opn.onl! Please verify your email address by clicking the button below:</p>
        <p><a href="{}" class="button">Verify Email</a></p>
        <p>Or copy and paste this link into your browser:</p>
        <p><a href="{}">{}</a></p>
        <p>This link expires in 24 hours.</p>
        <div class="footer">
            <p>If you didn't create an account on opn.onl, you can safely ignore this email.</p>
        </div>
    </div>
</body>
</html>
"#, verification_url, verification_url, verification_url);

        self.send_email(to, "Verify your email - opn.onl", &html).await
    }

    pub async fn send_password_reset_email(&self, to: &str, token: &str) -> Result<(), String> {
        let reset_url = format!("{}/reset-password?token={}", self.frontend_url, token);
        
        let html = format!(r#"
<!DOCTYPE html>
<html>
<head>
    <style>
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; line-height: 1.6; color: #333; }}
        .container {{ max-width: 600px; margin: 0 auto; padding: 20px; }}
        .button {{ display: inline-block; padding: 12px 24px; background-color: #2563eb; color: white; text-decoration: none; border-radius: 8px; font-weight: 600; }}
        .footer {{ margin-top: 40px; font-size: 12px; color: #666; }}
    </style>
</head>
<body>
    <div class="container">
        <h1>Reset your password</h1>
        <p>We received a request to reset your password. Click the button below to choose a new password:</p>
        <p><a href="{}" class="button">Reset Password</a></p>
        <p>Or copy and paste this link into your browser:</p>
        <p><a href="{}">{}</a></p>
        <p>This link expires in 1 hour.</p>
        <div class="footer">
            <p>If you didn't request a password reset, you can safely ignore this email.</p>
        </div>
    </div>
</body>
</html>
"#, reset_url, reset_url, reset_url);

        self.send_email(to, "Reset your password - opn.onl", &html).await
    }

    pub async fn send_welcome_email(&self, to: &str) -> Result<(), String> {
        let html = format!(r#"
<!DOCTYPE html>
<html>
<head>
    <style>
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; line-height: 1.6; color: #333; }}
        .container {{ max-width: 600px; margin: 0 auto; padding: 20px; }}
        .button {{ display: inline-block; padding: 12px 24px; background-color: #2563eb; color: white; text-decoration: none; border-radius: 8px; font-weight: 600; }}
    </style>
</head>
<body>
    <div class="container">
        <h1>Welcome to opn.onl!</h1>
        <p>Your email has been verified and your account is now active.</p>
        <p>You can now create short links, track analytics, and more.</p>
        <p><a href="{}/dashboard" class="button">Go to Dashboard</a></p>
    </div>
</body>
</html>
"#, self.frontend_url);

        self.send_email(to, "Welcome to opn.onl!", &html).await
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

