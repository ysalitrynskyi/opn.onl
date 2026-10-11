//! Whose standing decides whether a short link may still be served.

use sea_orm::sea_query::Query;
use sea_orm::{ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QuerySelect};

use crate::entity::{links, organizations, users};

/// `false` when the account that answers for `link` is disabled or deleted.
///
/// A personal link answers to the account that created it. An org link
/// belongs to the team, so it answers to the org's owner: a disabled ordinary
/// member must not take the team's links down with them. One primary-key
/// lookup either way. Links with no owner at all (anonymous creation) have no
/// account to lose and stay live.
pub async fn link_owner_is_active<C: ConnectionTrait>(
    db: &C,
    link: &links::Model,
) -> Result<bool, DbErr> {
    let owner = match (link.org_id, link.user_id) {
        (Some(org_id), _) => users::Column::Id.in_subquery(
            Query::select()
                .column(organizations::Column::OwnerId)
                .from(organizations::Entity)
                .and_where(organizations::Column::Id.eq(org_id))
                .to_owned(),
        ),
        (None, Some(user_id)) => users::Column::Id.eq(user_id),
        (None, None) => return Ok(true),
    };

    Ok(users::Entity::find()
        .select_only()
        .column(users::Column::Id)
        .filter(owner)
        .filter(users::Column::DeletedAt.is_null())
        .filter(users::Column::DisabledAt.is_null())
        .into_tuple::<i32>()
        .one(db)
        .await?
        .is_some())
}
