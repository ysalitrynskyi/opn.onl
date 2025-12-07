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
        ]
    }
}
