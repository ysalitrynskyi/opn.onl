use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ========================================
        // LINKS TABLE INDEXES
        // ========================================
        
        // Index for user's links lookup (dashboard queries)
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_links_user_id")
                    .table(Links::Table)
                    .col(Links::UserId)
                    .to_owned(),
            )
            .await?;

        // Index for soft delete filtering
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_links_deleted_at")
                    .table(Links::Table)
                    .col(Links::DeletedAt)
                    .to_owned(),
            )
            .await?;

        // Composite index for user dashboard (user_id + created_at + deleted_at)
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_links_user_created_deleted")
                    .table(Links::Table)
                    .col(Links::UserId)
                    .col(Links::CreatedAt)
                    .col(Links::DeletedAt)
                    .to_owned(),
            )
            .await?;

        // Index for original_url to detect duplicates (hash-based for long URLs)
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_links_original_url")
                    .table(Links::Table)
                    .col(Links::OriginalUrl)
                    .to_owned(),
            )
            .await?;

        // Index for active links filtering
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_links_is_active")
                    .table(Links::Table)
                    .col(Links::IsActive)
                    .to_owned(),
            )
            .await?;

        // Composite index for redirect lookups (code + deleted_at)
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_links_code_deleted")
                    .table(Links::Table)
                    .col(Links::Code)
                    .col(Links::DeletedAt)
                    .to_owned(),
            )
            .await?;

        // ========================================
        // CLICK_EVENTS TABLE INDEXES (for billions of rows)
        // ========================================

        // Index for link analytics (link_id for grouping)
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_click_events_link_id")
                    .table(ClickEvents::Table)
                    .col(ClickEvents::LinkId)
                    .to_owned(),
            )
            .await?;

        // Index for time-based queries
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_click_events_created_at")
                    .table(ClickEvents::Table)
                    .col(ClickEvents::CreatedAt)
                    .to_owned(),
            )
            .await?;

        // Composite index for link stats (link_id + created_at) - CRITICAL for analytics
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_click_events_link_created")
                    .table(ClickEvents::Table)
                    .col(ClickEvents::LinkId)
                    .col(ClickEvents::CreatedAt)
                    .to_owned(),
            )
            .await?;

        // Index for country analytics
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_click_events_country")
                    .table(ClickEvents::Table)
                    .col(ClickEvents::Country)
                    .to_owned(),
            )
            .await?;

        // Index for city analytics
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_click_events_city")
                    .table(ClickEvents::Table)
                    .col(ClickEvents::City)
                    .to_owned(),
            )
            .await?;

        // Composite for geo analytics (country + city)
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_click_events_geo")
                    .table(ClickEvents::Table)
                    .col(ClickEvents::Country)
                    .col(ClickEvents::City)
                    .to_owned(),
            )
            .await?;

        // ========================================
        // USERS TABLE INDEXES
        // ========================================

        // Index for soft delete filtering
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_users_deleted_at")
                    .table(Users::Table)
                    .col(Users::DeletedAt)
                    .to_owned(),
            )
            .await?;

        // Index for admin queries
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_users_is_admin")
                    .table(Users::Table)
                    .col(Users::IsAdmin)
                    .to_owned(),
            )
            .await?;

        // Index for email verification queries
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_users_email_verified")
                    .table(Users::Table)
                    .col(Users::EmailVerified)
                    .to_owned(),
            )
            .await?;

        // ========================================
        // FOLDERS TABLE INDEXES
        // ========================================

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_folders_user_id")
                    .table(Folders::Table)
                    .col(Folders::UserId)
                    .to_owned(),
            )
            .await?;

        // ========================================
        // TAGS TABLE INDEXES
        // ========================================

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_tags_user_id")
                    .table(Tags::Table)
                    .col(Tags::UserId)
                    .to_owned(),
            )
            .await?;

        // ========================================
        // LINK_TAGS TABLE INDEXES
        // ========================================

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_link_tags_link_id")
                    .table(LinkTags::Table)
                    .col(LinkTags::LinkId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_link_tags_tag_id")
                    .table(LinkTags::Table)
                    .col(LinkTags::TagId)
                    .to_owned(),
            )
            .await?;

        // ========================================
        // PASSKEYS TABLE INDEXES
        // ========================================

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_passkeys_user_id")
                    .table(Passkeys::Table)
                    .col(Passkeys::UserId)
                    .to_owned(),
            )
            .await?;

        // ========================================
        // AUDIT_LOG TABLE INDEXES
        // ========================================

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

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop all indexes in reverse order
        let indexes = [
            "idx_audit_log_created_at",
            "idx_passkeys_user_id",
            "idx_link_tags_tag_id",
            "idx_link_tags_link_id",
            "idx_tags_user_id",
            "idx_folders_user_id",
            "idx_users_email_verified",
            "idx_users_is_admin",
            "idx_users_deleted_at",
            "idx_click_events_geo",
            "idx_click_events_city",
            "idx_click_events_country",
            "idx_click_events_link_created",
            "idx_click_events_created_at",
            "idx_click_events_link_id",
            "idx_links_code_deleted",
            "idx_links_is_active",
            "idx_links_original_url",
            "idx_links_user_created_deleted",
            "idx_links_deleted_at",
            "idx_links_user_id",
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
    UserId,
    DeletedAt,
    CreatedAt,
    OriginalUrl,
    IsActive,
    Code,
}

#[derive(DeriveIden)]
enum ClickEvents {
    Table,
    LinkId,
    CreatedAt,
    Country,
    City,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    DeletedAt,
    IsAdmin,
    EmailVerified,
}

#[derive(DeriveIden)]
enum Folders {
    Table,
    UserId,
}

#[derive(DeriveIden)]
enum Tags {
    Table,
    UserId,
}

#[derive(DeriveIden)]
enum LinkTags {
    Table,
    LinkId,
    TagId,
}

#[derive(DeriveIden)]
enum Passkeys {
    Table,
    UserId,
}

#[derive(DeriveIden)]
enum AuditLog {
    Table,
    CreatedAt,
}

