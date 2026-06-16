use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Per-link opt-in: show a "you're leaving to X" safety interstitial before
        // redirecting. Off by default; only meaningful when the instance flag
        // ENABLE_SAFE_LINK_INTERSTITIAL is also on.
        manager
            .alter_table(
                Table::alter()
                    .table(Links::Table)
                    .add_column_if_not_exists(
                        ColumnDef::new(Links::SafeLinkInterstitial)
                            .boolean()
                            .not_null()
                            .default(false),
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
                    .drop_column(Links::SafeLinkInterstitial)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Links {
    Table,
    SafeLinkInterstitial,
}
