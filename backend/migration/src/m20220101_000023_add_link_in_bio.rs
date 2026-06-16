use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // User-level link-in-bio controls. Reuses existing profile columns
        // (display_name/bio/website/avatar_url/location) for the page header.
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column_if_not_exists(ColumnDef::new(Users::BioUsername).string().null())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column_if_not_exists(
                        ColumnDef::new(Users::BioEnabled).boolean().not_null().default(false),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column_if_not_exists(ColumnDef::new(Users::BioTheme).string().null())
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .unique()
                    .name("idx-users-bio_username")
                    .table(Users::Table)
                    .col(Users::BioUsername)
                    .to_owned(),
            )
            .await?;

        // Per-link bio controls.
        manager
            .alter_table(
                Table::alter()
                    .table(Links::Table)
                    .add_column_if_not_exists(
                        ColumnDef::new(Links::BioVisible).boolean().not_null().default(false),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Links::Table)
                    .add_column_if_not_exists(ColumnDef::new(Links::BioPosition).integer().null())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Links::Table)
                    .add_column_if_not_exists(ColumnDef::new(Links::BioLabel).string().null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(Index::drop().name("idx-users-bio_username").table(Users::Table).to_owned())
            .await?;
        for col in [Users::BioUsername, Users::BioEnabled, Users::BioTheme] {
            manager
                .alter_table(Table::alter().table(Users::Table).drop_column(col).to_owned())
                .await?;
        }
        for col in [Links::BioVisible, Links::BioPosition, Links::BioLabel] {
            manager
                .alter_table(Table::alter().table(Links::Table).drop_column(col).to_owned())
                .await?;
        }
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Users {
    Table,
    BioUsername,
    BioEnabled,
    BioTheme,
}

#[derive(DeriveIden)]
enum Links {
    Table,
    BioVisible,
    BioPosition,
    BioLabel,
}
