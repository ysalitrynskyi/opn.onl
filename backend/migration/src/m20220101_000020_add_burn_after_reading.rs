use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Intent flag: link self-destructs once its click cap is reached.
        manager
            .alter_table(
                Table::alter()
                    .table(Links::Table)
                    .add_column_if_not_exists(
                        ColumnDef::new(Links::BurnAfterReading)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;
        // Terminal marker: set once the link has been consumed.
        manager
            .alter_table(
                Table::alter()
                    .table(Links::Table)
                    .add_column_if_not_exists(ColumnDef::new(Links::BurnedAt).timestamp().null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Links::Table)
                    .drop_column(Links::BurnAfterReading)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Links::Table)
                    .drop_column(Links::BurnedAt)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Links {
    Table,
    BurnAfterReading,
    BurnedAt,
}
