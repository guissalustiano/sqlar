use eyre::ContextCompat;
use sqlparser::ast::{
    Assignment, AssignmentTarget, BinaryOperator, Expr, ObjectName, ObjectNamePart, SetExpr,
    Statement, Value, ValueWithSpan,
};

use crate::schema::Schema;

pub struct InputData {
    pub name: String,
    pub type_: tokio_postgres::types::Type,
}
pub struct ColumnData {
    pub name: String,
    pub type_: tokio_postgres::types::Type,
    pub is_nullable: bool,
}

pub enum ClientMethod {
    Query,
    Execute,
}

pub struct PrepareStatement {
    pub name: String,
    pub statement: Box<Statement>,
    pub parameter_types: Vec<InputData>,
    pub result_types: Vec<ColumnData>,
    pub client_method: ClientMethod,
}

pub(crate) async fn prepare_stmts(
    client: &impl tokio_postgres::GenericClient,
    stmts_raw: &str,
) -> eyre::Result<Vec<PrepareStatement>> {
    let schema = crate::schema::load_schema(client).await?;
    let stmts =
        sqlparser::parser::Parser::parse_sql(&sqlparser::dialect::PostgreSqlDialect {}, stmts_raw)?;

    let futs = stmts.into_iter().map(|stmt| async {
        let Statement::Prepare {
            name,
            data_types: _,
            statement,
        } = stmt
        else {
            eyre::bail!("sql files should contains only prepare statements, found {stmt}");
        };
        let ps = client.prepare(&statement.to_string()).await?;

        Ok(PrepareStatement {
            name: name.value,
            client_method: calc_client_method(&ps, &statement),
            parameter_types: ps
                .params()
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    Ok(InputData {
                        name: name_from_statement(&statement, i + 1)?.context("param not found")?,
                        type_: t.clone(),
                    })
                })
                .collect::<eyre::Result<_>>()?,
            result_types: infer_result_types(&ps, &statement, &schema)?,
            statement,
        })
    });

    futures::future::try_join_all(futs).await
}

fn infer_result_types(
    ps: &tokio_postgres::Statement,
    stmt: &Statement,
    schema: &Schema,
) -> eyre::Result<Vec<ColumnData>> {
    match stmt {
        // DML with returning clausure
        Statement::Delete(_) | Statement::Insert(_) | Statement::Update { .. }
            if !ps.columns().is_empty() =>
        {
            ps.columns()
                .iter()
                .map(|c| {
                    let sc = schema
                        .find_column_by_id(
                            c.table_oid().context("table_oid empty")?,
                            c.column_id().context("column empty")?,
                        )
                        .context("column not found")?;

                    Ok(ColumnData {
                        name: c.name().to_owned(),
                        type_: c.type_().to_owned(),
                        is_nullable: sc.nullable,
                    })
                })
                .collect()
        }
        _ => ps
            .columns()
            .iter()
            .map(|c| {
                Ok(ColumnData {
                    name: c.name().to_owned(),
                    type_: c.type_().to_owned(),
                    is_nullable: true,
                })
            })
            .collect(),
    }
}

fn calc_client_method(ps: &tokio_postgres::Statement, stmt: &Statement) -> ClientMethod {
    match stmt {
        Statement::Delete(_) | Statement::Insert(_) | Statement::Update { .. }
            if ps.columns().is_empty() =>
        {
            ClientMethod::Execute
        }
        _ => ClientMethod::Query,
    }
}

