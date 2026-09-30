use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // links.click_count is the per-link aggregate, incremented on every
        // redirect (directly or via the click buffer). integer saturates at
        // 2,147,483,647. Widen to bigint.
        //
        // Lock / cost:
        // - ALTER COLUMN TYPE integer → bigint rewrites every row and
        //   rebuilds indexes that include click_count (none currently, but
        //   the heap rewrite still happens). ACCESS EXCLUSIVE on `links`
        //   for the duration: all reads and writes block. `links` is much
        //   smaller than click_events; expect seconds to a few minutes on
        //   a large instance, not hours — still a table rewrite, so schedule
        //   it with F2 rather than during peak redirect traffic.
        // SeaORM wraps each migration in a transaction.
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE links ALTER COLUMN click_count TYPE bigint;")
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ACCESS EXCLUSIVE rewrite back to integer. Fails if any
        // click_count is greater than 2,147,483,647.
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE links ALTER COLUMN click_count TYPE integer;")
            .await?;
        Ok(())
    }
}
