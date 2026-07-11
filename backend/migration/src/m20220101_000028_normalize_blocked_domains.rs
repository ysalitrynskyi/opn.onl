use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

// Normalized form used by admin::block_domain and links::check_blocked:
// lowercase, then strip a leading scheme, then trailing '/' and '.'. Lowercasing
// first ensures an uppercase scheme (HTTPS://) is also stripped.
const NORM: &str = "rtrim(rtrim(replace(replace(lower(domain), 'https://', ''), 'http://', ''), '/'), '.')";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // check_blocked now matches a visitor's host against blocked_domains via
        // an indexed equality on the normalized `domain` column (host + parent
        // domains), instead of loading the whole table and normalizing each row
        // in Rust. Existing rows must therefore be normalized at rest. Newer rows
        // already are (block_domain normalizes on write), but older ones may carry
        // a scheme / trailing slash / dot / mixed case. Dedup any rows that would
        // collide after normalization (keep the lowest id, which the UNIQUE index
        // on `domain` requires), then normalize the survivors in place.
        let db = manager.get_connection();
        db.execute_unprepared(&format!(
            "DELETE FROM blocked_domains a USING blocked_domains b \
               WHERE a.id > b.id AND {norm_a} = {norm_b}; \
             UPDATE blocked_domains SET domain = {norm} WHERE domain <> {norm};",
            norm = NORM,
            norm_a = NORM.replace("domain", "a.domain"),
            norm_b = NORM.replace("domain", "b.domain"),
        ))
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Normalization is not reversible (the original casing/scheme is lost),
        // and the normalized values remain valid, so `down` is a no-op.
        Ok(())
    }
}
