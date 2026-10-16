use sea_orm::{ConnectionTrait, DbBackend, Statement};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        cap_uncapped_burn_links(manager.get_connection()).await
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // A repaired row is indistinguishable from a one-time link that was
        // saved with a cap of 1, so `down` cannot undo this.
        Ok(())
    }
}

/// One-time links saved before the writers always stored a cap of 1 redirect
/// forever: the flag with `max_clicks` NULL never reaches the consume path.
///
/// Set that cap. Idempotent, and it covers soft-deleted rows too: restoring
/// one must not bring back an uncapped one-time link. Rows that already have
/// a cap are left alone.
pub async fn cap_uncapped_burn_links<C: ConnectionTrait>(db: &C) -> Result<(), DbErr> {
    db.execute(Statement::from_string(
        DbBackend::Postgres,
        "UPDATE links SET max_clicks = 1 \
         WHERE burn_after_reading AND max_clicks IS NULL",
    ))
    .await?;
    Ok(())
}
