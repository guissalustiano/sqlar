# SQLc: SQL to type-safe rust code

> [!CAUTION]
> This repo is a work in progress and is not ready to be used yet
## Plan
Before publish I with improve the type analysis,
infering good params names and the righ types to input and output.
It's also planned to support domains and references with new types patters.
And explore dimensional analysis with constraints to avoid return Vec to everthing

## How to use?
Write prepare statments in sql file aside your rust code
```sql
  PREPARE list_users AS SELECT u.id, u.name FROM users u;
  PREPARE find_user AS SELECT u.id, u.name FROM users u where u.id = $1;
```

When done, run the cli to generate the rust code
```bash
  $ sqlc
```

Which generates:
```rust
    pub struct ListUsersRows {
        pub id: Option<i32>,
        pub name: Option<String>,
    }
    pub async fn list_users(
        c: impl tokio_postgres::GenericClient,
    ) -> Result<Vec<ListUsersRows>, tokio_postgres::Error> {
        c.query("SELECT u.id, u.name FROM users AS u", &[])
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
        pub param_0: Option<i32>,
    }
    pub struct FindUserRows {
        pub id: Option<i32>,
        pub name: Option<String>,
    }
    pub async fn find_user(
        c: impl tokio_postgres::GenericClient,
        p: FindUserParams,
    ) -> Result<Vec<FindUserRows>, tokio_postgres::Error> {
        c.query(
            "SELECT u.id, u.name FROM users AS u WHERE u.id = $1",
            &[&p.param_0],
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
```

# Licences
SQLc is licenced under AGPL-3.0.
You're free to use it to generate code for the Rust projects of your choice,
even commercial.

The generated code is not licenced by AGPL-3.0.  
