use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add profile fields to users table
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column_if_not_exists(
                        ColumnDef::new(Users::DisplayName)
                            .string()
                            .null()
                    )
                    .add_column_if_not_exists(
                        ColumnDef::new(Users::Bio)
                            .text()
                            .null()
                    )
                    .add_column_if_not_exists(
                        ColumnDef::new(Users::Website)
                            .string()
                            .null()
                    )
                    .add_column_if_not_exists(
                        ColumnDef::new(Users::AvatarUrl)
                            .string()
                            .null()
                    )
                    .add_column_if_not_exists(
                        ColumnDef::new(Users::Location)
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
                    .table(Users::Table)
                    .drop_column(Users::DisplayName)
                    .drop_column(Users::Bio)
                    .drop_column(Users::Website)
                    .drop_column(Users::AvatarUrl)
                    .drop_column(Users::Location)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Users {
    Table,
    DisplayName,
    Bio,
    Website,
    AvatarUrl,
    Location,
}


