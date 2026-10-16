use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Deserialize, Serialize)]
#[sea_orm(table_name = "abuse_reports")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    /// Short code as the reporter typed it (trimmed). Not a foreign key:
    /// the code stays readable after the link is deleted.
    pub code: String,
    pub link_id: Option<i32>,
    pub reason: String,
    pub details: Option<String>,
    pub reporter_email: Option<String>,
    pub status: String,
    pub created_at: DateTime,
    pub resolved_at: Option<DateTime>,
    pub resolved_by: Option<i32>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::links::Entity",
        from = "Column::LinkId",
        to = "super::links::Column::Id",
        on_update = "Cascade",
        on_delete = "SetNull"
    )]
    Link,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::ResolvedBy",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "SetNull"
    )]
    Resolver,
}

impl Related<super::links::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Link.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
