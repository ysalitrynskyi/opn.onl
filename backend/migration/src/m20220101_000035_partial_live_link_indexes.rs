use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ShareLock on links for each CREATE INDEX: reads continue, writes
        // block for the duration of the build (proportional to live-row
        // count). CREATE INDEX CONCURRENTLY cannot run inside SeaORM's
        // transaction wrapper.
        //
        // idx_links_user_created_deleted leads with created_at before
        // deleted_at, so `deleted_at IS NULL` is not an equality prefix.
        // These partial indexes match the live dashboard and folder-count
        // filters and omit soft-deleted rows.
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS idx_links_user_created_live \
                 ON links (user_id, created_at DESC) \
                 WHERE deleted_at IS NULL",
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS idx_links_folder_live \
                 ON links (folder_id) \
                 WHERE deleted_at IS NULL",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS idx_links_folder_live")
            .await?;
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS idx_links_user_created_live")
            .await?;
        Ok(())
    }
}
