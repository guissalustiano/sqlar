pub struct ListUsersRows {
    pub id: Option<i32>,
    pub name: Option<String>,
}
pub async fn list_users(
    c: impl tokio_postgres::GenericClient,
    p: ListUsersParams,
) -> Result<Vec<ListUsersRows>, tokio_postgres::Error> {
    c.query("SELECT u.id, u.name FROM users AS u", &[])
        .await
        .map(|rs| {
            rs.into_iter()
                .map(|r| ListUsersRows {
                    id: r.try_get(0)?,
                    name: r.try_get(1)?,
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
    c.query("SELECT u.id, u.name FROM users AS u WHERE u.id = $1", &[p.param_0])
        .await
        .map(|rs| {
            rs.into_iter()
                .map(|r| FindUserRows {
                    id: r.try_get(0)?,
                    name: r.try_get(1)?,
                })
                .collect()
        })
}
