pub struct LoadSchemaRows {
    pub oid: Option<tokio_postgres::types::Oid>,
    pub table: Option<String>,
    pub column: Option<Vec<String>>,
    pub type_oid: Option<Vec<tokio_postgres::types::Oid>>,
    pub nullable: Option<Vec<bool>>,
    pub column_position: Option<Vec<i16>>,
    pub has_unique_index: Option<Vec<bool>>,
}
pub async fn load_schema(
    c: &impl tokio_postgres::GenericClient,
) -> Result<Vec<LoadSchemaRows>, tokio_postgres::Error> {
    c.query(
            "SELECT c.oid, c.relname AS TABLE, ARRAY_AGG(a.attname) AS COLUMN, ARRAY_AGG(a.atttypid) AS type_oid, ARRAY_AGG(NOT a.attnotnull) AS nullable, ARRAY_AGG(a.attnum) AS column_position, ARRAY_AGG(EXISTS (SELECT 1 FROM pg_catalog.pg_index AS ix WHERE ix.indrelid = c.oid AND ix.indisunique = true AND a.attnum = ANY(ix.indkey))) AS has_unique_index FROM pg_catalog.pg_attribute AS a JOIN pg_catalog.pg_class AS c ON a.attrelid = c.oid JOIN pg_catalog.pg_namespace AS n ON c.relnamespace = n.oid WHERE a.attnum > 0 AND NOT a.attisdropped AND n.nspname <> 'information_schema' AND c.relkind = 'r' GROUP BY 1",
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
                    column_position: r.get(5),
                    has_unique_index: r.get(6),
                })
                .collect()
        })
}

pub struct LoadFuncsRows {
    pub function_name: String,
    pub return_type: tokio_postgres::types::Oid,
}
pub async fn load_funcs(
    c: &impl tokio_postgres::GenericClient,
) -> Result<Vec<LoadFuncsRows>, tokio_postgres::Error> {
    c.query(
        "
            SELECT
                p.proname AS function_name,
                p.prorettype AS return_type
            FROM
                pg_catalog.pg_proc p
            ",
        &[],
    )
    .await
    .map(|rs| {
        rs.into_iter()
            .map(|r| LoadFuncsRows {
                function_name: r.get(0),
                return_type: r.get(1),
            })
            .collect()
    })
}
