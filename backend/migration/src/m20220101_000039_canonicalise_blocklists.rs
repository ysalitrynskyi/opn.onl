use std::collections::HashMap;

use sea_orm::{ConnectionTrait, DbBackend, Statement};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        canonicalise_blocklists(manager.get_connection()).await
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // The spellings that were typed are not kept, and the canonical ones
        // remain valid, so `down` is a no-op.
        Ok(())
    }
}

/// Rewrite every blocklist row into the canonical spelling the app now writes
/// and looks up.
///
/// * `blocked_links.url`: `url_policy::canonical_blocked_url` in the backend.
/// * `blocked_domains.domain`, `blocked_email_domains.domain`: lower-case
///   punycode, no trailing dot.
///
/// The app compares those columns by equality, so a row written before the rules
/// existed (`HTTPS://Evil.org/p/#x`, `bücher.de`) would otherwise never match
/// again. Rows that collapse onto the same value keep the lowest id (the UNIQUE
/// indexes allow one). A row that has no canonical form is left alone.
///
/// The two canonicalisers below are a frozen copy of the backend's. A migration
/// must not change meaning when the app's rules do, and this crate cannot depend
/// on the backend. A backend test pins the two together.
pub async fn canonicalise_blocklists<C: ConnectionTrait>(db: &C) -> Result<(), DbErr> {
    canonicalise_column(db, "blocked_links", "url", canonical_blocked_url).await?;
    canonicalise_column(db, "blocked_domains", "domain", canonical_host).await?;
    canonicalise_column(db, "blocked_email_domains", "domain", canonical_host).await
}

async fn canonicalise_column<C: ConnectionTrait>(
    db: &C,
    table: &str,
    column: &str,
    canonical: fn(&str) -> Option<String>,
) -> Result<(), DbErr> {
    let rows = db
        .query_all(Statement::from_string(
            DbBackend::Postgres,
            format!("SELECT id, {column} FROM {table} ORDER BY id"),
        ))
        .await?;

    let mut kept: HashMap<String, i32> = HashMap::new();
    let mut duplicates: Vec<i32> = Vec::new();
    let mut rewrites: Vec<(i32, String)> = Vec::new();
    for row in rows {
        let id: i32 = row.try_get("", "id")?;
        let current: String = row.try_get("", column)?;
        let target = canonical(&current).unwrap_or_else(|| current.clone());
        if kept.contains_key(&target) {
            duplicates.push(id);
            continue;
        }
        if target != current {
            rewrites.push((id, target.clone()));
        }
        kept.insert(target, id);
    }

    // Duplicates go first so no rewrite lands on a value another row still holds.
    for id in duplicates {
        db.execute(Statement::from_sql_and_values(
            DbBackend::Postgres,
            format!("DELETE FROM {table} WHERE id = $1"),
            [id.into()],
        ))
        .await?;
    }
    for (id, value) in rewrites {
        db.execute(Statement::from_sql_and_values(
            DbBackend::Postgres,
            format!("UPDATE {table} SET {column} = $1 WHERE id = $2"),
            [value.into(), id.into()],
        ))
        .await?;
    }
    Ok(())
}

/// Lower-case punycode host without a trailing dot.
pub fn canonical_host(host: &str) -> Option<String> {
    let trimmed = host.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        return None;
    }
    match url::Host::parse(trimmed) {
        Ok(parsed) => Some(parsed.to_string()),
        Err(_) => Some(trimmed.to_ascii_lowercase()),
    }
}

/// Canonical `http(s)` URL: lower-case scheme and canonical host; default port,
/// user info and fragment dropped; trailing slashes dropped from the path
/// (an empty path is `/`); the query kept unless it is empty.
pub fn canonical_blocked_url(url: &str) -> Option<String> {
    let mut parsed = url::Url::parse(url.trim()).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }

    parsed.set_fragment(None);
    parsed.set_username("").ok()?;
    parsed.set_password(None).ok()?;
    let host = canonical_host(parsed.host_str()?)?;
    parsed.set_host(Some(&host)).ok()?;

    let query = parsed.query().filter(|q| !q.is_empty()).map(str::to_owned);
    parsed.set_query(None);
    let path = match parsed.path().trim_end_matches('/') {
        "" => "/".to_string(),
        trimmed => trimmed.to_string(),
    };
    parsed.set_path(&path);

    let mut canonical = parsed.to_string();
    if let Some(query) = query {
        canonical.push('?');
        canonical.push_str(&query);
    }
    Some(canonical)
}
