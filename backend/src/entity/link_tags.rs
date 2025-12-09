use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Deserialize, Serialize)]
#[sea_orm(table_name = "link_tags")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub link_id: i32,
    pub tag_id: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::links::Entity",
        from = "Column::LinkId",
        to = "super::links::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Link,
    #[sea_orm(
        belongs_to = "super::tags::Entity",
        from = "Column::TagId",
        to = "super::tags::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Tag,
}

impl Related<super::links::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Link.def()
    }
}

impl Related<super::tags::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Tag.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}




