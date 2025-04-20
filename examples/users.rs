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

pub struct FindUserParams {
    pub eq_id: Option<i32>,
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

fn main() {}
