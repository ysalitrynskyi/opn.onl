use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Short codes whose link rows were hard-deleted (organization purge,
        // admin hard delete). A soft-deleted link keeps its code reserved
        // through the global UNIQUE on links.code; a purged one would not, so
        // anyone could claim the code and receive the old link's traffic.
        // The primary key is the only index: every lookup is by exact code.
        manager
            .create_table(
                Table::create()
                    .table(RetiredCodes::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(RetiredCodes::Code)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(RetiredCodes::RetiredAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(RetiredCodes::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum RetiredCodes {
    Table,
    Code,
    RetiredAt,
}
