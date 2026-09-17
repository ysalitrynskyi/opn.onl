use aws_config::BehaviorVersion;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::Client as S3Client;
use chrono::Utc;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::Write;
use tokio::process::Command;
use tracing::{error, info};

/// Backup service for PostgreSQL to S3/R2
pub struct BackupService {
    s3_client: Option<S3Client>,
    bucket: String,
    database_url: String,
}

impl BackupService {
    pub async fn new() -> Self {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/opn_onl".to_string());

        let bucket = std::env::var("BACKUP_S3_BUCKET").unwrap_or_default();
        let endpoint = std::env::var("BACKUP_S3_ENDPOINT").ok();
        let access_key = std::env::var("BACKUP_S3_ACCESS_KEY").ok();
        let secret_key = std::env::var("BACKUP_S3_SECRET_KEY").ok();
        let region = std::env::var("BACKUP_S3_REGION").unwrap_or_else(|_| "auto".to_string());

        let s3_client = if let (Some(endpoint), Some(access_key), Some(secret_key)) =
            (endpoint, access_key, secret_key)
        {
            if !bucket.is_empty() {
                let creds = Credentials::new(access_key, secret_key, None, None, "static");

                let config = aws_sdk_s3::Config::builder()
                    .behavior_version(BehaviorVersion::latest())
                    .region(Region::new(region))
                    .endpoint_url(endpoint)
                    .credentials_provider(creds)
                    .force_path_style(true)
                    .build();

                info!("S3 backup service initialized");
                Some(S3Client::from_conf(config))
            } else {
                None
            }
        } else {
            info!("S3 backup not configured (missing BACKUP_S3_* env vars)");
            None
        };

        Self {
            s3_client,
            bucket,
            database_url,
        }
    }

    pub fn is_configured(&self) -> bool {
        self.s3_client.is_some()
    }

