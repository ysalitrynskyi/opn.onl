use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // links.click_count and links.created_at were created with defaults but
        // without NOT NULL, so the columns are nullable in Postgres while the
        // SeaORM models type them as non-Option (i32 / DateTime). A NULL in
        // either column would break row deserialization. Backfill any NULLs to
        // their defaults (defensive — the app always populates them), then
        // enforce NOT NULL so the schema matches the models.
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE links SET click_count = 0 WHERE click_count IS NULL; \
                 UPDATE links SET created_at = NOW() WHERE created_at IS NULL; \
                 ALTER TABLE links ALTER COLUMN click_count SET NOT NULL; \
                 ALTER TABLE links ALTER COLUMN created_at SET NOT NULL;",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE links ALTER COLUMN click_count DROP NOT NULL; \
                 ALTER TABLE links ALTER COLUMN created_at DROP NOT NULL;",
            )
            .await?;
        Ok(())
    }
}
