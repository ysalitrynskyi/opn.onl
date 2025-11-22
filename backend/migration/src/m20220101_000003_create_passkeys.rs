use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Passkeys::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Passkeys::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Passkeys::UserId).integer().not_null())
                    .col(ColumnDef::new(Passkeys::CredId).string().not_null()) // Base64 encoded
                    .col(ColumnDef::new(Passkeys::CredPublicKey).string().not_null()) // Base64 encoded
                    .col(ColumnDef::new(Passkeys::Counter).integer().not_null().default(0))
                    .col(ColumnDef::new(Passkeys::CreatedAt).timestamp().default(Expr::current_timestamp()))
                    .col(ColumnDef::new(Passkeys::LastUsed).timestamp().null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-passkey-user_id")
                            .from(Passkeys::Table, Passkeys::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Passkeys::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Passkeys {
    Table,
    Id,
    UserId,
    CredId,
    CredPublicKey,
    Counter,
    CreatedAt,
    LastUsed,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
