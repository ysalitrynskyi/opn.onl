pub use sea_orm_migration::prelude::*;

mod m20220101_000001_create_table;
mod m20220101_000002_create_click_events;
mod m20220101_000003_create_passkeys;
mod m20220101_000004_add_expiration;
mod m20220101_000005_add_password;
mod m20220101_000006_create_organizations;
mod m20220101_000007_create_folders;
mod m20220101_000008_create_tags;
mod m20220101_000009_enhance_links;
mod m20220101_000010_enhance_click_events;
mod m20220101_000011_create_audit_log;
mod m20220101_000012_add_email_verification;
mod m20220101_000013_create_blocked_tables;
mod m20220101_000014_add_passkey_name;
mod m20220101_000015_add_link_title;
mod m20220101_000016_add_user_profile;
mod m20220101_000017_add_performance_indexes;
mod m20220101_000018_add_link_pinned;
mod m20220101_000019_add_token_version;
mod m20220101_000020_add_burn_after_reading;
mod m20220101_000021_add_safe_link_interstitial;
mod m20220101_000022_create_routing_rules;
mod m20220101_000023_add_link_in_bio;
mod m20220101_000024_create_api_keys;
mod m20220101_000025_add_fk_indexes;
mod m20220101_000026_restrict_org_owner_fk;
mod m20220101_000027_links_not_null;
mod m20220101_000028_normalize_blocked_domains;
mod m20220101_000029_passkey_cred_id_unique;
mod m20220101_000030_domain_abuse_controls;
mod m20220101_000031_users_email_partial_unique;
mod m20220101_000032_click_events_id_bigint;
mod m20220101_000033_links_click_count_bigint;
mod m20220101_000034_idx_org_members_user_id;
mod m20220101_000035_partial_live_link_indexes;
mod m20220101_000036_idx_created_at_admin_activity;
mod m20220101_000037_fold_user_email_case;
mod m20220101_000038_retired_codes;
mod m20220101_000039_canonicalise_blocklists;
mod m20220101_000040_api_key_scope_expiry;
mod m20220101_000045_cap_uncapped_burn_links;

// Public so the backend's tests can pin its blocklist canonicalisation to the
// frozen copy this migration applies to existing rows.
pub use m20220101_000039_canonicalise_blocklists::{
    canonical_blocked_url, canonical_host, canonicalise_blocklists,
};
// Public so a test can apply the repair to rows it inserts after startup.
pub use m20220101_000045_cap_uncapped_burn_links::cap_uncapped_burn_links;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_create_table::Migration),
            Box::new(m20220101_000002_create_click_events::Migration),
            Box::new(m20220101_000003_create_passkeys::Migration),
            Box::new(m20220101_000004_add_expiration::Migration),
            Box::new(m20220101_000005_add_password::Migration),
            Box::new(m20220101_000006_create_organizations::Migration),
            Box::new(m20220101_000007_create_folders::Migration),
            Box::new(m20220101_000008_create_tags::Migration),
            Box::new(m20220101_000009_enhance_links::Migration),
            Box::new(m20220101_000010_enhance_click_events::Migration),
            Box::new(m20220101_000011_create_audit_log::Migration),
            Box::new(m20220101_000012_add_email_verification::Migration),
            Box::new(m20220101_000013_create_blocked_tables::Migration),
            Box::new(m20220101_000014_add_passkey_name::Migration),
            Box::new(m20220101_000015_add_link_title::Migration),
            Box::new(m20220101_000016_add_user_profile::Migration),
            Box::new(m20220101_000017_add_performance_indexes::Migration),
            Box::new(m20220101_000018_add_link_pinned::Migration),
            Box::new(m20220101_000019_add_token_version::Migration),
            Box::new(m20220101_000020_add_burn_after_reading::Migration),
            Box::new(m20220101_000021_add_safe_link_interstitial::Migration),
            Box::new(m20220101_000022_create_routing_rules::Migration),
            Box::new(m20220101_000023_add_link_in_bio::Migration),
            Box::new(m20220101_000024_create_api_keys::Migration),
            Box::new(m20220101_000025_add_fk_indexes::Migration),
            Box::new(m20220101_000026_restrict_org_owner_fk::Migration),
            Box::new(m20220101_000027_links_not_null::Migration),
            Box::new(m20220101_000028_normalize_blocked_domains::Migration),
            Box::new(m20220101_000029_passkey_cred_id_unique::Migration),
            Box::new(m20220101_000030_domain_abuse_controls::Migration),
            Box::new(m20220101_000031_users_email_partial_unique::Migration),
            Box::new(m20220101_000032_click_events_id_bigint::Migration),
            Box::new(m20220101_000033_links_click_count_bigint::Migration),
            Box::new(m20220101_000034_idx_org_members_user_id::Migration),
            Box::new(m20220101_000035_partial_live_link_indexes::Migration),
            Box::new(m20220101_000036_idx_created_at_admin_activity::Migration),
            Box::new(m20220101_000037_fold_user_email_case::Migration),
            Box::new(m20220101_000038_retired_codes::Migration),
            Box::new(m20220101_000039_canonicalise_blocklists::Migration),
            Box::new(m20220101_000040_api_key_scope_expiry::Migration),
            Box::new(m20220101_000045_cap_uncapped_burn_links::Migration),
        ]
    }
}
