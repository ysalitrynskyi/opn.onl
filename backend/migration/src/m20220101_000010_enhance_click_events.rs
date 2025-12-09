use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add GeoIP columns to click_events
        
        // Add city column
        manager
            .alter_table(
                Table::alter()
                    .table(ClickEvents::Table)
                    .add_column(ColumnDef::new(ClickEvents::City).string().null())
                    .to_owned(),
            )
            .await?;

        // Add region column
        manager
            .alter_table(
                Table::alter()
                    .table(ClickEvents::Table)
                    .add_column(ColumnDef::new(ClickEvents::Region).string().null())
                    .to_owned(),
            )
            .await?;

        // Add latitude column
        manager
            .alter_table(
                Table::alter()
                    .table(ClickEvents::Table)
                    .add_column(ColumnDef::new(ClickEvents::Latitude).double().null())
                    .to_owned(),
            )
            .await?;

        // Add longitude column
        manager
            .alter_table(
                Table::alter()
                    .table(ClickEvents::Table)
                    .add_column(ColumnDef::new(ClickEvents::Longitude).double().null())
                    .to_owned(),
            )
            .await?;

        // Add device column (parsed from user agent)
        manager
            .alter_table(
                Table::alter()
                    .table(ClickEvents::Table)
                    .add_column(ColumnDef::new(ClickEvents::Device).string().null())
                    .to_owned(),
            )
            .await?;

        // Add browser column (parsed from user agent)
        manager
            .alter_table(
                Table::alter()
                    .table(ClickEvents::Table)
                    .add_column(ColumnDef::new(ClickEvents::Browser).string().null())
                    .to_owned(),
            )
            .await?;

        // Add os column (parsed from user agent)
        manager
            .alter_table(
                Table::alter()
                    .table(ClickEvents::Table)
                    .add_column(ColumnDef::new(ClickEvents::Os).string().null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ClickEvents::Table)
                    .drop_column(ClickEvents::City)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(ClickEvents::Table)
                    .drop_column(ClickEvents::Region)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(ClickEvents::Table)
                    .drop_column(ClickEvents::Latitude)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(ClickEvents::Table)
                    .drop_column(ClickEvents::Longitude)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(ClickEvents::Table)
                    .drop_column(ClickEvents::Device)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(ClickEvents::Table)
                    .drop_column(ClickEvents::Browser)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(ClickEvents::Table)
                    .drop_column(ClickEvents::Os)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum ClickEvents {
    Table,
    City,
    Region,
    Latitude,
    Longitude,
    Device,
    Browser,
    Os,
}




