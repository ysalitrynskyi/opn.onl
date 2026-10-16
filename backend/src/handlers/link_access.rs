//! Who may act on a link.
//!
//! A personal link (no `org_id`) belongs to its creator and to nobody else.
//! An organization link belongs to the organization: what a caller may do is
//! decided by their CURRENT membership and role, and the creator gets nothing
//! extra from having made it. A creator who was removed from the organization,
//! or demoted to viewer, therefore loses control of its links the moment the
//! membership changes, and the other members can manage what the creator
//! left behind. Folders and tags follow the same rule (`folders.rs`, `tags.rs`).
//!
//! | action | personal link | organization link |
//! |--------|---------------|-------------------|
//! | `Read` | creator | any member |
//! | `Write` | creator | editor and above |
//! | `Clone` | creator | editor and above (the clone lands in the same org) |
//! | `Delete` | creator | admin and above, or the creator while still an editor |

use axum::{Json, http::StatusCode};
use sea_orm::sea_query::{Expr, Query, SelectStatement};
use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, DbErr, EntityTrait,
    PaginatorTrait, QueryFilter,
};
use std::collections::HashMap;

use crate::entity::{links, org_members};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkAction {
    Read,
    Write,
    Clone,
    Delete,
}

impl LinkAction {
    fn denied_message(self) -> &'static str {
        match self {
            LinkAction::Read => "You don't have permission to access this link",
            LinkAction::Write => "You don't have permission to modify this link",
            LinkAction::Clone => "You don't have permission to clone this link",
            LinkAction::Delete => "You don't have permission to delete this link",
        }
    }
}

/// The decision itself, with the caller's membership of the link's
/// organization (if any) already looked up.
fn permits(
    link: &links::Model,
    user_id: i32,
    member: Option<&org_members::Model>,
    action: LinkAction,
) -> bool {
    let is_creator = link.user_id == Some(user_id);
    if link.org_id.is_none() {
        return is_creator;
    }
    let Some(member) = member else {
        return false;
    };
    // `can_edit` / `is_admin` are explicit role allowlists, so an unknown or
    // future role can read but stays read-only until it is defined.
    match action {
        LinkAction::Read => true,
        LinkAction::Write | LinkAction::Clone => member.can_edit(),
        LinkAction::Delete => member.is_admin() || (is_creator && member.can_edit()),
    }
}

fn forbidden(action: LinkAction) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({"error": action.denied_message()})),
    )
}

fn db_error() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": "Database error"})),
    )
}

/// Authorize `user_id` to perform `action` on `link`.
///
/// Refusal is a 403 whether the caller is a stranger, a former member or just
/// too junior, so a response never says more than "not yours". A failed
/// membership lookup is a 500, never a silent allow.
pub async fn authorize_link<C: ConnectionTrait>(
    db: &C,
    user_id: i32,
    link: &links::Model,
    action: LinkAction,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let member = match link.org_id {
        Some(org_id) => org_members::Entity::find()
            .filter(org_members::Column::OrgId.eq(org_id))
            .filter(org_members::Column::UserId.eq(user_id))
            .one(db)
            .await
            .map_err(|_| db_error())?,
        None => None,
    };
    if permits(link, user_id, member.as_ref(), action) {
        Ok(())
    } else {
        Err(forbidden(action))
    }
}

/// Load a live link by id and authorize `user_id` for `action` on it. A missing
/// link and a forbidden one are the same 403, so the answer does not reveal
/// which ids exist.
pub async fn live_link_for(
    db: &DatabaseConnection,
    id: i32,
    user_id: i32,
    action: LinkAction,
) -> Result<links::Model, (StatusCode, Json<serde_json::Value>)> {
    let link = links::Entity::find_by_id(id)
        .filter(links::Column::DeletedAt.is_null())
        .one(db)
        .await
        .map_err(|_| db_error())?
        .ok_or_else(|| forbidden(action))?;
    authorize_link(db, user_id, &link, action).await?;
    Ok(link)
}

/// A caller's memberships, loaded once so a batch of links (bulk update and
/// delete) is authorized without a query per link.
pub struct CallerAccess {
    user_id: i32,
    memberships: HashMap<i32, org_members::Model>,
}

impl CallerAccess {
    pub async fn load<C: ConnectionTrait>(db: &C, user_id: i32) -> Result<Self, DbErr> {
        let memberships = org_members::Entity::find()
            .filter(org_members::Column::UserId.eq(user_id))
            .all(db)
            .await?
            .into_iter()
            .map(|member| (member.org_id, member))
            .collect();
        Ok(Self {
            user_id,
            memberships,
        })
    }

    pub fn can(&self, link: &links::Model, action: LinkAction) -> bool {
        let member = link.org_id.and_then(|org_id| self.memberships.get(&org_id));
        permits(link, self.user_id, member, action)
    }
}

/// Organizations `user_id` currently belongs to, as a subquery.
fn member_org_ids(user_id: i32) -> SelectStatement {
    Query::select()
        .column(org_members::Column::OrgId)
        .from(org_members::Entity)
        .and_where(Expr::col(org_members::Column::UserId).eq(user_id))
        .to_owned()
}

/// Links `user_id` may read: their personal links, plus every link of an
/// organization they currently belong to (whoever created it).
pub fn readable_links(user_id: i32) -> Condition {
    Condition::any()
        .add(
            Condition::all()
                .add(links::Column::OrgId.is_null())
                .add(links::Column::UserId.eq(user_id)),
        )
        .add(links::Column::OrgId.in_subquery(member_org_ids(user_id)))
}

