use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add title column to links table
        manager
            .alter_table(
                Table::alter()
                    .table(Links::Table)
                    .add_column_if_not_exists(
                        ColumnDef::new(Links::Title)
                            .string()
                            .null()
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
                    .drop_column(Links::Title)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Links {
    Table,
    Title,
}



