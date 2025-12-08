use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add new columns to Links table for enhanced features
        
        // Add notes column
        manager
            .alter_table(
                Table::alter()
                    .table(Links::Table)
                    .add_column(ColumnDef::new(Links::Notes).text().null())
                    .to_owned(),
            )
            .await?;

        // Add folder_id column
        manager
            .alter_table(
                Table::alter()
                    .table(Links::Table)
                    .add_column(ColumnDef::new(Links::FolderId).integer().null())
                    .to_owned(),
            )
            .await?;

        // Add org_id column
        manager
            .alter_table(
                Table::alter()
                    .table(Links::Table)
                    .add_column(ColumnDef::new(Links::OrgId).integer().null())
                    .to_owned(),
            )
            .await?;

        // Add starts_at column (for scheduled activation)
        manager
            .alter_table(
                Table::alter()
                    .table(Links::Table)
                    .add_column(ColumnDef::new(Links::StartsAt).timestamp().null())
                    .to_owned(),
            )
            .await?;

        // Add max_clicks column (for click limit)
        manager
            .alter_table(
                Table::alter()
                    .table(Links::Table)
                    .add_column(ColumnDef::new(Links::MaxClicks).integer().null())
                    .to_owned(),
            )
            .await?;

        // Add foreign keys
        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("fk-link-folder_id")
                    .from(Links::Table, Links::FolderId)
                    .to(Folders::Table, Folders::Id)
                    .on_delete(ForeignKeyAction::SetNull)
                    .on_update(ForeignKeyAction::Cascade)
                    .to_owned(),
            )
            .await?;

        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("fk-link-org_id")
                    .from(Links::Table, Links::OrgId)
                    .to(Organizations::Table, Organizations::Id)
                    .on_delete(ForeignKeyAction::Cascade)
                    .on_update(ForeignKeyAction::Cascade)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop foreign keys first
        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .name("fk-link-folder_id")
                    .table(Links::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .name("fk-link-org_id")
                    .table(Links::Table)
                    .to_owned(),
            )
            .await?;

        // Drop columns
        manager
            .alter_table(
                Table::alter()
                    .table(Links::Table)
                    .drop_column(Links::Notes)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Links::Table)
                    .drop_column(Links::FolderId)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Links::Table)
                    .drop_column(Links::OrgId)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Links::Table)
                    .drop_column(Links::StartsAt)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Links::Table)
                    .drop_column(Links::MaxClicks)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Links {
    Table,
    Notes,
    FolderId,
    OrgId,
    StartsAt,
    MaxClicks,
}

#[derive(DeriveIden)]
enum Folders {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Organizations {
    Table,
    Id,
}



