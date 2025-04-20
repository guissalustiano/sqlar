use eyre::ContextCompat;
use sqlparser::ast::{BinaryOperator, Expr, Statement, Value, ValueWithSpan};

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
                    name: find_param_node(&statement, i + 1)?.context("param not found")?,
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
                expr_find(selection, index)
            }
            _ => eyre::bail!("not supported yet"),
        },
        Statement::Insert(_) | Statement::Update { .. } | Statement::Delete(_) => {
            eyre::bail!("statement not supported yet")
        }
        _ => eyre::bail!("statement not supported"),
    }
}

fn expr_find(expr: &Expr, i: usize) -> eyre::Result<Option<String>> {
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
        Expr::BinaryOp { left, op: _, right } => {
            if let Some(r) = expr_find(&left, i)? {
                return Ok(Some(r));
            }
            expr_find(&right, i)
        }
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
