# SQLc: SQL to type-safe rust code

> [!CAUTION]
> This repo is a work in progress and is not ready to be used yet
## Plan
Before publish I with improve the type analysis, infering the righ types to output.
It's also planned to support domains and references with new types patters.
And explore dimensional analysis with constraints to avoid return Vec to everthing

## How to use?
Write prepare statments in sql file aside your rust code
```sql
PREPARE find_user AS SELECT id, name FROM users WHERE id = $1;
PREPARE list_users AS SELECT id, name FROM users;
PREPARE update_user AS UPDATE users SET name = $2 WHERE id = $1;
PREPARE delete_user AS DELETE FROM users WHERE id = $1;
```

When done, run the cli to generate the rust code
```bash
  $ sqlc .
```

Which generates:
```rust
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

pub struct ListUsersRows {
    pub id: Option<i32>,
    pub name: Option<String>,
}
pub async fn list_users(
    c: &impl tokio_postgres::GenericClient,
) -> Result<Vec<ListUsersRows>, tokio_postgres::Error> {
    c.query("SELECT id, name FROM users", &[]).await.map(|rs| {
        rs.into_iter()
            .map(|r| ListUsersRows {
                id: r.get(0),
                name: r.get(1),
            })
            .collect()
    })
}

pub struct UpdateUserParams {
    pub eq_id: i32,
    pub set_name: String,
}
pub async fn update_user(
    c: &impl tokio_postgres::GenericClient,
    p: UpdateUserParams,
) -> Result<u64, tokio_postgres::Error> {
    c.execute(
        "UPDATE users SET name = $2 WHERE id = $1",
        &[&p.eq_id, &p.set_name],
    )
    .await
}

pub struct DeleteUserParams {
    pub eq_id: i32,
}
pub async fn delete_user(
    c: &impl tokio_postgres::GenericClient,
    p: DeleteUserParams,
) -> Result<u64, tokio_postgres::Error> {
    c.execute("DELETE FROM users WHERE id = $1", &[&p.eq_id])
        .await
}
```

# Inspirations
- [cornucopia](https://github.com/cornucopia-rs/cornucopia) - The first sql code gen for rust, but uses a slice different sql grammar with don't allows "copy-paste" to postgres
- [diesel](https://github.com/cornucopia-rs/cornucopia) - Diesel had create a sql syntact anaylize using rust type system. This is awensome but the error generates are hard and slow to compile.
- [sqlx](https://github.com/launchbadge/sqlx) - Proves that people want to write sql, but a inpure proc-macro makes cache compile harder. 

# Licences
SQLc is licenced under AGPL-3.0.
You're free to use it to generate code for the Rust projects of your choice,
even commercial.

The generated code is not licenced by AGPL-3.0.  
