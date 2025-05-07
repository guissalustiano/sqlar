# SQLAR: SQL but better them fossil alternatives
Type-safe rust code

> [!CAUTION]
> This repo is a work in progress and is not ready to be used yet
## Plan
Before publishing, I will improve the type analysis, inferring the right types to output.
It's also planned to support domains and references with new types of patterns.
And explore dimensional analysis with constraints to avoid returning Vec to everything.

## How to use?
Write prepared statements in a SQL file aside from your rust code.
```sql
PREPARE find_user AS SELECT id, name FROM users WHERE id = $1;
PREPARE list_users AS SELECT id, name FROM users;
PREPARE create_user AS INSERT INTO users(name) VALUES ($1);
PREPARE update_user AS UPDATE users SET name = $2 WHERE id = $1;
PREPARE delete_user AS DELETE FROM users WHERE id = $1;
```

When done, run the CLI to generate the rust code
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

pub struct CreateUserParams {
    pub name: String,
}
pub async fn create_user(
    c: &impl tokio_postgres::GenericClient,
    p: CreateUserParams,
) -> Result<u64, tokio_postgres::Error> {
    c.execute("INSERT INTO users (name) VALUES ($1)", &[&p.name]).await
}

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

pub struct DeleteUserParams {
    pub eq_id: i32,
}
pub async fn delete_user(
    c: &impl tokio_postgres::GenericClient,
    p: DeleteUserParams,
) -> Result<u64, tokio_postgres::Error> {
    c.execute("DELETE FROM users WHERE id = $1", &[&p.eq_id]).await
}
```

# Inspirations
- [cornucopia](https://github.com/cornucopia-rs/cornucopia) - The first SQL code gen for rust, but uses a slice different SQL grammar and doesn't allow "copy-paste" to Postgres.
- [diesel](https://github.com/cornucopia-rs/cornucopia) - Diesel had created a SQL syntax analyzer using the Rust type system. This is spectacular, but the errors generated are hard and slow to compile.
- [sqlx](https://github.com/launchbadge/sqlx) - Proves that people want to write SQL, but an impure proc-macro makes cache compile harder. 

# Licenses
SQLc is licensed under AGPL-3.0.
You're free to use it to generate code for the Rust projects of your choice,
even commercials.

The generated code is not licensed by AGPL-3.0.  
