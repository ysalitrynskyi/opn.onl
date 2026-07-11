use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // passkeys.cred_id is a credential id unique to one authenticator
        // credential, but the column had no UNIQUE constraint and registration
        // discarded its insert result, so a re-registered credential could land
        // as a duplicate row. Delete any existing duplicates (keep the lowest id)
        // and add a UNIQUE index so the database enforces one row per credential.
        manager
            .get_connection()
            .execute_unprepared(
                "DELETE FROM passkeys a USING passkeys b \
                   WHERE a.id > b.id AND a.cred_id = b.cred_id; \
                 CREATE UNIQUE INDEX IF NOT EXISTS idx_passkeys_cred_id ON passkeys (cred_id);",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS idx_passkeys_cred_id;")
            .await?;
        Ok(())
    }
}
