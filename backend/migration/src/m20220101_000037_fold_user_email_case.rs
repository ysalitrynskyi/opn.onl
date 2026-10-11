use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Email addresses are now stored and compared in lowercase, but
        // `Bob@x.com` and `bob@x.com` could both register before, so live rows
        // may be mixed-case and may collide once folded.
        //
        // 1. Lowercase every live address whose lowercase form no other live
        //    account holds. The existing unique index on `email` cannot object:
        //    the rows left out are exactly the ones that would clash.
        // 2. Leave case collisions untouched. Which of two accounts owns the
        //    address is the operator's decision, so the app reports them
        //    (startup warning, admin overview) instead of merging or deleting.
        // 3. Add a unique index on `lower(email)` for live rows only if no
        //    collision is left; otherwise it could not be built, and failing
        //    here would stop the backend from starting on production data.
        //    Registration refuses a case-variant of a live address on its own,
        //    so a skipped index leaves no hole for new sign-ups.
        //
        // Soft-deleted rows keep their spelling: they are outside the live
        // index and the lookups, and an admin restore is checked against it.
        //
        // Lock / cost: the UPDATE only touches mixed-case rows and holds row
        // locks. CREATE UNIQUE INDEX (not CONCURRENTLY: SeaORM wraps each
        // migration in a transaction) takes SHARE on `users`, which blocks
        // writes for a scan of a small table.
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE users SET email = lower(email) \
                 WHERE deleted_at IS NULL \
                   AND email <> lower(email) \
                   AND lower(email) NOT IN ( \
                       SELECT lower(email) FROM users \
                       WHERE deleted_at IS NULL \
                       GROUP BY lower(email) HAVING COUNT(*) > 1 \
                   );",
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "DO $$ \
                 BEGIN \
                     IF EXISTS ( \
                         SELECT 1 FROM users WHERE deleted_at IS NULL \
                         GROUP BY lower(email) HAVING COUNT(*) > 1 \
                     ) THEN \
                         RAISE NOTICE 'idx_users_email_lower_live not created: live accounts share an email address that differs only by letter case'; \
                     ELSE \
                         CREATE UNIQUE INDEX IF NOT EXISTS idx_users_email_lower_live \
                             ON users (lower(email)) WHERE deleted_at IS NULL; \
                     END IF; \
                 END $$;",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // The lowercasing is not reversed: the original spelling is gone, and
        // the lowercase form is a valid address for the same mailbox.
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS idx_users_email_lower_live")
            .await?;
        Ok(())
    }
}
