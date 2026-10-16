use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Visitors can flag a short link. No IP address and no user agent:
        // the report is the code, the reason, optional details and an
        // optional email, nothing that identifies the browser.
        manager
            .create_table(
                Table::create()
                    .table(AbuseReports::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AbuseReports::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(AbuseReports::Code).text().not_null())
                    .col(ColumnDef::new(AbuseReports::LinkId).integer().null())
                    // The destination the reporter saw. The owner can edit
                    // links.original_url afterwards; blocking uses this copy.
                    .col(
                        ColumnDef::new(AbuseReports::DestinationUrl)
                            .text()
                            .not_null(),
                    )
                    .col(ColumnDef::new(AbuseReports::Reason).text().not_null())
                    .col(ColumnDef::new(AbuseReports::Details).text().null())
                    .col(ColumnDef::new(AbuseReports::ReporterEmail).text().null())
                    .col(
                        ColumnDef::new(AbuseReports::Status)
                            .text()
                            .not_null()
                            .default(Expr::value("open")),
                    )
                    .col(
                        ColumnDef::new(AbuseReports::CreatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(ColumnDef::new(AbuseReports::ResolvedAt).timestamp().null())
                    .col(ColumnDef::new(AbuseReports::ResolvedBy).integer().null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_abuse_reports_link")
                            .from(AbuseReports::Table, AbuseReports::LinkId)
                            .to(Links::Table, Links::Id)
                            .on_delete(ForeignKeyAction::SetNull)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_abuse_reports_resolved_by")
                            .from(AbuseReports::Table, AbuseReports::ResolvedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        let db = manager.get_connection();
        // SeaORM's column builder has no CHECK. The allowed sets are the
        // contract the handlers validate against.
        db.execute_unprepared(
            "ALTER TABLE abuse_reports ADD CONSTRAINT abuse_reports_reason_check \
             CHECK (reason IN ('phishing', 'malware', 'spam', 'illegal', 'other'))",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE abuse_reports ADD CONSTRAINT abuse_reports_status_check \
             CHECK (status IN ('open', 'resolved', 'dismissed'))",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE abuse_reports ADD CONSTRAINT abuse_reports_details_len \
             CHECK (details IS NULL OR char_length(details) <= 2000)",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE abuse_reports ADD CONSTRAINT abuse_reports_email_len \
             CHECK (reporter_email IS NULL OR char_length(reporter_email) <= 254)",
        )
        .await?;
        // The admin queue is "open, newest first". Postgres does not index
        // foreign keys on its own.
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_abuse_reports_status_created_at \
             ON abuse_reports (status, created_at)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_abuse_reports_link_id ON abuse_reports (link_id)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_abuse_reports_resolved_by \
             ON abuse_reports (resolved_by)",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AbuseReports::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum AbuseReports {
    Table,
    Id,
    Code,
    LinkId,
    DestinationUrl,
    Reason,
    Details,
    ReporterEmail,
    Status,
    CreatedAt,
    ResolvedAt,
    ResolvedBy,
}

#[derive(Iden)]
enum Links {
    Table,
    Id,
}

#[derive(Iden)]
enum Users {
    Table,
    Id,
}
