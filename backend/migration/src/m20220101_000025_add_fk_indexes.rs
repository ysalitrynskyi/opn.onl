use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ========================================
        // MISSING FOREIGN-KEY / FILTER INDEXES
        // ========================================
        // These columns back foreign keys and are used for filtering and
        // cascade-delete scans, but had no supporting index.

        // links.folder_id (FK -> folders, filter + cascade scans)
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_links_folder_id")
                    .table(Links::Table)
                    .col(Links::FolderId)
                    .to_owned(),
            )
            .await?;

        // links.org_id (FK -> organizations, filter + cascade scans)
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_links_org_id")
                    .table(Links::Table)
                    .col(Links::OrgId)
                    .to_owned(),
            )
            .await?;

        // folders.org_id (FK -> organizations, filter + cascade scans)
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_folders_org_id")
                    .table(Folders::Table)
                    .col(Folders::OrgId)
                    .to_owned(),
            )
            .await?;

        // tags.org_id (FK -> organizations, filter + cascade scans)
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_tags_org_id")
                    .table(Tags::Table)
                    .col(Tags::OrgId)
                    .to_owned(),
            )
            .await?;

        // organizations.owner_id (FK -> users, filter + cascade scans)
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_organizations_owner_id")
                    .table(Organizations::Table)
                    .col(Organizations::OwnerId)
                    .to_owned(),
            )
            .await?;

        // audit_log.user_id (FK -> users, filter + cascade scans)
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_audit_log_user_id")
                    .table(AuditLog::Table)
                    .col(AuditLog::UserId)
                    .to_owned(),
            )
            .await?;

        // ========================================
        // DROP DUPLICATE INDEX
        // ========================================
        // `idx_audit_log_created_at` (from m17) duplicates `idx-audit-created_at`
        // (from m11); both cover audit_log(created_at). Keep the m11 one and drop
        // this one. `.ok()` swallows the error if the index does not exist, since
        // SeaORM's `Index::drop()` has no `if_exists` builder.
        manager
            .drop_index(
                Index::drop()
                    .name("idx_audit_log_created_at")
                    .table(AuditLog::Table)
                    .to_owned(),
            )
            .await
            .ok();

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Recreate the duplicate index dropped in `up`.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_audit_log_created_at")
                    .table(AuditLog::Table)
                    .col(AuditLog::CreatedAt)
                    .to_owned(),
            )
            .await?;

        // Drop the FK indexes created in `up`, in reverse order.
        let indexes = [
            "idx_audit_log_user_id",
            "idx_organizations_owner_id",
            "idx_tags_org_id",
            "idx_folders_org_id",
            "idx_links_org_id",
            "idx_links_folder_id",
        ];

        for idx in indexes {
            manager
                .drop_index(Index::drop().name(idx).to_owned())
                .await
                .ok(); // Ignore errors if index doesn't exist
        }

        Ok(())
    }
}

// Table references
#[derive(DeriveIden)]
enum Links {
    Table,
    FolderId,
    OrgId,
}

#[derive(DeriveIden)]
enum Folders {
    Table,
    OrgId,
}

#[derive(DeriveIden)]
enum Tags {
    Table,
    OrgId,
}

#[derive(DeriveIden)]
enum Organizations {
    Table,
    OwnerId,
}

#[derive(DeriveIden)]
enum AuditLog {
    Table,
    UserId,
    CreatedAt,
}
