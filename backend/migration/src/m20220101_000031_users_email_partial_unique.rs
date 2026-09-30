use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // users.email was created as a table-wide UNIQUE constraint
        // (`users_email_key`, from m20220101_000001 `.unique_key()`). Soft
        // delete is an UPDATE of `deleted_at`, so that constraint kept blocking
        // the address after the account was gone: the user could not register
        // it again, and the 409 told an unauthenticated attacker the address
        // still existed. A UNIQUE *constraint* cannot be partial in Postgres;
        // only a unique index can. Drop the constraint (name confirmed against
        // information_schema.table_constraints / pg_indexes) and replace it
        // with a partial unique index on live rows.
        //
        // Lock / cost:
        // - ALTER TABLE ... DROP CONSTRAINT takes ACCESS EXCLUSIVE on `users`
        //   for a brief catalog change (the backing unique index is dropped
        //   with it).
        // - CREATE UNIQUE INDEX (not CONCURRENTLY: SeaORM wraps each
        //   migration in a transaction, and CONCURRENTLY cannot run inside
        //   one) takes SHARE on `users` — blocks writes, allows reads — for
        //   a table scan. `users` is small relative to click_events; expect
        //   milliseconds to a few seconds even at tens of thousands of rows.
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE users DROP CONSTRAINT users_email_key; \
                 CREATE UNIQUE INDEX idx_users_email_live ON users (email) \
                   WHERE deleted_at IS NULL;",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Re-adding UNIQUE (email) takes ACCESS EXCLUSIVE while it builds the
        // constraint index, and will FAIL if any address has already been
        // reused (one deleted row + one live row). Do not run down() on a
        // live database that has reused emails; restore from a backup taken
        // before up() instead.
        manager
            .get_connection()
            .execute_unprepared(
                "DROP INDEX IF EXISTS idx_users_email_live; \
                 ALTER TABLE users ADD CONSTRAINT users_email_key UNIQUE (email);",
            )
            .await?;
        Ok(())
    }
}
