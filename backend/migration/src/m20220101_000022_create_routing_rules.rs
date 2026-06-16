use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(RoutingRules::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(RoutingRules::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(RoutingRules::LinkId).integer().not_null())
                    // Lower priority is evaluated first.
                    .col(ColumnDef::new(RoutingRules::Priority).integer().not_null().default(0))
                    // Match conditions (NULL = "matches anything").
                    .col(ColumnDef::new(RoutingRules::MatchDevice).string().null())
                    .col(ColumnDef::new(RoutingRules::MatchOs).string().null())
                    .col(ColumnDef::new(RoutingRules::MatchCountry).string().null())
                    .col(ColumnDef::new(RoutingRules::MatchLang).string().null())
                    .col(ColumnDef::new(RoutingRules::DestinationUrl).text().not_null())
                    // Relative weight for A/B splits among equally-matching rules.
                    .col(ColumnDef::new(RoutingRules::Weight).integer().not_null().default(1))
                    .col(
                        ColumnDef::new(RoutingRules::CreatedAt)
                            .timestamp()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-routing_rule-link_id")
                            .from(RoutingRules::Table, RoutingRules::LinkId)
                            .to(Links::Table, Links::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx-routing_rules-link_priority")
                    .table(RoutingRules::Table)
                    .col(RoutingRules::LinkId)
                    .col(RoutingRules::Priority)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(RoutingRules::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum RoutingRules {
    Table,
    Id,
    LinkId,
    Priority,
    MatchDevice,
    MatchOs,
    MatchCountry,
    MatchLang,
    DestinationUrl,
    Weight,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Links {
    Table,
    Id,
}
