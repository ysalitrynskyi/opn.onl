use aws_config::BehaviorVersion;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::config::{Credentials, Region};
use chrono::Utc;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::Write;
use std::process::Command;
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
                let creds = Credentials::new(
                    access_key,
                    secret_key,
                    None,
                    None,
                    "static",
                );

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
        let client = self.s3_client.as_ref().ok_or("Backup service not configured")?;

        // Generate backup filename
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("backup_{}.sql.gz", timestamp);

        info!("Creating database backup: {}", filename);

        // Run pg_dump
        let output = Command::new("pg_dump")
            .arg(&self.database_url)
            .arg("--no-owner")
            .arg("--no-acl")
            .output()
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
        let client = self.s3_client.as_ref().ok_or("Backup service not configured")?;

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
        let client = self.s3_client.as_ref().ok_or("Backup service not configured")?;

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
        let client = self.s3_client.as_ref().ok_or("Backup service not configured")?;

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




