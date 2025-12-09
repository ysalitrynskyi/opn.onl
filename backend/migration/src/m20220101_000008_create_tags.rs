use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create Tags Table
        manager
            .create_table(
                Table::create()
                    .table(Tags::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Tags::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Tags::Name).string().not_null())
                    .col(ColumnDef::new(Tags::Color).string().null())
                    .col(ColumnDef::new(Tags::UserId).integer().null())
                    .col(ColumnDef::new(Tags::OrgId).integer().null())
                    .col(ColumnDef::new(Tags::CreatedAt).timestamp().default(Expr::current_timestamp()))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-tag-user_id")
                            .from(Tags::Table, Tags::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-tag-org_id")
                            .from(Tags::Table, Tags::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create Link-Tags Junction Table
        manager
            .create_table(
                Table::create()
                    .table(LinkTags::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(LinkTags::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(LinkTags::LinkId).integer().not_null())
                    .col(ColumnDef::new(LinkTags::TagId).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-linktag-link_id")
                            .from(LinkTags::Table, LinkTags::LinkId)
                            .to(Links::Table, Links::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-linktag-tag_id")
                            .from(LinkTags::Table, LinkTags::TagId)
                            .to(Tags::Table, Tags::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Unique constraint for link-tag pairs
        manager
            .create_index(
                Index::create()
                    .name("idx-linktag-unique")
                    .table(LinkTags::Table)
                    .col(LinkTags::LinkId)
                    .col(LinkTags::TagId)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(LinkTags::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Tags::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Tags {
    Table,
    Id,
    Name,
    Color,
    UserId,
    OrgId,
    CreatedAt,
}

#[derive(DeriveIden)]
enum LinkTags {
    Table,
    Id,
    LinkId,
    TagId,
}

#[derive(DeriveIden)]
enum Links {
    Table,
    Id,
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





