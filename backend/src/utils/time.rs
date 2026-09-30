//! Timestamps leave the API as RFC 3339 in UTC.
//!
//! Every `*_at` column is a naive `TIMESTAMP` holding UTC. `NaiveDateTime`'s
//! `Display` prints `2026-09-30 10:54:42` with no offset, and browsers parse a
//! date-time without an offset as *local* time, so every time the app showed
//! was shifted by the viewer's UTC offset: an evening expiry in New York
//! rendered as the next day, and recent clicks were hours off.

use chrono::{NaiveDateTime, SecondsFormat};

/// `2026-09-30T10:54:42.123456Z`: the stored UTC instant with an explicit `Z`.
pub fn utc_rfc3339(dt: NaiveDateTime) -> String {
    dt.and_utc().to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

#[cfg(test)]
mod tests {
    use super::utc_rfc3339;
    use chrono::NaiveDate;

    #[test]
    fn naive_utc_is_sent_with_an_explicit_zulu_offset() {
        let day = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
        assert_eq!(
            utc_rfc3339(day.and_hms_opt(4, 59, 0).unwrap()),
            "2026-11-15T04:59:00Z"
        );
        assert_eq!(
            utc_rfc3339(day.and_hms_micro_opt(10, 54, 42, 123_456).unwrap()),
            "2026-11-15T10:54:42.123456Z"
        );
    }
}
