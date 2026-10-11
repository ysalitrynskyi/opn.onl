use std::collections::BTreeMap;

use sea_orm::{ConnectionTrait, DbBackend, Statement};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// The address as the app stores it from now on (`normalize_email` in the
/// backend, copied here because a migration must not change when the app does):
/// local part lowercased, domain as lowercase punycode with no trailing dot.
/// `None` for an address that is not `local@domain-name`.
fn canonical_email(email: &str) -> Option<String> {
    let (local, domain) = email.trim().rsplit_once('@')?;
    let domain = domain.trim().trim_end_matches('.');
    if domain.contains('%') {
        return None;
    }
    match url::Host::parse(domain).ok()? {
        url::Host::Domain(ascii) => Some(format!("{}@{ascii}", local.trim().to_ascii_lowercase())),
        _ => None,
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Email addresses are now stored and compared in lowercase, with a
        // Unicode domain as punycode. `Bob@x.com` and `bob@x.com` (or
        // `x@bücher.de` and `x@xn--bcher-kva.de`) could both register before,
        // so live rows may be mixed-case and may collide once folded.
        //
        // 1. Lowercase every live ASCII address that no other live ASCII
        //    address shares apart from case. The existing unique index on
        //    `email` cannot object: the rows left out are exactly the ones
        //    that would clash.
        // 2. Do the same for addresses with a Unicode domain, which only Rust
        //    can convert to punycode: rewrite one when no other live account
        //    holds its canonical form, in either spelling.
        // 3. Leave collisions untouched. Which of two accounts owns the
        //    address is the operator's decision, so the app reports them
        //    (startup warning, admin overview) instead of merging or deleting.
        // 4. Add a unique index on `lower(email)` for live rows unless two of
        //    them still share a lowercase form; it could not be built, and
        //    failing here would stop the backend from starting on production
        //    data. (A Unicode address left unfolded differs textually from its
        //    punycode twin, so it does not block the index.) Registration
        //    refuses a variant of a live address on its own, so a skipped
        //    index leaves no hole for new sign-ups.
        //
        // Soft-deleted rows keep their spelling: they are outside the live
        // index and the lookups, and an admin restore folds the address.
        //
        // Lock / cost: the updates only touch rows that change and hold row
        // locks. CREATE UNIQUE INDEX (not CONCURRENTLY: SeaORM wraps each
        // migration in a transaction) takes SHARE on `users`, which blocks
        // writes for a scan of a small table.
        let db = manager.get_connection();

        db.execute_unprepared(
            "UPDATE users SET email = lower(email) \
             WHERE deleted_at IS NULL \
               AND octet_length(email) = length(email) \
               AND email <> lower(email) \
               AND lower(email) NOT IN ( \
                   SELECT lower(email) FROM users \
                   WHERE deleted_at IS NULL AND octet_length(email) = length(email) \
                   GROUP BY lower(email) HAVING COUNT(*) > 1 \
               );",
        )
        .await?;

        let unicode = db
            .query_all(Statement::from_string(
                DbBackend::Postgres,
                "SELECT id, email FROM users \
                 WHERE deleted_at IS NULL AND octet_length(email) <> length(email)",
            ))
            .await?;
        let mut by_canonical: BTreeMap<String, Vec<i32>> = BTreeMap::new();
        for row in unicode {
            let id: i32 = row.try_get("", "id")?;
            let email: String = row.try_get("", "email")?;
            if let Some(canonical) = canonical_email(&email) {
                by_canonical.entry(canonical).or_default().push(id);
            }
        }
        for (canonical, ids) in by_canonical {
            if ids.len() != 1 {
                continue;
            }
            let id = ids[0];
            let held_elsewhere = db
                .query_one(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    "SELECT COUNT(*)::bigint FROM users \
                     WHERE deleted_at IS NULL AND id <> $1 AND lower(email) = $2",
                    [id.into(), canonical.clone().into()],
                ))
                .await?
                .map(|row| row.try_get_by_index::<i64>(0))
                .transpose()?
                .unwrap_or(0);
            if held_elsewhere == 0 {
                db.execute(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    "UPDATE users SET email = $2 WHERE id = $1",
                    [id.into(), canonical.into()],
                ))
                .await?;
            }
        }

        db.execute_unprepared(
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
        // The folding is not reversed: the original spelling is gone, and the
        // folded form is a valid address for the same mailbox.
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS idx_users_email_lower_live")
            .await?;
        Ok(())
    }
}
