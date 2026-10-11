use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// A short code whose link row was hard-deleted. It stays reserved for good:
/// see `utils::retired_codes`.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Deserialize, Serialize)]
#[sea_orm(table_name = "retired_codes")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub code: String,
    pub retired_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
