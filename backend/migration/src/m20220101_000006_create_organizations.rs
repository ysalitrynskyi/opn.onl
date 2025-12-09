use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create Organizations Table
        manager
            .create_table(
                Table::create()
                    .table(Organizations::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Organizations::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Organizations::Name).string().not_null())
                    .col(ColumnDef::new(Organizations::Slug).string().not_null().unique_key())
                    .col(ColumnDef::new(Organizations::OwnerId).integer().not_null())
                    .col(ColumnDef::new(Organizations::CreatedAt).timestamp().default(Expr::current_timestamp()))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-org-owner_id")
                            .from(Organizations::Table, Organizations::OwnerId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create Organization Members Table
        manager
            .create_table(
                Table::create()
                    .table(OrgMembers::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(OrgMembers::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(OrgMembers::OrgId).integer().not_null())
                    .col(ColumnDef::new(OrgMembers::UserId).integer().not_null())
                    .col(ColumnDef::new(OrgMembers::Role).string().not_null().default("member"))
                    .col(ColumnDef::new(OrgMembers::JoinedAt).timestamp().default(Expr::current_timestamp()))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-orgmember-org_id")
                            .from(OrgMembers::Table, OrgMembers::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-orgmember-user_id")
                            .from(OrgMembers::Table, OrgMembers::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Add unique constraint for org member
        manager
            .create_index(
                Index::create()
                    .name("idx-orgmember-unique")
                    .table(OrgMembers::Table)
                    .col(OrgMembers::OrgId)
                    .col(OrgMembers::UserId)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(OrgMembers::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Organizations::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Organizations {
    Table,
    Id,
    Name,
    Slug,
    OwnerId,
    CreatedAt,
}

#[derive(DeriveIden)]
enum OrgMembers {
    Table,
    Id,
    OrgId,
    UserId,
    Role,
    JoinedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}





