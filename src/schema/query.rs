pub struct LoadSchemaRows {
    pub oid: Option<tokio_postgres::types::Oid>,
    pub table: Option<String>,
    pub column: Option<Vec<String>>,
    pub type_oid: Option<Vec<tokio_postgres::types::Oid>>,
    pub nullable: Option<Vec<bool>>,
}
pub async fn load_schema(
    c: &impl tokio_postgres::GenericClient,
) -> Result<Vec<LoadSchemaRows>, tokio_postgres::Error> {
    c.query(
            "SELECT c.oid, c.relname AS TABLE, ARRAY_AGG(a.attname) AS COLUMN, ARRAY_AGG(a.atttypid) AS type_oid, ARRAY_AGG(a.attnotnull) AS nullable FROM pg_catalog.pg_attribute AS a JOIN pg_catalog.pg_class AS c ON a.attrelid = c.oid JOIN pg_catalog.pg_namespace AS n ON c.relnamespace = n.oid WHERE a.attnum > 0 AND NOT a.attisdropped AND n.nspname NOT LIKE 'pg_%' AND n.nspname <> 'information_schema' AND c.relkind = 'r' GROUP BY 1",
            &[],
        )
        .await
        .map(|rs| {
            rs.into_iter()
                .map(|r| LoadSchemaRows {
                    oid: r.get(0),
                    table: r.get(1),
                    column: r.get(2),
                    type_oid: r.get(3),
                    nullable: r.get(4),
                })
                .collect()
        })
}
