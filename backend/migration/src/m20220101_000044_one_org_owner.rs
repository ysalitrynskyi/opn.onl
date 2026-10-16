use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // One owner membership per organization. The transfer handler locks the
        // org row and re-checks; this index is the backstop for a write that
        // skips that check. A failed migration takes production down, so rows
        // that already violate it are repaired first and the index is skipped
        // (with a warning) if any duplicate remains.
        //
        // Repair:
        // - When `organizations.owner_id` already has an owner-role membership,
        //   every other owner-role row in that org is demoted to admin.
        // - When it does not, `owner_id` becomes the oldest owner membership
        //   (`joined_at`, then `id`) and the rest are demoted.
        // - An org with no owner-role row is left alone. The index allows zero
        //   owners; it only forbids two.
        //
        // Lock / cost: the updates take row locks on `org_members` and
        // `organizations`. `CREATE UNIQUE INDEX` (not CONCURRENTLY: SeaORM runs
        // each migration inside a transaction, and CONCURRENTLY cannot) takes
        // SHARE on `org_members` for a table scan. The table is one row per
        // membership, far smaller than `click_events`.
        manager
            .get_connection()
            .execute_unprepared(
                "DO $$\n\
                 DECLARE\n\
                   still_duplicate boolean;\n\
                 BEGIN\n\
                   UPDATE org_members AS member\n\
                   SET role = 'admin'\n\
                   FROM organizations AS org\n\
                   WHERE member.org_id = org.id\n\
                     AND member.role = 'owner'\n\
                     AND member.user_id <> org.owner_id\n\
                     AND EXISTS (\n\
                       SELECT 1 FROM org_members AS keeper\n\
                       WHERE keeper.org_id = org.id\n\
                         AND keeper.user_id = org.owner_id\n\
                         AND keeper.role = 'owner'\n\
                     );\n\
                 \n\
                   UPDATE organizations AS org\n\
                   SET owner_id = picked.user_id\n\
                   FROM (\n\
                     SELECT DISTINCT ON (org_id) org_id, user_id\n\
                     FROM org_members\n\
                     WHERE role = 'owner'\n\
                     ORDER BY org_id, joined_at NULLS LAST, id\n\
                   ) AS picked\n\
                   WHERE org.id = picked.org_id\n\
                     AND NOT EXISTS (\n\
                       SELECT 1 FROM org_members AS keeper\n\
                       WHERE keeper.org_id = org.id\n\
                         AND keeper.user_id = org.owner_id\n\
                         AND keeper.role = 'owner'\n\
                     );\n\
                 \n\
                   UPDATE org_members AS member\n\
                   SET role = 'admin'\n\
                   FROM organizations AS org\n\
                   WHERE member.org_id = org.id\n\
                     AND member.role = 'owner'\n\
                     AND member.user_id <> org.owner_id;\n\
                 \n\
                   SELECT EXISTS (\n\
                     SELECT 1 FROM org_members\n\
                     WHERE role = 'owner'\n\
                     GROUP BY org_id\n\
                     HAVING COUNT(*) > 1\n\
                   ) INTO still_duplicate;\n\
                 \n\
                   IF still_duplicate THEN\n\
                     RAISE WARNING 'org_members still has more than one owner per organization; skipping idx_org_members_one_owner';\n\
                   ELSE\n\
                     CREATE UNIQUE INDEX IF NOT EXISTS idx_org_members_one_owner\n\
                       ON org_members (org_id) WHERE role = 'owner';\n\
                   END IF;\n\
                 END $$;",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS idx_org_members_one_owner;")
            .await?;
        Ok(())
    }
}
