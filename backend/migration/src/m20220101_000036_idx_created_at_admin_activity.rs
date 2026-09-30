use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ShareLock on links, then on users. GET /admin/activity groups by
        // created_at::date over a 1..=365 day window; click_events already
        // has idx_click_events_created_at. CREATE INDEX CONCURRENTLY cannot
        // run inside SeaORM's transaction. Build time follows table size.
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS idx_links_created_at \
                 ON links (created_at)",
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS idx_users_created_at \
                 ON users (created_at)",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS idx_users_created_at")
            .await?;
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS idx_links_created_at")
            .await?;
        Ok(())
    }
}
