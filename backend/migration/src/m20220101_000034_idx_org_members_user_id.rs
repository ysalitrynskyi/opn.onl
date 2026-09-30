use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ShareLock on org_members: reads continue, writes block until the
        // build finishes. The table is one row per membership, so the lock
        // is short except on a very large instance.
        // CREATE INDEX CONCURRENTLY cannot run inside SeaORM's transaction.
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS idx_org_members_user_id \
                 ON org_members (user_id)",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS idx_org_members_user_id")
            .await?;
        Ok(())
    }
}
