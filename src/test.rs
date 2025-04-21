use std::sync::{Arc, OnceLock, Weak};

use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{ContainerAsync, ImageExt},
};
use tokio::sync::Mutex;

use crate::translate_file;

pub(crate) async fn db_transaction() -> (
    Arc<ContainerAsync<Postgres>>,
    tokio_postgres::Transaction<'static>,
) {
    use testcontainers_modules::testcontainers::runners::AsyncRunner;
    // https://github.com/testcontainers/testcontainers-rs/issues/707#issuecomment-2248314261
    static C: OnceLock<Mutex<Weak<ContainerAsync<Postgres>>>> = OnceLock::new();

    let mut guard = C.get_or_init(|| Mutex::new(Weak::new())).lock().await;
    let c = if let Some(c) = guard.upgrade() {
        c
    } else {
        let c = testcontainers_modules::postgres::Postgres::default()
            .with_tag("16-alpine")
            .with_container_name("pg-sqlc-test")
            .start()
            .await
            .unwrap();
        let c = Arc::new(c);
        *guard = Arc::downgrade(&c);

        c
    };
    let host_port = c.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@localhost:{host_port}/postgres");

    let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls)
        .await
        .unwrap();

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });
    // TODO: client pool
    let client = Box::leak(Box::new(client));
    let t = client.transaction().await.unwrap();

    (c, t)
}

async fn e2e(ts: &str, ps: &str) -> String {
    let (_c, t) = db_transaction().await;
    t.execute(ts, &[]).await.unwrap();

    let mut sql = std::io::Cursor::new(ps);
    let mut rs = std::io::Cursor::new(Vec::new());

    translate_file(&t, &mut sql, &mut rs).await.unwrap();
    String::from_utf8(rs.into_inner()).unwrap()
}