fn name_from_statement(stmt: &Statement, i: usize) -> eyre::Result<Option<String>> {
    match stmt {
        Statement::Query(q) => match *q.body {
            SetExpr::Select(ref select) => select
                .selection
                .as_ref()
                .and_then(|s| name_from_expr(s, i).transpose())
                .transpose(),
            _ => eyre::bail!("not supported yet"),
        },
        Statement::Delete(delete) => delete
            .selection
            .as_ref()
            .and_then(|s| name_from_expr(s, i).transpose())
            .transpose(),
        Statement::Update {
            selection,
            assignments,
            ..
        } => selection
            .as_ref()
            .and_then(|s| name_from_expr(s, i).transpose())
            .or_else(|| {
                assignments.iter().find_map(|a| {
                    {
                        let a: &Assignment = a;
                        is_placehold(&a.value, i)
                            .then(|| {
                                Ok(match &a.target {
                                    AssignmentTarget::ColumnName(ObjectName(os)) => {
                                        std::iter::once("set")
                                            .chain(os.iter().map(|o| match o {
                                                ObjectNamePart::Identifier(ident) => {
                                                    ident.value.as_str()
                                                }
                                            }))
                                            .collect::<Vec<&str>>()
                                            .join("_")
                                    }
                                    AssignmentTarget::Tuple(_) => {
                                        eyre::bail!("{} with tuple is not supported yet", a.target)
                                    }
                                })
                            })
                            .transpose()
                    }
                    .transpose()
                })
            })
            .transpose(),
        Statement::Insert(insert) => insert
            .source
            .as_ref()
            .and_then(|q| {
                match *q.body {
                    SetExpr::Values(ref v) => Ok(v.rows.iter().find_map(|row| {
                        row.iter()
                            .zip(&insert.columns)
                            .find_map(|(v, c)| is_placehold(v, i).then(|| c.value.clone()))
                    })),
                    _ => Err(eyre::eyre!("insert statemenr not found")),
                }
                .transpose()
            })
            .transpose(),
        _ => eyre::bail!("statement not supported"),
    }
}

fn is_placehold(e: &Expr, i: usize) -> bool {
    if let Expr::Value(ValueWithSpan {
        value: Value::Placeholder(p),
        span: _,
    }) = e
    {
        *p == format!("${i}")
    } else {
        false
    }
}

fn name_from_expr(expr: &Expr, i: usize) -> eyre::Result<Option<String>> {
    fn name_expr(e: &Expr) -> eyre::Result<String> {
        Ok(match e {
            Expr::CompoundIdentifier(idents) => idents
                .iter()
                .map(|i| i.value.as_str())
                .collect::<Vec<_>>()
                .join("_"),
            Expr::Identifier(ident) => ident.value.to_owned(),
            _ => eyre::bail!("{e} not supported yet"),
        })
    }
    fn name_op(op: &sqlparser::ast::BinaryOperator) -> eyre::Result<&str> {
        Ok(match op {
            BinaryOperator::Eq => "eq",
            BinaryOperator::PGLikeMatch => "like",
            BinaryOperator::Gt => "gt",
            BinaryOperator::Lt => "lt",
            BinaryOperator::GtEq => "ge",
            BinaryOperator::LtEq => "le",
            _ => eyre::bail!("op {op} not supported yet"),
        })
    }
    match expr {
        Expr::Identifier(_) | Expr::Value(_) => Ok(None),
        Expr::BinaryOp { left, op, right } if is_placehold(&left, i) => {
            Ok(Some(format!("{}_{}", name_op(op)?, name_expr(&right)?)))
        }
        Expr::BinaryOp { left, op, right } if is_placehold(right, i) => {
            Ok(Some(format!("{}_{}", name_op(op)?, name_expr(&left)?)))
        }
        Expr::BinaryOp { left, op: _, right } => name_from_expr(&left, i)
            .transpose()
            .or_else(|| name_from_expr(&right, i).transpose())
            .transpose(),
        Expr::Like {
            negated: _,
            any: _,
            expr,
            pattern,
            escape_char: _,
        } if is_placehold(&pattern, i) => Ok(Some(format!(
            "{}_{}",
            name_op(&BinaryOperator::PGLikeMatch)?,
            name_expr(&expr)?
        ))),
        _ => eyre::bail!("{expr} not supported yet"),
    }
}
