use aws_config::BehaviorVersion;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::primitives::ByteStream;
use chrono::{DateTime, Utc};
use flate2::Compression;
use flate2::write::GzEncoder;
use std::future::Future;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;
use tracing::{error, info, warn};

use super::env::{env_nonempty, env_parse};

/// Hours between scheduled backups when `BACKUP_INTERVAL_HOURS` is not set.
pub const DEFAULT_BACKUP_INTERVAL_HOURS: u32 = 24;

/// Backups the scheduled prune leaves in place when `BACKUP_KEEP` is not set.
pub const DEFAULT_BACKUP_KEEP: usize = 14;

/// Every backup object name starts with this, and it is the prefix the bucket
/// is listed under.
const BACKUP_PREFIX: &str = "backup_";

/// When scheduled backups run, and how many backups the prune after each one
/// leaves behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupSchedule {
    pub interval_hours: u32,
    pub keep: usize,
}

impl BackupSchedule {
    /// The schedule for the given settings, or `None` when nothing should run:
    /// no backup storage is configured, or the interval is 0. `keep` is raised
    /// to 1, because the prune that follows a backup must never remove the
    /// backup it follows.
    pub fn resolve(configured: bool, interval_hours: u32, keep: usize) -> Option<Self> {
        if !configured || interval_hours == 0 {
            return None;
        }
        Some(Self {
            interval_hours,
            keep: keep.max(1),
        })
    }

    pub fn interval(&self) -> Duration {
        Duration::from_secs(u64::from(self.interval_hours) * 60 * 60)
    }
}

/// `BACKUP_INTERVAL_HOURS` and `BACKUP_KEEP`, which compose passes as empty
/// strings when the operator left them alone.
fn schedule_from_env(configured: bool) -> Option<BackupSchedule> {
    let interval_hours = env_parse("BACKUP_INTERVAL_HOURS", DEFAULT_BACKUP_INTERVAL_HOURS);
    let keep = env_parse("BACKUP_KEEP", DEFAULT_BACKUP_KEEP);
    let schedule = BackupSchedule::resolve(configured, interval_hours, keep);
    if schedule.is_some() && keep == 0 {
        warn!("BACKUP_KEEP=0 would delete every backup, including the one just taken; keeping 1");
    }
    schedule
}

/// Backup service for PostgreSQL to S3/R2
pub struct BackupService {
    s3_client: Option<S3Client>,
    bucket: String,
    database_url: String,
    schedule: Option<BackupSchedule>,
}

impl BackupService {
    pub async fn new() -> Self {
        let database_url = crate::utils::database_url::resolve_database_url();

        // Compose passes unset options as empty strings; a blank endpoint or
        // key is not a configured bucket.
        let bucket = env_nonempty("BACKUP_S3_BUCKET").unwrap_or_default();
        let endpoint = env_nonempty("BACKUP_S3_ENDPOINT");
        let access_key = env_nonempty("BACKUP_S3_ACCESS_KEY");
        let secret_key = env_nonempty("BACKUP_S3_SECRET_KEY");
        let region = env_nonempty("BACKUP_S3_REGION").unwrap_or_else(|| "auto".to_string());

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

        let schedule = schedule_from_env(s3_client.is_some());

        Self {
            s3_client,
            bucket,
            database_url,
            schedule,
        }
    }

    pub fn is_configured(&self) -> bool {
        self.s3_client.is_some()
    }

    /// The scheduled-backup settings, or `None` when no backup runs on its own.
    pub fn schedule(&self) -> Option<BackupSchedule> {
        self.schedule
    }

