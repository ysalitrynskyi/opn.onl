use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // `GET /links` orders by `is_pinned DESC, created_at DESC, id DESC` and
        // pages with LIMIT/OFFSET. `idx_links_user_created_live` cannot give
        // that order because `is_pinned` leads it, so Postgres sorted every live
        // row of the account (or organization) before returning a page. These
        // indexes match the order and the `deleted_at IS NULL` filter, for the
        // personal list (`user_id`) and the organization list (`org_id`).
        //
        // ShareLock on links for each CREATE INDEX: reads continue, writes
        // block for the duration of the build. CREATE INDEX CONCURRENTLY
        // cannot run inside SeaORM's transaction wrapper.
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS idx_links_user_list_order_live \
                 ON links (user_id, is_pinned DESC, created_at DESC, id DESC) \
                 WHERE deleted_at IS NULL",
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS idx_links_org_list_order_live \
                 ON links (org_id, is_pinned DESC, created_at DESC, id DESC) \
                 WHERE deleted_at IS NULL AND org_id IS NOT NULL",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS idx_links_org_list_order_live")
            .await?;
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS idx_links_user_list_order_live")
            .await?;
        Ok(())
    }
}
