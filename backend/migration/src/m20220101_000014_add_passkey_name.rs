use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add name column to passkeys table
        manager
            .alter_table(
                Table::alter()
                    .table(Passkeys::Table)
                    .add_column_if_not_exists(
                        ColumnDef::new(Passkeys::Name)
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
                    .table(Passkeys::Table)
                    .drop_column(Passkeys::Name)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Passkeys {
    Table,
    Name,
}



