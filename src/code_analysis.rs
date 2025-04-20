use eyre::ContextCompat;
use sqlparser::ast::{Expr, Statement};

pub struct InputData {
    pub name: String,
    pub type_: tokio_postgres::types::Type,
}
pub struct ColumnData {
    pub name: String,
    pub type_: tokio_postgres::types::Type,
}

pub struct PrepareStatement {
    pub name: String,
    pub statement: Box<Statement>,
    pub parameter_types: Vec<InputData>,
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
        let Statement::Prepare {
            name,
            data_types: _,
            statement,
        } = stmt
        else {
            eyre::bail!("not support {stmt} statement");
        };
        let ps = client.prepare(&statement.to_string()).await?;

        let parameter_types = ps
            .params()
            .iter()
            .enumerate()
            .map(|(i, t)| {
                Ok(InputData {
                    name: find_param_node(&statement, i)?.context("param not found")?,
                    type_: t.clone(),
                })
            })
            .collect::<eyre::Result<_>>()?;

        Ok(PrepareStatement {
            name: name.value,
            statement,
            parameter_types,
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

fn find_param_node(stmt: &Statement, index: usize) -> eyre::Result<Option<String>> {
    match stmt {
        Statement::Query(query) => match *query.body {
            sqlparser::ast::SetExpr::Select(ref select) => {
                let Some(ref selection) = select.selection else {
                    return Ok(None);
                };

                match selection {
                    Expr::BinaryOp { left, op, right: _ } => {
                        let field_name = match **left {
                            Expr::CompoundIdentifier(ref idents) => {
                                idents.iter().map(|i| i.value.as_str()).collect()
                            }
                            Expr::Identifier(ref ident) => vec![ident.value.as_str()],

                            _ => eyre::bail!("left not supported yet"),
                        };
                        let op_name = match op {
                            sqlparser::ast::BinaryOperator::Eq => "eq",
                            _ => eyre::bail!("op not supported yet"),
                        };
                        let final_name = std::iter::once(op_name)
                            .chain(field_name.into_iter())
                            .collect::<Vec<&str>>()
                            .join("_");
                        Ok(Some(final_name))
                    }
                    _ => eyre::bail!("selection not supported yet"),
                }
            }
            _ => eyre::bail!("not supported yet"),
        },
        Statement::Insert(_) | Statement::Update { .. } | Statement::Delete(_) => {
            eyre::bail!("statement not supported yet")
        }
        _ => eyre::bail!("statement not supported"),
    }
}
