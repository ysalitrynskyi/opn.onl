use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // click_events is the highest-volume table. Its serial `id` was
        // integer, so inserts stop at 2,147,483,647. Widen the column and
        // its sequence to bigint.
        //
        // Lock / cost:
        // - ALTER COLUMN TYPE integer → bigint rewrites every row (4 bytes
        //   to 8) and rebuilds the primary-key index. It takes ACCESS
        //   EXCLUSIVE on `click_events` for the whole rewrite: all reads
        //   and writes block. On a large table this is minutes to hours
        //   and needs disk for a full copy plus the new index.
        // - ALTER SEQUENCE AS bigint is a catalog change (brief) and
        //   raises maxvalue from 2^31-1 to 2^63-1 so nextval can pass the
        //   old integer ceiling.
        // SeaORM wraps each migration in a transaction, so this cannot be
        // split into a CONCURRENTLY rebuild.
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE click_events ALTER COLUMN id TYPE bigint; \
                 ALTER SEQUENCE click_events_id_seq AS bigint;",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ACCESS EXCLUSIVE rewrite back to integer. Fails if any `id` is
        // greater than 2,147,483,647 — do not down() a live table that has
        // already passed the old ceiling.
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER SEQUENCE click_events_id_seq AS integer; \
                 ALTER TABLE click_events ALTER COLUMN id TYPE integer;",
            )
            .await?;
        Ok(())
    }
}