    /// Create a backup and upload to S3
    pub async fn create_backup(&self) -> Result<String, String> {
        let client = self
            .s3_client
            .as_ref()
            .ok_or("Backup service not configured")?;

        let filename = backup_filename(Utc::now());

        info!("Creating database backup: {}", filename);

        // Stream pg_dump stdout through gzip onto disk so the API process never
        // holds the raw dump and the compressed copy together in RSS.
        let tmp_path = std::env::temp_dir().join(format!("{filename}.{}", uuid::Uuid::new_v4()));
        let _guard = DeleteOnDrop(tmp_path.clone());
        stream_pg_dump_to_gzip_file(&self.database_url, &tmp_path).await?;

        let compressed_len = std::fs::metadata(&tmp_path).map(|m| m.len()).unwrap_or(0);
        info!("Backup compressed: {} bytes", compressed_len);

        let body = ByteStream::from_path(&tmp_path)
            .await
            .map_err(|e| format!("Failed to read compressed backup: {}", e))?;

        client
            .put_object()
            .bucket(&self.bucket)
            .key(&filename)
            .body(body)
            .content_type("application/gzip")
            .send()
            .await
            .map_err(|e| format!("Failed to upload backup: {}", e))?;

        info!("Backup uploaded successfully: {}", filename);
        Ok(filename)
    }

    /// List available backups, newest first. Only objects named the way
    /// `create_backup` names them are listed.
    pub async fn list_backups(&self) -> Result<Vec<String>, String> {
        let client = self
            .s3_client
            .as_ref()
            .ok_or("Backup service not configured")?;

        let client = client.clone();
        let bucket = self.bucket.clone();
        collect_backup_keys(move |token| {
            let client = client.clone();
            let bucket = bucket.clone();
            async move {
                let mut req = client
                    .list_objects_v2()
                    .bucket(bucket)
                    .prefix(BACKUP_PREFIX);
                if let Some(token) = token {
                    req = req.continuation_token(token);
                }
                let response = req
                    .send()
                    .await
                    .map_err(|e| format!("Failed to list backups: {}", e))?;
                Ok(ObjectListPage {
                    keys: response
                        .contents()
                        .iter()
                        .filter_map(|obj| obj.key().map(|k| k.to_string()))
                        .collect(),
                    is_truncated: response.is_truncated() == Some(true),
                    next_continuation_token: response.next_continuation_token().map(str::to_string),
                })
            }
        })
        .await
        .map(newest_first)
    }

    /// Delete all but the `keep_count` newest backups. Returns how many were
    /// deleted. `keep_count` must be at least 1: 0 would delete every backup.
    pub async fn cleanup_old_backups(&self, keep_count: usize) -> Result<usize, String> {
        if keep_count == 0 {
            return Err("keep must be at least 1".to_string());
        }
        let client = self
            .s3_client
            .as_ref()
            .ok_or("Backup service not configured")?;

        let to_delete = keys_to_prune(self.list_backups().await?, keep_count);

        let mut deleted_count = 0;
        for key in &to_delete {
            match client
                .delete_object()
                .bucket(&self.bucket)
                .key(key)
                .send()
                .await
            {
                Ok(_) => deleted_count += 1,
                Err(e) => error!("Failed to delete old backup {}: {}", key, e),
            }
        }

        info!("Cleaned up {} old backups", deleted_count);
        if deleted_count < to_delete.len() {
            return Err(format!(
                "deleted {} of {} old backups; the rest could not be deleted",
                deleted_count,
                to_delete.len()
            ));
        }
        Ok(deleted_count)
    }

    /// One scheduled pass: take a backup, then prune down to `keep`. The prune
    /// only follows an upload that succeeded, so a failing dump never eats the
    /// copies already in the bucket.
    pub async fn run_scheduled(&self, keep: usize) -> Result<ScheduledRun, String> {
        let filename = self.create_backup().await?;
        let pruned = self
            .cleanup_old_backups(keep)
            .await
            .map_err(|e| format!("{filename} was uploaded, but pruning old backups failed: {e}"))?;
        Ok(ScheduledRun { filename, pruned })
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
            schedule: None, // No client, so nothing to schedule.
        }
    }
}

/// What one scheduled pass did.
#[derive(Debug)]
pub struct ScheduledRun {
    pub filename: String,
    pub pruned: usize,
}

