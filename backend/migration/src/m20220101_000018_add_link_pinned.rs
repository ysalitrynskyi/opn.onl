use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add is_pinned column to links table for favorite/pin functionality
        manager
            .alter_table(
                Table::alter()
                    .table(Links::Table)
                    .add_column_if_not_exists(
                        ColumnDef::new(Links::IsPinned)
                            .boolean()
                            .not_null()
                            .default(false)
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Links::Table)
                    .drop_column(Links::IsPinned)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Links {
    Table,
    IsPinned,
}


