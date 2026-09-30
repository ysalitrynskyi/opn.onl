//! Folder delete must unfile links atomically with removing the folder row.
//! Real router + real Postgres via `common::spawn_real_app`.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::{folders, links};
use sea_orm::{ConnectionTrait, DatabaseConnection, EntityTrait};
use serde_json::{Value, json};

async fn register_verified(server: &axum_test::TestServer, db: &DatabaseConnection) -> String {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    let user_id = body["user_id"].as_i64().unwrap() as i32;
    mark_email_verified(db, user_id).await;
    body["token"].as_str().unwrap().to_string()
}

async fn folder_and_link(
    server: &axum_test::TestServer,
    db: &DatabaseConnection,
    token: &str,
) -> (i32, i32) {
    let folder = server
        .post("/folders")
        .authorization_bearer(token)
        .json(&json!({ "name": "to-delete" }))
        .await;
    assert_eq!(
        folder.status_code(),
        201,
        "create folder: {}",
        folder.text()
    );
    let folder_id = folder.json::<Value>()["id"].as_i64().unwrap() as i32;

    let link = server
        .post("/links")
        .authorization_bearer(token)
        .json(&json!({
            "original_url": "https://iana.org/in-folder",
            "folder_id": folder_id,
        }))
        .await;
    assert_eq!(link.status_code(), 201, "create link: {}", link.text());
    let link_id = link.json::<Value>()["id"].as_i64().unwrap() as i32;
    let stored = links::Entity::find_by_id(link_id)
        .one(db)
        .await
        .expect("db")
        .expect("link");
    assert_eq!(
        stored.folder_id,
        Some(folder_id),
        "create must file the link"
    );
    (folder_id, link_id)
}

#[tokio::test]
async fn delete_folder_unfiles_links_and_removes_the_row() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let (folder_id, link_id) = folder_and_link(&server, &db, &token).await;

    let del = server
        .delete(&format!("/folders/{folder_id}"))
        .authorization_bearer(&token)
        .await;
    assert_eq!(del.status_code(), 204, "delete folder: {}", del.text());

    assert!(
        folders::Entity::find_by_id(folder_id)
            .one(&db)
            .await
            .expect("db")
            .is_none(),
        "folder row must be gone"
    );
    let link = links::Entity::find_by_id(link_id)
        .one(&db)
        .await
        .expect("db")
        .expect("link survives");
    assert_eq!(
        link.folder_id, None,
        "fk-link-folder_id ON DELETE SET NULL must unfile the link"
    );
}

/// If the folder DELETE fails, links must still point at the folder. The old
/// two-statement path NULLed folder_id first, so a later failure left an empty
/// folder and a client retry looking at a successful-looking empty folder.
#[tokio::test]
async fn failed_folder_delete_does_not_unfile_links() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let (folder_id, link_id) = folder_and_link(&server, &db, &token).await;

    let suffix = unique_code().to_lowercase();
    let fn_name = format!("fail_folder_del_{suffix}");
    db.execute_unprepared(&format!(
        r#"
        CREATE FUNCTION {fn_name}() RETURNS trigger AS $$
        BEGIN
            RAISE EXCEPTION 'injected folder delete failure';
        END;
        $$ LANGUAGE plpgsql
        "#
    ))
    .await
    .expect("create delete-failure function");
    db.execute_unprepared(&format!(
        r#"
        CREATE TRIGGER {fn_name}
        BEFORE DELETE ON folders
        FOR EACH ROW
        WHEN (OLD.id = {folder_id})
        EXECUTE PROCEDURE {fn_name}()
        "#
    ))
    .await
    .expect("install delete-failure trigger");

    let del = server
        .delete(&format!("/folders/{folder_id}"))
        .authorization_bearer(&token)
        .await;

    let _ = db
        .execute_unprepared(&format!("DROP TRIGGER IF EXISTS {fn_name} ON folders"))
        .await;
    let _ = db
        .execute_unprepared(&format!("DROP FUNCTION IF EXISTS {fn_name}()"))
        .await;

    assert_eq!(
        del.status_code(),
        500,
        "injected delete failure must surface: {}",
        del.text()
    );
    assert!(
        folders::Entity::find_by_id(folder_id)
            .one(&db)
            .await
            .expect("db")
            .is_some(),
        "folder row must still exist after a failed delete"
    );
    let link = links::Entity::find_by_id(link_id)
        .one(&db)
        .await
        .expect("db")
        .expect("link");
    assert_eq!(
        link.folder_id,
        Some(folder_id),
        "a failed delete must not unfile links first"
    );
}
