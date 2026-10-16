use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // The URL a routed click was sent to, so an A/B split or a routing rule
        // has a result. Rule ids change on every save, so the URL itself is
        // kept. NULL means the link's own URL.
        //
        // click_events is the largest table. A nullable column without a
        // default is a catalog-only change: Postgres neither rewrites nor scans
        // the table, existing rows read NULL, and the ACCESS EXCLUSIVE lock is
        // held only for the catalog update. A NOT NULL here would scan every
        // row.
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE click_events ADD COLUMN destination TEXT NULL")
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE click_events DROP COLUMN IF EXISTS destination")
            .await?;
        Ok(())
    }
}
