use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Deserialize, Serialize)]
#[sea_orm(table_name = "links")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique)]
    pub code: String,
    pub original_url: String,
    pub user_id: Option<i32>,
    pub created_at: DateTime,
    pub click_count: i32,
    pub expires_at: Option<DateTime>,
    pub password_hash: Option<String>,
    // New fields
    pub title: Option<String>,
    pub notes: Option<String>,
    pub folder_id: Option<i32>,
    pub org_id: Option<i32>,
    pub starts_at: Option<DateTime>,
    pub max_clicks: Option<i32>,
    pub deleted_at: Option<DateTime>,
    #[sea_orm(default_value = "false")]
    pub is_pinned: bool,
    // Burn-after-reading: when true, the link self-destructs once its click cap
    // is reached. `burned_at` is stamped when it has been consumed.
    #[sea_orm(default_value = "false")]
    pub burn_after_reading: bool,
    pub burned_at: Option<DateTime>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    User,
    #[sea_orm(
        belongs_to = "super::folders::Entity",
        from = "Column::FolderId",
        to = "super::folders::Column::Id",
        on_update = "Cascade",
        on_delete = "SetNull"
    )]
    Folder,
    #[sea_orm(
        belongs_to = "super::organizations::Entity",
        from = "Column::OrgId",
        to = "super::organizations::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Organization,
    #[sea_orm(has_many = "super::click_events::Entity")]
    ClickEvents,
    #[sea_orm(has_many = "super::link_tags::Entity")]
    LinkTags,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::folders::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Folder.def()
    }
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}

impl Related<super::click_events::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ClickEvents.def()
    }
}

impl Related<super::link_tags::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::LinkTags.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

// Helper methods
impl Model {
    /// Check if link is deleted (soft delete)
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }

    /// Check if link is currently active (considering starts_at, expires_at, max_clicks, and soft delete)
    pub fn is_active(&self) -> bool {
        // Check soft delete first
        if self.is_deleted() {
            return false;
        }

        // A consumed one-time link is permanently inactive.
        if self.burned_at.is_some() {
            return false;
        }

        let now = chrono::Utc::now().naive_utc();
        
        // Check if link hasn't started yet
        if let Some(starts_at) = self.starts_at {
            if now < starts_at {
                return false;
            }
        }
        
        // Check if link has expired
        if let Some(expires_at) = self.expires_at {
            if now > expires_at {
                return false;
            }
        }
        
        // Check if max clicks reached
        if let Some(max_clicks) = self.max_clicks {
            if self.click_count >= max_clicks {
                return false;
            }
        }
        
        true
    }

    /// Get the reason why link is inactive
    pub fn inactive_reason(&self) -> Option<&'static str> {
        let now = chrono::Utc::now().naive_utc();

        // Burn state takes priority so a consumed one-time link shows the
        // dedicated message rather than the generic max-clicks one.
        if self.burned_at.is_some()
            || (self.burn_after_reading
                && self.max_clicks.map(|m| self.click_count >= m).unwrap_or(false))
        {
            return Some("This one-time link has already been opened");
        }

        if let Some(starts_at) = self.starts_at {
            if now < starts_at {
                return Some("Link is scheduled to activate later");
            }
        }
        
        if let Some(expires_at) = self.expires_at {
            if now > expires_at {
                return Some("Link has expired");
            }
        }
        
        if let Some(max_clicks) = self.max_clicks {
            if self.click_count >= max_clicks {
                return Some("Link has reached maximum clicks");
            }
        }
        
        None
    }
}

#[cfg(test)]
mod burn_tests {
    use super::*;

    fn model() -> Model {
        Model {
            id: 1,
            code: "abc".into(),
            original_url: "https://opn.onl".into(),
            user_id: Some(1),
            created_at: chrono::Utc::now().naive_utc(),
            click_count: 0,
            expires_at: None,
            password_hash: None,
            title: None,
            notes: None,
            folder_id: None,
            org_id: None,
            starts_at: None,
            max_clicks: None,
            deleted_at: None,
            is_pinned: false,
            burn_after_reading: false,
            burned_at: None,
        }
    }

    #[test]
    fn fresh_burn_link_is_active() {
        let mut m = model();
        m.burn_after_reading = true;
        m.max_clicks = Some(1);
        assert!(m.is_active());
        assert!(m.inactive_reason().is_none());
    }

    #[test]
    fn burn_link_exhausted_by_count_shows_one_time_message() {
        let mut m = model();
        m.burn_after_reading = true;
        m.max_clicks = Some(1);
        m.click_count = 1;
        assert!(!m.is_active());
        assert_eq!(
            m.inactive_reason(),
            Some("This one-time link has already been opened")
        );
    }

    #[test]
    fn burned_at_marks_inactive_even_below_cap() {
        let mut m = model();
        m.burn_after_reading = true;
        m.max_clicks = Some(5);
        m.burned_at = Some(chrono::Utc::now().naive_utc());
        assert!(!m.is_active());
        assert_eq!(
            m.inactive_reason(),
            Some("This one-time link has already been opened")
        );
    }

    #[test]
    fn non_burn_max_clicks_keeps_generic_message() {
        let mut m = model();
        m.max_clicks = Some(1);
        m.click_count = 1;
        assert!(!m.is_active());
        assert_eq!(m.inactive_reason(), Some("Link has reached maximum clicks"));
    }
}