/// Run scheduled backups for the life of the process. Does nothing unless the
/// service is configured and `BACKUP_INTERVAL_HOURS` is not 0.
///
/// The first backup is one interval after startup, not at startup: a container
/// that crash-loops must not start a `pg_dump` and an upload on every boot.
/// The task ends when `shutdown` flips to true.
pub fn spawn_backup_task(backup: Arc<BackupService>, shutdown: watch::Receiver<bool>) {
    let Some(schedule) = backup.schedule() else {
        if backup.is_configured() {
            info!("Scheduled backups disabled (BACKUP_INTERVAL_HOURS=0)");
        }
        return;
    };

    info!(
        "Scheduled backups enabled: every {} h, keeping the newest {}; the first run is {} h after startup",
        schedule.interval_hours, schedule.keep, schedule.interval_hours
    );

    tokio::spawn(run_schedule(schedule.interval(), shutdown, move || {
        let backup = backup.clone();
        async move { backup.run_scheduled(schedule.keep).await }
    }));
}

/// Call `job` every `period`, starting one `period` from now, until `shutdown`
/// flips to true. A tick that arrives while the previous run is still going
/// starts nothing, so runs never overlap.
async fn run_schedule<F, Fut>(period: Duration, mut shutdown: watch::Receiver<bool>, job: F)
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<ScheduledRun, String>> + Send + 'static,
{
    let mut ticker = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
    // After a stall (a suspended host) wait for the next regular tick instead of
    // firing the missed ones back to back.
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut slot = RunSlot::default();
    loop {
        tokio::select! {
            _ = ticker.tick() => {}
            // A dropped sender (`Err`) also ends the wait.
            _ = shutdown.wait_for(|stopping| *stopping) => break,
        }
        if !slot.start_unless_busy(|| log_outcome(job())) {
            warn!("Scheduled backup skipped: the previous run is still going");
        }
    }
    // Nothing is uploaded after shutdown begins. A `pg_dump` already running
    // finishes on its own.
    slot.abort();
}

async fn log_outcome(run: impl Future<Output = Result<ScheduledRun, String>>) {
    let started = std::time::Instant::now();
    match run.await {
        Ok(done) => info!(
            "Scheduled backup {} uploaded in {:.1}s; removed {} older backups",
            done.filename,
            started.elapsed().as_secs_f32(),
            done.pruned
        ),
        Err(e) => error!(
            "Scheduled backup failed after {:.1}s: {}",
            started.elapsed().as_secs_f32(),
            e
        ),
    }
}

/// The scheduled run in flight, if any.
#[derive(Default)]
struct RunSlot(Option<JoinHandle<()>>);

impl RunSlot {
    /// Start `run` unless the previous one has not finished. Returns whether
    /// it started.
    fn start_unless_busy<Fut>(&mut self, run: impl FnOnce() -> Fut) -> bool
    where
        Fut: Future<Output = ()> + Send + 'static,
    {
        if self.0.as_ref().is_some_and(|run| !run.is_finished()) {
            return false;
        }
        self.0 = Some(tokio::spawn(run()));
        true
    }

    fn abort(&mut self) {
        if let Some(run) = self.0.take() {
            run.abort();
        }
    }
}

/// The object name for a backup taken at `at`. The timestamp is UTC and fixed
/// width, so name order is age order.
fn backup_filename(at: DateTime<Utc>) -> String {
    format!("{BACKUP_PREFIX}{}.sql.gz", at.format("%Y%m%d_%H%M%S"))
}

/// Whether `key` is a name `backup_filename` produces. A bucket can hold other
/// objects under the same prefix, and those are not ours to list or delete.
fn is_backup_key(key: &str) -> bool {
    let Some(stamp) = key
        .strip_prefix(BACKUP_PREFIX)
        .and_then(|rest| rest.strip_suffix(".sql.gz"))
    else {
        return false;
    };
    // YYYYMMDD_HHMMSS
    stamp.len() == 15
        && stamp.bytes().enumerate().all(|(i, b)| {
            if i == 8 {
                b == b'_'
            } else {
                b.is_ascii_digit()
            }
        })
}

