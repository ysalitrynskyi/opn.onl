//! The integration tests that must run alone: they change the process
//! environment (link caps, public URLs, retention windows, backup settings,
//! feature switches turned off) or sweep the whole shared database (purges,
//! retention scrubs) and would see, or delete, other tests' rows.
//!
//! Every test starts with `let _serial = common::serial_guard().await;`, so the
//! tests run one at a time and each starts from the environment the one before
//! it started from. Tests that need neither go in `integration`.
//!
//! Run one file with `cargo test --test integration_serial backup_tests::`.

#[path = "../common/mod.rs"]
mod common;

mod account_purge_settings_tests;
mod account_purge_task_tests;
mod account_purge_tests;
mod analytics_retention_settings_tests;
mod audit_email_masking_tests;
mod backup_tests;
mod cors_tests;
mod deleted_link_purge_task_tests;
mod deleted_link_purge_tests;
mod env_switch_tests;
mod handler_rate_limit_tests;
mod https_redirect_tests;
mod left_org_link_cap_tests;
mod link_cap_settings_tests;
mod link_create_auth_tests;
mod link_expiry_notice_tests;
mod new_features_tests;
mod optional_services_tests;
mod privacy_retention_tests;
mod public_base_url_tests;
mod scale_robustness_tests;
