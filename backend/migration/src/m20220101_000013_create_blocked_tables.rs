use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create blocked_links table
        manager
            .create_table(
                Table::create()
                    .table(BlockedLinks::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(BlockedLinks::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(BlockedLinks::Url)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(BlockedLinks::Reason).string())
                    .col(ColumnDef::new(BlockedLinks::BlockedBy).integer())
                    .col(
                        ColumnDef::new(BlockedLinks::CreatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_blocked_links_user")
                            .from(BlockedLinks::Table, BlockedLinks::BlockedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create blocked_domains table
        manager
            .create_table(
                Table::create()
                    .table(BlockedDomains::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(BlockedDomains::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(BlockedDomains::Domain)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(BlockedDomains::Reason).string())
                    .col(ColumnDef::new(BlockedDomains::BlockedBy).integer())
                    .col(
                        ColumnDef::new(BlockedDomains::CreatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_blocked_domains_user")
                            .from(BlockedDomains::Table, BlockedDomains::BlockedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(BlockedLinks::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(BlockedDomains::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum BlockedLinks {
    Table,
    Id,
    Url,
    Reason,
    BlockedBy,
    CreatedAt,
}

#[derive(Iden)]
enum BlockedDomains {
    Table,
    Id,
    Domain,
    Reason,
    BlockedBy,
    CreatedAt,
}

#[derive(Iden)]
enum Users {
    Table,
    Id,
}