/// The backups among `keys`, newest first.
fn newest_first(mut keys: Vec<String>) -> Vec<String> {
    keys.retain(|key| is_backup_key(key));
    keys.sort_unstable_by(|a, b| b.cmp(a));
    keys
}

/// The backups to delete so that the `keep` newest remain.
fn keys_to_prune(keys: Vec<String>, keep: usize) -> Vec<String> {
    let mut keys = newest_first(keys);
    keys.split_off(keep.min(keys.len()))
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

struct DeleteOnDrop(PathBuf);

impl Drop for DeleteOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Gzip `reader` onto `dest` in bounded chunks so the raw dump never sits in RSS.
fn gzip_chunked_copy(mut reader: impl Read, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::create(dest)
        .map_err(|e| format!("Failed to create temp backup file: {e}"))?;
    let mut encoder = GzEncoder::new(file, Compression::default());
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("Failed to read dump: {e}"))?;
        if n == 0 {
            break;
        }
        encoder
            .write_all(&buf[..n])
            .map_err(|e| format!("Failed to compress backup: {e}"))?;
        encoder
            .flush()
            .map_err(|e| format!("Failed to flush compressed backup: {e}"))?;
    }
    encoder
        .finish()
        .map_err(|e| format!("Failed to finish compression: {e}"))?;
    Ok(())
}

async fn stream_pg_dump_to_gzip_file(database_url: &str, dest: &Path) -> Result<(), String> {
    let dest = dest.to_path_buf();
    let database_url = database_url.to_string();
    tokio::task::spawn_blocking(move || {
        let mut cmd = pg_dump_command(&database_url);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to run pg_dump: {e}"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "pg_dump stdout not piped".to_string())?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| "pg_dump stderr not piped".to_string())?;

        let stderr_thread = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = stderr.read_to_end(&mut buf);
            buf
        });

        let compress_result = gzip_chunked_copy(stdout, &dest);
        if compress_result.is_err() {
            let _ = child.kill();
        }
        let status = child
            .wait()
            .map_err(|e| format!("Failed to wait for pg_dump: {e}"))?;
        let stderr_bytes = stderr_thread.join().unwrap_or_default();
        if !status.success() {
            return Err(format!(
                "pg_dump failed: {}",
                String::from_utf8_lossy(&stderr_bytes)
            ));
        }
        compress_result
    })
    .await
    .map_err(|e| format!("Backup task failed: {e}"))?
}

struct ObjectListPage {
    keys: Vec<String>,
    is_truncated: bool,
    next_continuation_token: Option<String>,
}

