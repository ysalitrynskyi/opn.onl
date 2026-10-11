//! Log file setup.

use tracing_appender::rolling::{InitError, RollingFileAppender, Rotation};

/// Daily log files kept when `LOG_MAX_FILES` is unset.
pub const DEFAULT_LOG_MAX_FILES: usize = 14;

const LOG_FILE_PREFIX: &str = "opn-onl.log";

/// The log file in `dir`: a new `opn-onl.log.<date>` each day, with the oldest
/// removed once there are more than `max_files` of them (0 keeps every file).
/// Without a limit the files pile up on the volume that also holds the
/// database, one more per day, for as long as the container lives.
pub fn daily_log_appender(dir: &str, max_files: usize) -> Result<RollingFileAppender, InitError> {
    RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX)
        .max_log_files(max_files)
        .build(dir)
}

#[cfg(test)]
mod tests {
    use super::daily_log_appender;
    use std::time::Duration;

    fn log_files(dir: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    /// Writes `days` stale log files, oldest first. The appender picks what to
    /// delete by creation time, so each file gets a moment of its own.
    fn dir_with_stale_logs(label: &str, days: u32) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("opn-log-test-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for day in 1..=days {
            std::fs::write(dir.join(format!("opn-onl.log.2020-01-{day:02}")), b"old\n").unwrap();
            std::thread::sleep(Duration::from_millis(25));
        }
        dir
    }

    #[test]
    fn old_log_files_are_removed_beyond_the_limit() {
        let dir = dir_with_stale_logs("bounded", 5);

        let appender = daily_log_appender(dir.to_str().unwrap(), 3).unwrap();
        drop(appender);

        // Today's file plus the two newest stale ones.
        let files = log_files(&dir);
        assert_eq!(files.len(), 3, "{files:?}");
        assert!(
            files.contains(&"opn-onl.log.2020-01-04".to_string()),
            "{files:?}"
        );
        assert!(
            files.contains(&"opn-onl.log.2020-01-05".to_string()),
            "{files:?}"
        );
        assert!(
            !files.contains(&"opn-onl.log.2020-01-01".to_string()),
            "{files:?}"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_limit_of_zero_keeps_every_file() {
        let dir = dir_with_stale_logs("unbounded", 4);

        let appender = daily_log_appender(dir.to_str().unwrap(), 0).unwrap();
        drop(appender);

        assert_eq!(log_files(&dir).len(), 5, "4 old files and today's");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn files_that_are_not_ours_are_left_alone() {
        let dir = dir_with_stale_logs("foreign", 3);
        std::fs::write(dir.join("notes.txt"), b"keep me").unwrap();

        let appender = daily_log_appender(dir.to_str().unwrap(), 2).unwrap();
        drop(appender);

        assert!(log_files(&dir).contains(&"notes.txt".to_string()));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
