use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Deserialize, Serialize)]
#[sea_orm(table_name = "organizations")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
    #[sea_orm(unique)]
    pub slug: String,
    pub owner_id: i32,
    pub created_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::OwnerId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Owner,
    #[sea_orm(has_many = "super::org_members::Entity")]
    Members,
    #[sea_orm(has_many = "super::folders::Entity")]
    Folders,
    #[sea_orm(has_many = "super::tags::Entity")]
    Tags,
    #[sea_orm(has_many = "super::links::Entity")]
    Links,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Owner.def()
    }
}

impl Related<super::org_members::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Members.def()
    }
}

impl Related<super::folders::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Folders.def()
    }
}

impl Related<super::tags::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Tags.def()
    }
}

impl Related<super::links::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Links.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}