/// Walk `list_objects_v2` pages until S3 reports the listing is complete.
async fn collect_backup_keys<F, Fut>(mut fetch_page: F) -> Result<Vec<String>, String>
where
    F: FnMut(Option<String>) -> Fut,
    Fut: std::future::Future<Output = Result<ObjectListPage, String>>,
{
    let mut backups = Vec::new();
    let mut continuation = None;
    loop {
        let page = fetch_page(continuation).await?;
        backups.extend(page.keys);
        continuation = if page.is_truncated {
            page.next_continuation_token
        } else {
            None
        };
        if continuation.is_none() {
            break;
        }
    }
    Ok(backups)
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
        assert!(
            !joined.contains("p%40ss"),
            "encoded password on argv: {joined}"
        );
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

    #[cfg(unix)]
    #[test]
    fn gzip_emits_output_before_reader_eof() {
        use std::io::Write;
        use std::os::unix::net::UnixStream;
        use std::time::{Duration, Instant};

        let dest =
            std::env::temp_dir().join(format!("opn-gzip-stream-{}.gz", uuid::Uuid::new_v4()));
        let dest_for_thread = dest.clone();
        let _guard = DeleteOnDrop(dest.clone());

        let (mut tx, rx) = UnixStream::pair().expect("unix socket pair");
        let handle = std::thread::spawn(move || gzip_chunked_copy(rx, &dest_for_thread));

        let chunk = vec![b'X'; 256 * 1024];
        tx.write_all(&chunk).expect("write dump chunk");
        tx.flush().expect("flush dump chunk");

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if dest.metadata().map(|m| m.len() > 0).unwrap_or(false) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "gzip waited for the full dump before writing; would double RSS"
            );
            std::thread::sleep(Duration::from_millis(10));
        }

        drop(tx);
        handle.join().expect("gzip thread").expect("gzip copy");
    }

    #[tokio::test]
    async fn lists_backups_across_s3_pages() {
        let keys = collect_backup_keys(|token| async move {
            Ok(match token.as_deref() {
                None => ObjectListPage {
                    keys: (0..1000).map(|i| format!("backup_old_{i:04}")).collect(),
                    is_truncated: true,
                    next_continuation_token: Some("page2".into()),
                },
                Some("page2") => ObjectListPage {
                    keys: (0..500).map(|i| format!("backup_new_{i:04}")).collect(),
                    is_truncated: false,
                    next_continuation_token: None,
                },
                other => panic!("unexpected continuation token {other:?}"),
            })
        })
        .await
        .unwrap();

        assert_eq!(
            keys.len(),
            1500,
            "must walk every list_objects_v2 page, not stop at 1000"
        );
        assert!(
            keys.iter().any(|k| k.starts_with("backup_new_")),
            "newest keys live on the second page and must be listed"
        );
    }

    #[test]
    fn schedule_runs_only_when_configured_with_a_nonzero_interval() {
        assert_eq!(
            BackupSchedule::resolve(true, 24, 14),
            Some(BackupSchedule {
                interval_hours: 24,
                keep: 14
            })
        );
        assert_eq!(BackupSchedule::resolve(true, 0, 14), None, "interval 0");
        assert_eq!(BackupSchedule::resolve(false, 24, 14), None, "no storage");
        assert_eq!(BackupSchedule::resolve(false, 0, 14), None);
    }

    #[test]
    fn schedule_never_prunes_down_to_nothing() {
        let schedule = BackupSchedule::resolve(true, 6, 0).expect("scheduled");
        assert_eq!(schedule.keep, 1);
        assert_eq!(schedule.interval(), Duration::from_secs(6 * 60 * 60));
    }

    #[test]
    fn schedule_settings_come_from_the_environment() {
        use crate::utils::test_env::{remove_env, set_env};

        remove_env("BACKUP_INTERVAL_HOURS");
        remove_env("BACKUP_KEEP");
        assert_eq!(
            schedule_from_env(true),
            Some(BackupSchedule {
                interval_hours: 24,
                keep: 14
            }),
            "defaults"
        );
        assert_eq!(schedule_from_env(false), None, "storage not configured");

        // Compose passes an option left alone as an empty string.
        set_env("BACKUP_INTERVAL_HOURS", "");
        set_env("BACKUP_KEEP", "");
        assert_eq!(schedule_from_env(true).map(|s| s.interval_hours), Some(24));

        set_env("BACKUP_INTERVAL_HOURS", "6");
        set_env("BACKUP_KEEP", "3");
        assert_eq!(
            schedule_from_env(true),
            Some(BackupSchedule {
                interval_hours: 6,
                keep: 3
            })
        );

        set_env("BACKUP_KEEP", "0");
        assert_eq!(schedule_from_env(true).map(|s| s.keep), Some(1));

        set_env("BACKUP_INTERVAL_HOURS", "0");
        assert_eq!(schedule_from_env(true), None, "0 turns scheduling off");

        remove_env("BACKUP_INTERVAL_HOURS");
        remove_env("BACKUP_KEEP");
    }

    #[test]
    fn backup_names_round_trip_and_foreign_names_do_not_match() {
        let at = DateTime::parse_from_rfc3339("2026-10-11T03:04:05Z")
            .unwrap()
            .with_timezone(&Utc);
        let name = backup_filename(at);
        assert_eq!(name, "backup_20261011_030405.sql.gz");
        assert!(is_backup_key(&name));

        for foreign in [
            "backup_pre-migration.sql.gz",
            "backup_20261011_030405.sql",
            "backup_20261011_030405.sql.gz.tmp",
            "backup_20261011-030405.sql.gz",
            "backup_2026101_1030405.sql.gz",
            "backup_20261011_03040.sql.gz",
            "backup_20261011_0304050.sql.gz",
            "backup_",
            "notes.txt",
            "dir/backup_20261011_030405.sql.gz",
            "",
        ] {
            assert!(!is_backup_key(foreign), "{foreign:?} is not a backup name");
        }
    }

    fn names(days: std::ops::RangeInclusive<u32>) -> Vec<String> {
        days.map(|day| format!("backup_202610{day:02}_030000.sql.gz"))
            .collect()
    }

    #[test]
    fn prune_keeps_exactly_the_newest_n() {
        // Shuffled on purpose: S3 lists in key order, but nothing here may
        // rely on the input order.
        let mut keys = names(1..=20);
        keys.reverse();
        keys.swap(3, 11);

        let pruned = keys_to_prune(keys, 14);
        assert_eq!(pruned.len(), 6);
        let mut pruned_sorted = pruned.clone();
        pruned_sorted.sort();
        assert_eq!(pruned_sorted, names(1..=6), "the six oldest go");
    }

    #[test]
    fn prune_with_fewer_backups_than_keep_deletes_nothing() {
        assert!(keys_to_prune(names(1..=3), 14).is_empty());
        assert!(keys_to_prune(names(1..=14), 14).is_empty());
        assert!(keys_to_prune(Vec::new(), 1).is_empty());
    }

    #[test]
    fn prune_to_one_leaves_only_the_newest() {
        let pruned = keys_to_prune(names(1..=5), 1);
        assert_eq!(pruned.len(), 4);
        assert!(!pruned.contains(&"backup_20261005_030000.sql.gz".to_string()));
    }

    #[test]
    fn prune_never_touches_or_counts_foreign_objects() {
        let mut keys = names(1..=4);
        // A hand-named copy sorts after every timestamped name, so counting it
        // as a backup would make it the "newest" and push a real one out.
        keys.push("backup_pre-migration.sql.gz".to_string());
        keys.push("backup_zzz".to_string());

        let pruned = keys_to_prune(keys.clone(), 2);
        assert_eq!(
            pruned.len(),
            2,
            "two real backups beyond keep; foreign objects are not counted"
        );
        assert!(pruned.iter().all(|key| is_backup_key(key)));
        assert!(!pruned.contains(&"backup_pre-migration.sql.gz".to_string()));

        let listed = newest_first(keys);
        assert_eq!(
            listed,
            ["04", "03", "02", "01"].map(|d| format!("backup_202610{d}_030000.sql.gz"))
        );
    }

    /// A fake backup run that records how many are in flight at once and holds
    /// each one until the test releases it.
    #[derive(Clone, Default)]
    struct FakeRuns {
        started: Arc<std::sync::atomic::AtomicUsize>,
        in_flight: Arc<std::sync::atomic::AtomicUsize>,
        max_in_flight: Arc<std::sync::atomic::AtomicUsize>,
        release: Arc<tokio::sync::Notify>,
    }

    /// Counts a run as in flight until it finishes or is dropped.
    struct InFlight(Arc<std::sync::atomic::AtomicUsize>);

    impl Drop for InFlight {
        fn drop(&mut self) {
            self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    impl FakeRuns {
        async fn run(self) -> Result<ScheduledRun, String> {
            use std::sync::atomic::Ordering::SeqCst;
            self.started.fetch_add(1, SeqCst);
            let now = self.in_flight.fetch_add(1, SeqCst) + 1;
            self.max_in_flight.fetch_max(now, SeqCst);
            let _flight = InFlight(self.in_flight.clone());
            self.release.notified().await;
            Ok(ScheduledRun {
                filename: "backup_20261011_030000.sql.gz".to_string(),
                pruned: 0,
            })
        }

        fn started(&self) -> usize {
            self.started.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    async fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !condition() {
            assert!(std::time::Instant::now() < deadline, "timed out: {what}");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    #[tokio::test]
    async fn a_tick_while_a_run_is_going_starts_nothing() {
        let runs = FakeRuns::default();
        let mut slot = RunSlot::default();

        assert!(slot.start_unless_busy(|| log_outcome(runs.clone().run())));
        wait_until("first run to start", || runs.started() == 1).await;

        // The next tick arrives while the first run is still going.
        assert!(!slot.start_unless_busy(|| log_outcome(runs.clone().run())));
        assert!(!slot.start_unless_busy(|| log_outcome(runs.clone().run())));
        tokio::task::yield_now().await;
        assert_eq!(runs.started(), 1);

        // Once it finishes, the next tick starts a run again.
        runs.release.notify_one();
        let handle = slot.0.take().expect("run handle");
        handle.await.expect("run finishes");
        assert!(slot.start_unless_busy(|| log_outcome(runs.clone().run())));
        wait_until("second run to start", || runs.started() == 2).await;
        slot.abort();
    }

    #[tokio::test]
    async fn the_first_backup_is_one_interval_after_startup_not_at_startup() {
        let runs = FakeRuns::default();
        let (stop, shutdown) = watch::channel(false);
        let task = tokio::spawn(run_schedule(Duration::from_millis(500), shutdown, {
            let runs = runs.clone();
            move || runs.clone().run()
        }));

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(runs.started(), 0, "a restarting container must not back up");

        wait_until("the first run, one interval in", || runs.started() == 1).await;
        let _ = stop.send(true);
        tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .expect("the task stops on shutdown")
            .expect("no panic");
    }

    #[tokio::test]
    async fn the_schedule_repeats_without_overlap() {
        use std::sync::atomic::Ordering::SeqCst;

        let runs = FakeRuns::default();
        let (stop, shutdown) = watch::channel(false);
        let task = tokio::spawn(run_schedule(Duration::from_millis(60), shutdown, {
            let runs = runs.clone();
            move || runs.clone().run()
        }));

        wait_until("first scheduled run", || runs.started() == 1).await;
        // Several more ticks arrive while that run is held open.
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert_eq!(runs.started(), 1, "ticks during a run start nothing");

        runs.release.notify_one();
        wait_until("a second run after the first finished", || {
            runs.started() == 2
        })
        .await;
        assert_eq!(runs.max_in_flight.load(SeqCst), 1, "runs never overlap");

        let _ = stop.send(true);
        tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .expect("the task stops on shutdown")
            .expect("no panic");
    }

    #[tokio::test]
    async fn shutdown_stops_the_schedule_and_abandons_a_run_in_flight() {
        let runs = FakeRuns::default();
        let (stop, shutdown) = watch::channel(false);
        let task = tokio::spawn(run_schedule(Duration::from_millis(20), shutdown, {
            let runs = runs.clone();
            move || runs.clone().run()
        }));
        wait_until("a run to start", || runs.started() == 1).await;

        let _ = stop.send(true);
        tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .expect("the task stops while a run is held open")
            .expect("no panic");

        wait_until("the abandoned run to be dropped", || {
            runs.in_flight.load(std::sync::atomic::Ordering::SeqCst) == 0
        })
        .await;
        let started = runs.started();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(runs.started(), started, "no run starts after shutdown");
    }
}
