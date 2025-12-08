use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Deserialize, Serialize)]
#[sea_orm(table_name = "org_members")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub org_id: i32,
    pub user_id: i32,
    pub role: String, // "owner", "admin", "editor", "viewer"
    pub joined_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::organizations::Entity",
        from = "Column::OrgId",
        to = "super::organizations::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Organization,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    User,
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

// Role helpers
impl Model {
    pub fn is_owner(&self) -> bool {
        self.role == "owner"
    }

    pub fn is_admin(&self) -> bool {
        self.role == "admin" || self.role == "owner"
    }

    pub fn can_edit(&self) -> bool {
        self.role == "editor" || self.is_admin()
    }

    pub fn can_view(&self) -> bool {
        true // All members can view
    }
}



