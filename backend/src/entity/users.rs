use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Deserialize, Serialize)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique)]
    pub email: String,
    pub password_hash: String,
    pub created_at: DateTime,
    pub email_verified: bool,
    pub verification_token: Option<String>,
    pub verification_token_expires: Option<DateTime>,
    pub password_reset_token: Option<String>,
    pub password_reset_expires: Option<DateTime>,
    pub is_admin: bool,
    pub deleted_at: Option<DateTime>,
    // Profile fields
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub website: Option<String>,
    pub avatar_url: Option<String>,
    pub location: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::links::Entity")]
    Links,
}

impl Related<super::links::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Links.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    #[allow(dead_code)]
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}
