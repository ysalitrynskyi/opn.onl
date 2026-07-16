use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column(ColumnDef::new(Users::DisabledAt).timestamp().null())
                    .add_column(ColumnDef::new(Users::DisabledReason).string().null())
                    .add_column(ColumnDef::new(Users::DisabledBy).integer().null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("fk-users-disabled_by")
                    .from(Users::Table, Users::DisabledBy)
                    .to(Users::Table, Users::Id)
                    .on_delete(ForeignKeyAction::SetNull)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-users-disabled_at")
                    .table(Users::Table)
                    .col(Users::DisabledAt)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(BlockedEmailDomains::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(BlockedEmailDomains::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(BlockedEmailDomains::Domain)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(BlockedEmailDomains::Reason).string().null())
                    .col(
                        ColumnDef::new(BlockedEmailDomains::BlockedBy)
                            .integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(BlockedEmailDomains::CreatedAt)
                            .timestamp()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-blocked_email_domains-blocked_by")
                            .from(BlockedEmailDomains::Table, BlockedEmailDomains::BlockedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(BlockedEmailDomains::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx-users-disabled_at")
                    .table(Users::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .name("fk-users-disabled_by")
                    .table(Users::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .drop_column(Users::DisabledBy)
                    .drop_column(Users::DisabledReason)
                    .drop_column(Users::DisabledAt)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
    DisabledAt,
    DisabledReason,
    DisabledBy,
}

#[derive(DeriveIden)]
enum BlockedEmailDomains {
    Table,
    Id,
    Domain,
    Reason,
    BlockedBy,
    CreatedAt,
}
