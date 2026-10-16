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
    pub click_count: i64,
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
    // Safe-link interstitial: when true, browser visits are shown a
    // "you're leaving to X" preview before redirecting (opt-in, instance-gated).
    #[sea_orm(default_value = "false")]
    pub safe_link_interstitial: bool,
    // Link-in-bio: whether this link appears on the owner's public bio page.
    #[sea_orm(default_value = "false")]
    pub bio_visible: bool,
    pub bio_position: Option<i32>,
    pub bio_label: Option<String>,
    /// When the owner was reminded that the current `expires_at` is near.
    /// Changing the expiry clears it, so a new date gets its own reminder.
    pub expiry_notice_sent_at: Option<DateTime>,
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

    /// The click cap that applies. A stored cap wins, including zero. A one-time
    /// link saved before the writers always stored a cap of 1 is still one visit.
    pub fn effective_max_clicks(&self) -> Option<i32> {
        self.activity().effective_max_clicks()
    }

    /// The columns that decide whether the link is live, apart from the rest of
    /// the row. Lists that only need that answer select these and nothing else.
    pub fn activity(&self) -> LinkActivity {
        LinkActivity {
            deleted_at: self.deleted_at,
            burned_at: self.burned_at,
            starts_at: self.starts_at,
            expires_at: self.expires_at,
            max_clicks: self.max_clicks,
            burn_after_reading: self.burn_after_reading,
            click_count: self.click_count,
        }
    }

    /// Check if link is currently active (considering starts_at, expires_at, max_clicks, and soft delete)
    pub fn is_active(&self) -> bool {
        self.activity().is_active()
    }

    /// Get the reason why link is inactive
    pub fn inactive_reason(&self) -> Option<&'static str> {
        self.inactive_kind().map(InactiveKind::message)
    }

    /// Why the link is not live, or `None` while it is. The one place the
    /// precedence between the reasons is decided.
    pub fn inactive_kind(&self) -> Option<InactiveKind> {
        let now = chrono::Utc::now().naive_utc();

        // Same cap `is_active` uses, so the two cannot disagree.
        let cap_reached = self
            .effective_max_clicks()
            .is_some_and(|max_clicks| self.click_count >= i64::from(max_clicks));

        // Burn state takes priority so a consumed one-time link shows the
        // dedicated message rather than the generic max-clicks one.
        if self.burned_at.is_some() || (self.burn_after_reading && cap_reached) {
            return Some(InactiveKind::OneTimeOpened);
        }

        if let Some(starts_at) = self.starts_at
            && now < starts_at
        {
            return Some(InactiveKind::NotStarted);
        }

        if let Some(expires_at) = self.expires_at
            && now > expires_at
        {
            return Some(InactiveKind::Expired);
        }

        if cap_reached {
            return Some(InactiveKind::ClickCapReached);
        }

        None
    }
}

/// Why a link is not currently live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InactiveKind {
    OneTimeOpened,
    NotStarted,
    Expired,
    ClickCapReached,
}