/// The links `user_id` created that they may still read: personal ones, and
/// organization ones only while they remain a member. This is what an
/// account-level view (link list, export, dashboard totals) shows when no
/// organization is selected.
pub fn own_links(user_id: i32) -> Condition {
    Condition::all().add(links::Column::UserId.eq(user_id)).add(
        Condition::any()
            .add(links::Column::OrgId.is_null())
            .add(links::Column::OrgId.in_subquery(member_org_ids(user_id))),
    )
}

/// What a link list or export shows. With an organization selected, every one
/// of its links, whoever created each, to a current member (anyone else gets
/// nothing); without one, the caller's own links.
pub fn listed_links(user_id: i32, org_id: Option<i32>) -> Condition {
    match org_id {
        Some(org_id) => Condition::all()
            .add(readable_links(user_id))
            .add(links::Column::OrgId.eq(org_id)),
        None => own_links(user_id),
    }
}

/// Live links that still belong to this account: [`own_links`], excluding
/// soft-deleted rows. Create, bulk create, clone and the profile's
/// `link_count` all use this, so a link in an organization the user has left
/// cannot keep counting after they lose the ability to delete it.
pub async fn count_own_live_links<C: ConnectionTrait>(db: &C, user_id: i32) -> u64 {
    links::Entity::find()
        .filter(own_links(user_id))
        .filter(links::Column::DeletedAt.is_null())
        .count(db)
        .await
        .unwrap_or(0)
}

/// `(link_count, total_clicks)` for `/auth/me` and `PUT /auth/profile`.
///
/// `link_count` matches [`count_own_live_links`]. Clicks follow [`own_links`]
/// with no `deleted_at` filter: a click on a soft-deleted personal link still
/// counts, and a click on a link in an organization the user has left does not.
pub async fn own_link_totals<C: ConnectionTrait>(db: &C, user_id: i32) -> (i64, i64) {
    let link_count = count_own_live_links(db, user_id).await as i64;
    let total_clicks = crate::entity::click_events::Entity::find()
        .inner_join(links::Entity)
        .filter(own_links(user_id))
        .count(db)
        .await
        .unwrap_or(0) as i64;
    (link_count, total_clicks)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CALLER: i32 = 7;
    const OTHER: i32 = 8;

    fn link(org_id: Option<i32>, creator: i32) -> links::Model {
        links::Model {
            id: 1,
            code: "abc".to_string(),
            original_url: "https://example.com".to_string(),
            user_id: Some(creator),
            created_at: chrono::Utc::now().naive_utc(),
            click_count: 0,
            expires_at: None,
            password_hash: None,
            title: None,
            notes: None,
            folder_id: None,
            org_id,
            starts_at: None,
            max_clicks: None,
            deleted_at: None,
            is_pinned: false,
            burn_after_reading: false,
            burned_at: None,
            safe_link_interstitial: false,
            bio_visible: false,
            bio_position: None,
            bio_label: None,
            expiry_notice_sent_at: None,
        }
    }

    fn member(role: &str) -> org_members::Model {
        org_members::Model {
            id: 1,
            org_id: 1,
            user_id: CALLER,
            role: role.to_string(),
            joined_at: chrono::Utc::now().naive_utc(),
        }
    }

    const ALL: [LinkAction; 4] = [
        LinkAction::Read,
        LinkAction::Write,
        LinkAction::Clone,
        LinkAction::Delete,
    ];

    #[test]
    fn personal_link_belongs_to_its_creator_only() {
        for action in ALL {
            assert!(permits(&link(None, CALLER), CALLER, None, action));
            assert!(!permits(&link(None, OTHER), CALLER, None, action));
            // Membership of some org says nothing about a personal link.
            assert!(!permits(
                &link(None, OTHER),
                CALLER,
                Some(&member("owner")),
                action
            ));
        }
    }

    #[test]
    fn organization_link_needs_current_membership() {
        for action in ALL {
            assert!(
                !permits(&link(Some(1), CALLER), CALLER, None, action),
                "a creator who is no longer a member gets nothing ({action:?})"
            );
            assert!(!permits(&link(Some(1), OTHER), CALLER, None, action));
        }
    }

    #[test]
    fn organization_link_role_matrix_for_a_link_someone_else_created() {
        let link = link(Some(1), OTHER);
        let expect = |role: &str, read, write, clone, delete| {
            let m = member(role);
            assert_eq!(permits(&link, CALLER, Some(&m), LinkAction::Read), read);
            assert_eq!(permits(&link, CALLER, Some(&m), LinkAction::Write), write);
            assert_eq!(permits(&link, CALLER, Some(&m), LinkAction::Clone), clone);
            assert_eq!(permits(&link, CALLER, Some(&m), LinkAction::Delete), delete);
        };
        expect("owner", true, true, true, true);
        expect("admin", true, true, true, true);
        expect("editor", true, true, true, false);
        expect("viewer", true, false, false, false);
        expect("future-role", true, false, false, false);
    }

    #[test]
    fn creator_who_is_still_an_editor_may_delete_their_own_org_link() {
        let link = link(Some(1), CALLER);
        assert!(permits(
            &link,
            CALLER,
            Some(&member("editor")),
            LinkAction::Delete
        ));
        // Demoted: the creator no longer controls it.
        for action in [LinkAction::Write, LinkAction::Clone, LinkAction::Delete] {
            assert!(!permits(&link, CALLER, Some(&member("viewer")), action));
        }
        assert!(permits(
            &link,
            CALLER,
            Some(&member("viewer")),
            LinkAction::Read
        ));
    }
}
