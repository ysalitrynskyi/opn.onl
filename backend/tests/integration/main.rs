//! The backend integration tests, built as one test binary so the app is
//! linked once instead of once per file. Every test here runs in parallel with
//! all the others on the fixed environment of `common::baseline_env`.
//!
//! A test that changes an environment variable, or sweeps the whole database
//! (a purge, a retention scrub), belongs in `integration_serial` instead. The
//! files left at the top of `tests/` are binaries of their own for reasons
//! their headers give (a global allocator, log capture, settings read once per
//! process).
//!
//! Run one file with `cargo test --test integration admin_tests::`.

#[path = "../common/mod.rs"]
mod common;

mod abuse_guards_tests;
mod abuse_report_tests;
mod account_settings_tests;
mod account_takedown_tests;
mod admin_bootstrap_tests;
mod admin_restore_domain_block_tests;
mod admin_tests;
mod analytics_dashboard_aggregate_tests;
mod analytics_dashboard_today_tests;
mod analytics_tests;
mod api_error_shape_and_limits_tests;
mod api_key_scope_tests;
mod audit_avatar;
mod audit_check_blocked;
mod audit_correctness_regression;
mod audit_link_security_regression;
mod audit_openapi;
mod audit_org_admin_ws_regression;
mod audit_p0_regression;
mod audit_p1_regression;
mod audit_p2_regression;
mod audit_passkey;
mod audit_privacy_regression;
mod audit_profile_url;
mod auth_security_regression;
mod auth_tests;
mod bind_parameter_limit_tests;
mod bio_tests;
mod blocklist_normalisation_tests;
mod click_cap_race_tests;
mod click_flush_batch_tests;
mod data_lifecycle_tests;
mod dead_link_page_tests;
mod domain_abuse_hardening_tests;
mod email_case_folding_tests;
mod email_domain_admin_signin_tests;
mod email_verification_switch_tests;
mod entity_tests;
mod first_admin_race_tests;
mod folders_access_and_move_tests;
mod folders_delete_tests;
mod folders_list_query_tests;
mod geoip_tests;
mod health_redis_tests;
mod id_list_cap_tests;
mod last_admin_delete_tests;
mod link_create_edge_tests;
mod link_create_lifecycle_tests;
mod link_export_stream_tests;
mod link_get_tests;
mod link_health_and_schedule_tests;
mod link_list_paging_tests;
mod link_missing_id_tests;
mod link_stats_series_tests;
mod link_update_semantics_tests;
mod links_list_query_tests;
mod links_tests;
mod mail_background_tests;
mod org_link_access_tests;
mod org_link_reassign_bio_tests;
mod org_list_query_tests;
mod org_membership_tests;
mod org_owner_deletion_policy_tests;
mod org_ownership_race_tests;
mod org_role_and_audit_tests;
mod password_verify_limit_tests;
mod performance_index_tests;
mod preview_inactive_reason_tests;
mod qa_v1_regression;
mod rate_limit_and_buffer_tests;
mod rate_limiter_integration_tests;
mod rate_limiter_tests;
mod real_integration_tests;
mod realtime_limits_tests;
mod redirect_safety_tests;
mod retired_codes_tests;
mod routing_attribution_tests;
mod routing_rules_tests;
mod schema_bigint_tests;
mod security_headers_tests;
mod sparklines_tests;
mod tags_association_tests;
mod tags_list_query_tests;
mod trace_redaction_tests;
mod users_email_reuse_tests;
mod websocket_comprehensive_tests;