impl InactiveKind {
    /// The sentence a visitor to `GET /{code}` is given.
    pub fn message(self) -> &'static str {
        match self {
            Self::OneTimeOpened => "This one-time link has already been opened",
            Self::NotStarted => "Link is scheduled to activate later",
            Self::Expired => "Link has expired",
            Self::ClickCapReached => "Link has reached maximum clicks",
        }
    }

    /// Stable identifier for API clients that word the reason themselves. A
    /// one-time link and a capped one are both `used_up`; the preview already
    /// says which of the two it is.
    pub fn code(self) -> &'static str {
        match self {
            Self::OneTimeOpened | Self::ClickCapReached => "used_up",
            Self::NotStarted => "not_started",
            Self::Expired => "expired",
        }
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
            safe_link_interstitial: false,
            bio_visible: false,
            bio_position: None,
            bio_label: None,
            expiry_notice_sent_at: None,
        }
    }

    #[test]
    fn burn_link_without_a_stored_cap_is_one_visit() {
        let mut m = model();
        m.burn_after_reading = true;
        assert_eq!(m.effective_max_clicks(), Some(1));
        assert!(m.is_active());
        assert!(m.inactive_reason().is_none());

        m.click_count = 1;
        assert!(!m.is_active());
        assert_eq!(
            m.inactive_reason(),
            Some("This one-time link has already been opened")
        );
    }

    #[test]
    fn a_stored_cap_wins_over_the_one_time_default() {
        let mut m = model();
        m.burn_after_reading = true;
        m.max_clicks = Some(5);
        assert_eq!(m.effective_max_clicks(), Some(5));
        m.click_count = 1;
        assert!(m.is_active());

        // Zero is a real cap, not "missing".
        m.max_clicks = Some(0);
        m.click_count = 0;
        assert_eq!(m.effective_max_clicks(), Some(0));
        assert!(!m.is_active());
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

    #[test]
    fn unconstrained_link_is_active() {
        assert!(model().is_active());
        assert!(model().inactive_reason().is_none());
    }

    #[test]
    fn future_starts_at_is_inactive() {
        let mut m = model();
        m.starts_at = Some(chrono::Utc::now().naive_utc() + chrono::Duration::hours(1));
        assert!(!m.is_active());
        assert_eq!(
            m.inactive_reason(),
            Some("Link is scheduled to activate later")
        );
    }

    #[test]
    fn past_expires_at_is_inactive() {
        let mut m = model();
        m.expires_at = Some(chrono::Utc::now().naive_utc() - chrono::Duration::hours(1));
        assert!(!m.is_active());
        assert_eq!(m.inactive_reason(), Some("Link has expired"));
    }

    #[test]
    fn zero_max_clicks_is_inactive() {
        let mut m = model();
        m.max_clicks = Some(0);
        m.click_count = 0;
        assert!(!m.is_active());
        assert_eq!(m.inactive_reason(), Some("Link has reached maximum clicks"));
    }

    #[test]
    fn inactive_codes_tell_clients_why_without_parsing_prose() {
        assert_eq!(model().inactive_kind(), None);

        let mut scheduled = model();
        scheduled.starts_at = Some(chrono::Utc::now().naive_utc() + chrono::Duration::hours(1));
        assert_eq!(
            scheduled.inactive_kind().map(InactiveKind::code),
            Some("not_started")
        );

        let mut expired = model();
        expired.expires_at = Some(chrono::Utc::now().naive_utc() - chrono::Duration::hours(1));
        assert_eq!(
            expired.inactive_kind().map(InactiveKind::code),
            Some("expired")
        );

        let mut capped = model();
        capped.max_clicks = Some(1);
        capped.click_count = 1;
        assert_eq!(
            capped.inactive_kind().map(InactiveKind::code),
            Some("used_up")
        );

        let mut burned = model();
        burned.burn_after_reading = true;
        burned.burned_at = Some(chrono::Utc::now().naive_utc());
        assert_eq!(
            burned.inactive_kind().map(InactiveKind::code),
            Some("used_up")
        );
    }
}

/// The fields [`Model::is_active`] reads, so a query can load just these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkActivity {
    pub deleted_at: Option<DateTime>,
    pub burned_at: Option<DateTime>,
    pub starts_at: Option<DateTime>,
    pub expires_at: Option<DateTime>,
    pub max_clicks: Option<i32>,
    pub burn_after_reading: bool,
    pub click_count: i64,
}

impl LinkActivity {
    /// See [`Model::effective_max_clicks`].
    pub fn effective_max_clicks(&self) -> Option<i32> {
        self.max_clicks.or(self.burn_after_reading.then_some(1))
    }

    /// See [`Model::is_active`].
    pub fn is_active(&self) -> bool {
        // Check soft delete first
        if self.deleted_at.is_some() {
            return false;
        }

        // A consumed one-time link is permanently inactive.
        if self.burned_at.is_some() {
            return false;
        }

        let now = chrono::Utc::now().naive_utc();

        // Check if link hasn't started yet
        if let Some(starts_at) = self.starts_at
            && now < starts_at
        {
            return false;
        }

        // Check if link has expired
        if let Some(expires_at) = self.expires_at
            && now > expires_at
        {
            return false;
        }

        // Check if max clicks reached. An uncapped one-time link counts as 1.
        if self
            .effective_max_clicks()
            .is_some_and(|max_clicks| self.click_count >= i64::from(max_clicks))
        {
            return false;
        }

        true
    }
}
