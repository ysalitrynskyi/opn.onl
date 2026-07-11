//! Tests for the real `links::Model` state logic (is_active / is_deleted /
//! inactive_reason). This file previously defined a LOCAL `Link` struct with a
//! reimplemented `is_active()` and tested that copy — so it passed regardless of
//! what the production model did. It now constructs the real entity model and
//! calls its real methods, so the tests fail if that logic regresses.

use chrono::{Duration, Utc};
use opn_onl_backend::entity::links;

/// A baseline active link. Tests clone this and flip the one field under test.
fn base_link() -> links::Model {
    links::Model {
        id: 1,
        code: "abc123".to_string(),
        original_url: "https://example.com".to_string(),
        user_id: Some(1),
        created_at: Utc::now().naive_utc(),
        click_count: 0,
        expires_at: None,
        password_hash: None,
        title: None,
        notes: None,
        folder_id: None,
        org_id: None,
        starts_at: None,
        max_clicks: None,
        deleted_at: None,
        is_pinned: false,
        burn_after_reading: false,
        burned_at: None,
        safe_link_interstitial: false,
        bio_visible: false,
        bio_position: None,
        bio_label: None,
    }
}

#[test]
fn fresh_link_is_active() {
    assert!(base_link().is_active());
    assert!(!base_link().is_deleted());
    assert_eq!(base_link().inactive_reason(), None);
}

#[test]
fn soft_deleted_link_is_inactive() {
    let link = links::Model {
        deleted_at: Some(Utc::now().naive_utc()),
        ..base_link()
    };
    assert!(link.is_deleted());
    assert!(!link.is_active());
}

#[test]
fn expired_link_is_inactive_with_reason() {
    let link = links::Model {
        expires_at: Some((Utc::now() - Duration::hours(1)).naive_utc()),
        ..base_link()
    };
    assert!(!link.is_active());
    assert_eq!(link.inactive_reason(), Some("Link has expired"));
}

#[test]
fn not_yet_started_link_is_inactive_with_reason() {
    let link = links::Model {
        starts_at: Some((Utc::now() + Duration::hours(1)).naive_utc()),
        ..base_link()
    };
    assert!(!link.is_active());
    assert_eq!(link.inactive_reason(), Some("Link is scheduled to activate later"));
}

#[test]
fn link_within_its_schedule_is_active() {
    let link = links::Model {
        starts_at: Some((Utc::now() - Duration::hours(1)).naive_utc()),
        expires_at: Some((Utc::now() + Duration::hours(1)).naive_utc()),
        ..base_link()
    };
    assert!(link.is_active());
}

#[test]
fn max_clicks_boundary() {
    let reached = links::Model { max_clicks: Some(5), click_count: 5, ..base_link() };
    assert!(!reached.is_active(), "at the cap the link is inactive");

    let under = links::Model { max_clicks: Some(5), click_count: 4, ..base_link() };
    assert!(under.is_active(), "one click below the cap the link is still active");
}

#[test]
fn burned_one_time_link_is_inactive_with_reason() {
    let link = links::Model {
        burn_after_reading: true,
        burned_at: Some(Utc::now().naive_utc()),
        ..base_link()
    };
    assert!(!link.is_active());
    assert_eq!(link.inactive_reason(), Some("This one-time link has already been opened"));
}

#[test]
fn one_time_link_at_cap_reports_burn_reason_over_max_clicks() {
    // burn message takes priority over the generic max-clicks message.
    let link = links::Model {
        burn_after_reading: true,
        max_clicks: Some(1),
        click_count: 1,
        ..base_link()
    };
    assert_eq!(link.inactive_reason(), Some("This one-time link has already been opened"));
}
