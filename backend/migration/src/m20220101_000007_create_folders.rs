use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create Folders Table
        manager
            .create_table(
                Table::create()
                    .table(Folders::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Folders::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Folders::Name).string().not_null())
                    .col(ColumnDef::new(Folders::Color).string().null())
                    .col(ColumnDef::new(Folders::UserId).integer().null())
                    .col(ColumnDef::new(Folders::OrgId).integer().null())
                    .col(ColumnDef::new(Folders::CreatedAt).timestamp().default(Expr::current_timestamp()))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-folder-user_id")
                            .from(Folders::Table, Folders::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-folder-org_id")
                            .from(Folders::Table, Folders::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Folders::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Folders {
    Table,
    Id,
    Name,
    Color,
    UserId,
    OrgId,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Organizations {
    Table,
    Id,
}