#[tokio::test]
async fn without_input() {
    let rs = e2e(
        "CREATE TABLE users(id INT PRIMARY KEY GENERATED ALWAYS AS IDENTITY, name TEXT);",
        "PREPARE list_users AS SELECT id, name FROM users;",
    )
    .await;

    insta::assert_snapshot!(rs, @r#"
    pub struct ListUsersRows {
        pub id: Option<i32>,
        pub name: Option<String>,
    }
    pub async fn list_users(
        c: &impl tokio_postgres::GenericClient,
    ) -> Result<Vec<ListUsersRows>, tokio_postgres::Error> {
        c.query("SELECT id, name FROM users", &[])
            .await
            .map(|rs| {
                rs.into_iter()
                    .map(|r| ListUsersRows {
                        id: r.get(0),
                        name: r.get(1),
                    })
                    .collect()
            })
    }
    "#);
}

#[tokio::test]
async fn with_input() {
    let rs = e2e(
        "CREATE TABLE users(id INT PRIMARY KEY GENERATED ALWAYS AS IDENTITY, name TEXT);",
        "PREPARE find_user AS SELECT id, name FROM users where id = $1;",
    )
    .await;

    insta::assert_snapshot!(rs, @r#"
    pub struct FindUserParams {
        pub eq_id: i32,
    }
    pub struct FindUserRows {
        pub id: Option<i32>,
        pub name: Option<String>,
    }
    pub async fn find_user(
        c: &impl tokio_postgres::GenericClient,
        p: FindUserParams,
    ) -> Result<Vec<FindUserRows>, tokio_postgres::Error> {
        c.query("SELECT id, name FROM users WHERE id = $1", &[&p.eq_id])
            .await
            .map(|rs| {
                rs.into_iter()
                    .map(|r| FindUserRows {
                        id: r.get(0),
                        name: r.get(1),
                    })
                    .collect()
            })
    }
    "#);
}

#[tokio::test]
async fn with_input_right() {
    let rs = e2e(
        "CREATE TABLE users(id INT PRIMARY KEY GENERATED ALWAYS AS IDENTITY, name TEXT);",
        "PREPARE find_user AS SELECT id, name FROM users where $1 = id;",
    )
    .await;

    insta::assert_snapshot!(rs, @r#"
    pub struct FindUserParams {
        pub eq_id: i32,
    }
    pub struct FindUserRows {
        pub id: Option<i32>,
        pub name: Option<String>,
    }
    pub async fn find_user(
        c: &impl tokio_postgres::GenericClient,
        p: FindUserParams,
    ) -> Result<Vec<FindUserRows>, tokio_postgres::Error> {
        c.query("SELECT id, name FROM users WHERE $1 = id", &[&p.eq_id])
            .await
            .map(|rs| {
                rs.into_iter()
                    .map(|r| FindUserRows {
                        id: r.get(0),
                        name: r.get(1),
                    })
                    .collect()
            })
    }
    "#);
}

#[tokio::test]
async fn with_multiple_inputs() {
    let rs = e2e(
        "CREATE TABLE users(id INT PRIMARY KEY GENERATED ALWAYS AS IDENTITY, name TEXT);",
        "PREPARE find_user AS SELECT id, name FROM users WHERE id > $1 AND name LIKE $2;",
    )
    .await;

    insta::assert_snapshot!(rs, @r#"
    pub struct FindUserParams {
        pub gt_id: i32,
        pub like_name: String,
    }
    pub struct FindUserRows {
        pub id: Option<i32>,
        pub name: Option<String>,
    }
    pub async fn find_user(
        c: &impl tokio_postgres::GenericClient,
        p: FindUserParams,
    ) -> Result<Vec<FindUserRows>, tokio_postgres::Error> {
        c.query(
                "SELECT id, name FROM users WHERE id > $1 AND name LIKE $2",
                &[&p.gt_id, &p.like_name],
            )
            .await
            .map(|rs| {
                rs.into_iter()
                    .map(|r| FindUserRows {
                        id: r.get(0),
                        name: r.get(1),
                    })
                    .collect()
            })
    }
    "#);
}

#[tokio::test]
async fn update() {
    let rs = e2e(
        "CREATE TABLE users(id INT PRIMARY KEY GENERATED ALWAYS AS IDENTITY, name TEXT);",
        "PREPARE update_user AS UPDATE users SET name = $2 WHERE id = $1;",
    )
    .await;

    insta::assert_snapshot!(rs, @r#"
    pub struct UpdateUserParams {
        pub eq_id: i32,
        pub set_name: String,
    }
    pub async fn update_user(
        c: &impl tokio_postgres::GenericClient,
        p: UpdateUserParams,
    ) -> Result<u64, tokio_postgres::Error> {
        c.execute("UPDATE users SET name = $2 WHERE id = $1", &[&p.eq_id, &p.set_name]).await
    }
    "#);
}

#[tokio::test]
async fn update_with_return() {
    let rs = e2e(
        "CREATE TABLE users(id INT PRIMARY KEY GENERATED ALWAYS AS IDENTITY, name TEXT);",
        "PREPARE update_user AS UPDATE users SET name = $2 WHERE id = $1 RETURNING id, name;",
    )
    .await;

    insta::assert_snapshot!(rs, @r#"
    pub struct UpdateUserParams {
        pub eq_id: i32,
        pub set_name: String,
    }
    pub struct UpdateUserRows {
        pub id: Option<i32>,
        pub name: Option<String>,
    }
    pub async fn update_user(
        c: &impl tokio_postgres::GenericClient,
        p: UpdateUserParams,
    ) -> Result<Vec<UpdateUserRows>, tokio_postgres::Error> {
        c.query(
                "UPDATE users SET name = $2 WHERE id = $1 RETURNING id, name",
                &[&p.eq_id, &p.set_name],
            )
            .await
            .map(|rs| {
                rs.into_iter()
                    .map(|r| UpdateUserRows {
                        id: r.get(0),
                        name: r.get(1),
                    })
                    .collect()
            })
    }
    "#);
}

#[tokio::test]
async fn delete() {
    let rs = e2e(
        "CREATE TABLE users(id INT PRIMARY KEY GENERATED ALWAYS AS IDENTITY, name TEXT);",
        "PREPARE delete_user AS DELETE FROM users WHERE id = $1",
    )
    .await;

    insta::assert_snapshot!(rs, @r#"
    pub struct DeleteUserParams {
        pub eq_id: i32,
    }
    pub async fn delete_user(
        c: &impl tokio_postgres::GenericClient,
        p: DeleteUserParams,
    ) -> Result<u64, tokio_postgres::Error> {
        c.execute("DELETE FROM users WHERE id = $1", &[&p.eq_id]).await
    }
    "#);
}

#[tokio::test]
async fn delete_with_return() {
    let rs = e2e(
        "CREATE TABLE users(id INT PRIMARY KEY GENERATED ALWAYS AS IDENTITY, name TEXT);",
        "PREPARE delete_user AS DELETE FROM users WHERE id = $1 returning id, name",
    )
    .await;

    insta::assert_snapshot!(rs, @r#"
    pub struct DeleteUserParams {
        pub eq_id: i32,
    }
    pub struct DeleteUserRows {
        pub id: Option<i32>,
        pub name: Option<String>,
    }
    pub async fn delete_user(
        c: &impl tokio_postgres::GenericClient,
        p: DeleteUserParams,
    ) -> Result<Vec<DeleteUserRows>, tokio_postgres::Error> {
        c.query("DELETE FROM users WHERE id = $1 RETURNING id, name", &[&p.eq_id])
            .await
            .map(|rs| {
                rs.into_iter()
                    .map(|r| DeleteUserRows {
                        id: r.get(0),
                        name: r.get(1),
                    })
                    .collect()
            })
    }
    "#);
}

#[tokio::test]
async fn multiple_prepare() {
    let rs = e2e(
        "CREATE TABLE users(id INT PRIMARY KEY GENERATED ALWAYS AS IDENTITY, name TEXT);",
        "PREPARE list_users AS SELECT id, name FROM users;
         PREPARE find_user AS SELECT id, name FROM users where id = $1;",
    )
    .await;

    insta::assert_snapshot!(rs, @r#"
    pub struct ListUsersRows {
        pub id: Option<i32>,
        pub name: Option<String>,
    }
    pub async fn list_users(
        c: &impl tokio_postgres::GenericClient,
    ) -> Result<Vec<ListUsersRows>, tokio_postgres::Error> {
        c.query("SELECT id, name FROM users", &[])
            .await
            .map(|rs| {
                rs.into_iter()
                    .map(|r| ListUsersRows {
                        id: r.get(0),
                        name: r.get(1),
                    })
                    .collect()
            })
    }

    pub struct FindUserParams {
        pub eq_id: i32,
    }
    pub struct FindUserRows {
        pub id: Option<i32>,
        pub name: Option<String>,
    }
    pub async fn find_user(
        c: &impl tokio_postgres::GenericClient,
        p: FindUserParams,
    ) -> Result<Vec<FindUserRows>, tokio_postgres::Error> {
        c.query("SELECT id, name FROM users WHERE id = $1", &[&p.eq_id])
            .await
            .map(|rs| {
                rs.into_iter()
                    .map(|r| FindUserRows {
                        id: r.get(0),
                        name: r.get(1),
                    })
                    .collect()
            })
    }
    "#);
}
