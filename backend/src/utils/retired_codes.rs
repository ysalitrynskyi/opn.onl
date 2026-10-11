//! Short codes that must never be issued again.
//!
//! A soft-deleted link keeps its row, so the global UNIQUE on `links.code`
//! keeps its code from being reused (`create_link` rejects such an alias with a
//! clear message). Rows that are hard-deleted (an organization purge, an admin
//! hard delete) would release the code, and whoever claimed it next would
//! receive the old link's traffic: printed QR codes, published vanity URLs.
//! Those paths copy the codes here, in the same transaction as the delete, and
//! every path that hands out a code refuses one that is listed.

use sea_orm::{ConnectionTrait, DatabaseBackend, DbErr, EntityTrait, Statement};

use crate::entity::retired_codes;

/// True when `code` was retired. One primary-key lookup.
pub async fn is_retired<C: ConnectionTrait>(db: &C, code: &str) -> Result<bool, DbErr> {
    Ok(retired_codes::Entity::find_by_id(code)
        .one(db)
        .await?
        .is_some())
}

/// Retire the code of every link in an organization, soft-deleted ones
/// included. Call it before the links are deleted.
pub async fn retire_org_codes<C: ConnectionTrait>(db: &C, org_id: i32) -> Result<(), DbErr> {
    retire(
        db,
        "INSERT INTO retired_codes (code) SELECT code FROM links WHERE org_id = $1 \
         ON CONFLICT (code) DO NOTHING",
        org_id,
    )
    .await
}

/// Retire the code of every personal (non-organization) link of a user,
/// soft-deleted ones included. Call it before the links are deleted.
pub async fn retire_personal_codes<C: ConnectionTrait>(db: &C, user_id: i32) -> Result<(), DbErr> {
    retire(
        db,
        "INSERT INTO retired_codes (code) SELECT code FROM links \
         WHERE user_id = $1 AND org_id IS NULL ON CONFLICT (code) DO NOTHING",
        user_id,
    )
    .await
}

async fn retire<C: ConnectionTrait>(db: &C, sql: &str, id: i32) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [id.into()],
    ))
    .await?;
    Ok(())
}