    /// Create a backup and upload to S3
    pub async fn create_backup(&self) -> Result<String, String> {
        let client = self
            .s3_client
            .as_ref()
            .ok_or("Backup service not configured")?;

        // Generate backup filename
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("backup_{}.sql.gz", timestamp);

        info!("Creating database backup: {}", filename);

        // Run pg_dump asynchronously so the dump doesn't block a runtime worker
        // thread (tokio::process spawns and awaits without blocking). The
        // password is passed via PGPASSWORD, not argv — see pg_dump_command.
        let output = Command::from(pg_dump_command(&self.database_url))
            .output()
            .await
            .map_err(|e| format!("Failed to run pg_dump: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "pg_dump failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        // Compress the dump
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(&output.stdout)
            .map_err(|e| format!("Failed to compress backup: {}", e))?;
        let compressed = encoder
            .finish()
            .map_err(|e| format!("Failed to finish compression: {}", e))?;

        info!("Backup compressed: {} bytes", compressed.len());

        // Upload to S3
        client
            .put_object()
            .bucket(&self.bucket)
            .key(&filename)
            .body(compressed.into())
            .content_type("application/gzip")
            .send()
            .await
            .map_err(|e| format!("Failed to upload backup: {}", e))?;

        info!("Backup uploaded successfully: {}", filename);
        Ok(filename)
    }

    /// List available backups
    pub async fn list_backups(&self) -> Result<Vec<String>, String> {
        let client = self
            .s3_client
            .as_ref()
            .ok_or("Backup service not configured")?;

        let response = client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix("backup_")
            .send()
            .await
            .map_err(|e| format!("Failed to list backups: {}", e))?;

        let backups: Vec<String> = response
            .contents()
            .iter()
            .filter_map(|obj| obj.key().map(|k| k.to_string()))
            .collect();

        Ok(backups)
    }

    /// Delete old backups (keep last N)
    pub async fn cleanup_old_backups(&self, keep_count: usize) -> Result<usize, String> {
        let client = self
            .s3_client
            .as_ref()
            .ok_or("Backup service not configured")?;

        let mut backups = self.list_backups().await?;
        backups.sort();
        backups.reverse(); // Most recent first

        let to_delete = backups.into_iter().skip(keep_count).collect::<Vec<_>>();
        let deleted_count = to_delete.len();

        for key in to_delete {
            if let Err(e) = client
                .delete_object()
                .bucket(&self.bucket)
                .key(&key)
                .send()
                .await
            {
                error!("Failed to delete old backup {}: {}", key, e);
            }
        }

        info!("Cleaned up {} old backups", deleted_count);
        Ok(deleted_count)
    }

    /// Download a backup
    pub async fn download_backup(&self, filename: &str) -> Result<Vec<u8>, String> {
        let client = self
            .s3_client
            .as_ref()
            .ok_or("Backup service not configured")?;

        let response = client
            .get_object()
            .bucket(&self.bucket)
            .key(filename)
            .send()
            .await
            .map_err(|e| format!("Failed to download backup: {}", e))?;

        let data = response
            .body
            .collect()
            .await
            .map_err(|e| format!("Failed to read backup data: {}", e))?;

        Ok(data.into_bytes().to_vec())
    }
}

impl Clone for BackupService {
    fn clone(&self) -> Self {
        // Clone is expensive, create new instance
        Self {
            s3_client: None, // Will be re-initialized when needed
            bucket: self.bucket.clone(),
            database_url: self.database_url.clone(),
        }
    }
}

/// Split a postgres URL into a password-free connection URI and the password.
/// The password is returned separately so `pg_dump` can receive it via
/// `PGPASSWORD` instead of the process argument list (`/proc/pid/cmdline`, `ps`).
fn pg_dump_connection(database_url: &str) -> (String, Option<String>) {
    let mut parsed = match url::Url::parse(database_url) {
        Ok(u) => u,
        Err(_) => return (database_url.to_string(), None),
    };

    let password = parsed.password().and_then(|encoded| {
        if encoded.is_empty() {
            return None;
        }
        Some(
            urlencoding::decode(encoded)
                .map(|cow| cow.into_owned())
                .unwrap_or_else(|_| encoded.to_string()),
        )
    });

    if parsed.password().is_some() {
        let _ = parsed.set_password(None);
    }

    (parsed.to_string(), password)
}

fn pg_dump_command(database_url: &str) -> std::process::Command {
    let (safe_url, password) = pg_dump_connection(database_url);
    let mut cmd = std::process::Command::new("pg_dump");
    if let Some(password) = password {
        cmd.env("PGPASSWORD", password);
    }
    cmd.arg(safe_url).arg("--no-owner").arg("--no-acl");
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_pgpassword(cmd: &std::process::Command) -> Option<String> {
        cmd.get_envs().find_map(|(k, v)| {
            if k == "PGPASSWORD" {
                v.map(|val| val.to_string_lossy().into_owned())
            } else {
                None
            }
        })
    }

    fn argv_joined(cmd: &std::process::Command) -> String {
        cmd.get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn pg_dump_argv_omits_password() {
        let url = "postgres://opn:s3cret-pass@localhost:5432/opn_onl";
        let cmd = pg_dump_command(url);
        let joined = argv_joined(&cmd);
        assert!(
            !joined.contains("s3cret-pass"),
            "password leaked onto argv: {joined}"
        );
        assert_eq!(env_pgpassword(&cmd).as_deref(), Some("s3cret-pass"));
        assert!(
            joined.contains("postgres://opn@localhost:5432/opn_onl"),
            "password-stripped URI missing from argv: {joined}"
        );
    }

    #[test]
    fn pg_dump_argv_decodes_percent_encoded_password() {
        let url = "postgres://opn:p%40ss%2Fword@localhost:5432/opn_onl";
        let cmd = pg_dump_command(url);
        let joined = argv_joined(&cmd);
        assert!(!joined.contains("p%40ss"), "encoded password on argv: {joined}");
        assert!(
            !joined.contains("p@ss/word"),
            "decoded password on argv: {joined}"
        );
        assert_eq!(env_pgpassword(&cmd).as_deref(), Some("p@ss/word"));
    }

    #[test]
    fn pg_dump_skips_pgpassword_when_url_has_no_password() {
        let url = "postgresql://opn@localhost:5432/opn_onl?sslmode=require";
        let (safe, pw) = pg_dump_connection(url);
        assert!(pw.is_none());
        assert!(safe.contains("sslmode=require"));
        let cmd = pg_dump_command(url);
        assert!(env_pgpassword(&cmd).is_none());
    }
}
