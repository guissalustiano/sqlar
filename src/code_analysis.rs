pub struct ColumnData {
    pub name: String,
    pub type_: tokio_postgres::types::Type,
}

pub struct PrepareStatement {
    pub name: String,
    pub statement: Box<sqlparser::ast::Statement>,
    pub parameter_types: Vec<tokio_postgres::types::Type>,
    pub result_types: Vec<ColumnData>,
}

pub(crate) async fn prepare_stmts(
    client: &impl tokio_postgres::GenericClient,
    stmts_raw: &str,
) -> eyre::Result<Vec<PrepareStatement>> {
    let _schema = crate::schema::load_schema(client).await?;
    let stmts =
        sqlparser::parser::Parser::parse_sql(&sqlparser::dialect::PostgreSqlDialect {}, stmts_raw)?;

    let futs = stmts.into_iter().map(|stmt| async move {
        let sqlparser::ast::Statement::Prepare {
            name,
            data_types: _,
            statement,
        } = stmt
        else {
            eyre::bail!("not support {stmt} statement");
        };
        let ps = client.prepare(&statement.to_string()).await?;

        Ok(PrepareStatement {
            name: name.value,
            statement,
            parameter_types: ps.params().to_vec(),
            result_types: ps
                .columns()
                .iter()
                .map(|c| ColumnData {
                    // c also contains the table id and column id
                    name: c.name().to_owned(),
                    type_: c.type_().to_owned(),
                })
                .collect(),
        })
    });

    futures::future::try_join_all(futs).await
}
