use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Reminders are on for every account, existing ones included; the owner
        // turns them off in Settings. Both columns have a constant default, so
        // neither ALTER rewrites its table.
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE users ADD COLUMN IF NOT EXISTS notify_link_expiry BOOLEAN NOT NULL DEFAULT TRUE",
        )
        .await?;
        // When the reminder for the current `expires_at` was mailed. NULL means
        // none yet; changing the expiry sets it back to NULL.
        db.execute_unprepared(
            "ALTER TABLE links ADD COLUMN IF NOT EXISTS expiry_notice_sent_at TIMESTAMP NULL",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("ALTER TABLE links DROP COLUMN IF EXISTS expiry_notice_sent_at")
            .await?;
        db.execute_unprepared("ALTER TABLE users DROP COLUMN IF EXISTS notify_link_expiry")
            .await?;
        Ok(())
    }
}
