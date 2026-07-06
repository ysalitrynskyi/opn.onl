use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // organizations.owner_id previously cascaded on user deletion, so
        // hard-deleting an org owner silently wiped the whole organization
        // (members, folders, tags, links, audit log) via the org's own
        // cascades. RESTRICT makes the database refuse to delete a user who
        // still owns an organization; the application must transfer ownership
        // or deliberately delete the org first.
        manager
            .alter_table(
                Table::alter()
                    .table(Organizations::Table)
                    .drop_foreign_key(Alias::new("fk-org-owner_id"))
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Organizations::Table)
                    .add_foreign_key(
                        TableForeignKey::new()
                            .name("fk-org-owner_id")
                            .from_tbl(Organizations::Table)
                            .from_col(Organizations::OwnerId)
                            .to_tbl(Users::Table)
                            .to_col(Users::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Organizations::Table)
                    .drop_foreign_key(Alias::new("fk-org-owner_id"))
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Organizations::Table)
                    .add_foreign_key(
                        TableForeignKey::new()
                            .name("fk-org-owner_id")
                            .from_tbl(Organizations::Table)
                            .from_col(Organizations::OwnerId)
                            .to_tbl(Users::Table)
                            .to_col(Users::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Organizations {
    Table,
    OwnerId,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
