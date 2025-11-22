use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ClickEvents::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ClickEvents::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(ClickEvents::LinkId).integer().not_null())
                    .col(ColumnDef::new(ClickEvents::CreatedAt).timestamp().default(Expr::current_timestamp()))
                    .col(ColumnDef::new(ClickEvents::IpAddress).string().null())
                    .col(ColumnDef::new(ClickEvents::UserAgent).string().null())
                    .col(ColumnDef::new(ClickEvents::Referer).string().null())
                    .col(ColumnDef::new(ClickEvents::Country).string().null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-click_event-link_id")
                            .from(ClickEvents::Table, ClickEvents::LinkId)
                            .to(Links::Table, Links::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ClickEvents::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum ClickEvents {
    Table,
    Id,
    LinkId,
    CreatedAt,
    IpAddress,
    UserAgent,
    Referer,
    Country,
}

#[derive(DeriveIden)]
enum Links {
    Table,
    Id,
}
