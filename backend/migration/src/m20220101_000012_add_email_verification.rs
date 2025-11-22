use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add email verification fields to users
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column(ColumnDef::new(Users::EmailVerified).boolean().not_null().default(false))
                    .add_column(ColumnDef::new(Users::VerificationToken).string().null())
                    .add_column(ColumnDef::new(Users::VerificationTokenExpires).timestamp().null())
                    .add_column(ColumnDef::new(Users::PasswordResetToken).string().null())
                    .add_column(ColumnDef::new(Users::PasswordResetExpires).timestamp().null())
                    .add_column(ColumnDef::new(Users::IsAdmin).boolean().not_null().default(false))
                    .add_column(ColumnDef::new(Users::DeletedAt).timestamp().null())
                    .to_owned(),
            )
            .await?;

        // Add soft delete to links
        manager
            .alter_table(
                Table::alter()
                    .table(Links::Table)
                    .add_column(ColumnDef::new(Links::DeletedAt).timestamp().null())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .drop_column(Users::EmailVerified)
                    .drop_column(Users::VerificationToken)
                    .drop_column(Users::VerificationTokenExpires)
                    .drop_column(Users::PasswordResetToken)
                    .drop_column(Users::PasswordResetExpires)
                    .drop_column(Users::IsAdmin)
                    .drop_column(Users::DeletedAt)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Links::Table)
                    .drop_column(Links::DeletedAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(Iden)]
enum Users {
    Table,
    EmailVerified,
    VerificationToken,
    VerificationTokenExpires,
    PasswordResetToken,
    PasswordResetExpires,
    IsAdmin,
    DeletedAt,
}

#[derive(Iden)]
enum Links {
    Table,
    DeletedAt,
}

