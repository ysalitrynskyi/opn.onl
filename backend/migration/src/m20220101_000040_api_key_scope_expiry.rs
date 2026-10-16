use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Existing keys are full and never expire, which is what they could
        // already do. scope is checked here so a bad value cannot be stored
        // and then skipped by a handler.
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE api_keys
                ADD COLUMN scope TEXT NOT NULL DEFAULT 'full',
                ADD COLUMN expires_at TIMESTAMP NULL",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE api_keys
                ADD CONSTRAINT chk_api_keys_scope CHECK (scope IN ('full', 'read_only'))",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("ALTER TABLE api_keys DROP CONSTRAINT IF EXISTS chk_api_keys_scope")
            .await?;
        db.execute_unprepared(
            "ALTER TABLE api_keys DROP COLUMN IF EXISTS scope, DROP COLUMN IF EXISTS expires_at",
        )
        .await?;
        Ok(())
    }
}
