use itertools::Itertools;
use sqlparser::ast::{Expr, FromTable, Statement, TableObject};
use tokio_postgres::types::Type;

use crate::{code_analysis::ColumnData, schema::Schema};

pub(crate) fn infer_output(stmt: &Statement, schema: &Schema) -> eyre::Result<Vec<ColumnData>> {
    match stmt {
        Statement::Delete(d) => match &d.returning {
            Some(rs) => {
                let FromTable::WithFromKeyword(from) = &d.from else {
                    unreachable!("big query syntax")
                };
                rs.iter()
                    .map(|r| resolve_select_item(r, schema, from))
                    .collect()
            }
            None => Ok(vec![]),
        },
        Statement::Insert(i) => match &i.returning {
            Some(rs) => {
                let TableObject::TableName(table_name) = &i.table else {
                    unreachable!("clickhouse syntax")
                };
                let tables = &[sqlparser::ast::TableWithJoins {
                    relation: sqlparser::ast::TableFactor::Table {
                        name: table_name.clone(),
                        alias: None,
                        args: None,
                        with_hints: vec![],
                        version: None,
                        with_ordinality: false,
                        partitions: vec![],
                        json_path: None,
                        sample: None,
                        index_hints: vec![],
                    },
                    joins: vec![],
                }];
                rs.iter()
                    .map(|r| resolve_select_item(r, schema, tables))
                    .collect()
            }
            None => Ok(vec![]),
        },
        Statement::Update {
            returning, table, ..
        } => match &returning {
            Some(rs) => {
                let tables = &[table.clone()];
                rs.iter()
                    .map(|r| resolve_select_item(r, schema, tables))
                    .collect()
            }
            None => Ok(vec![]),
        },
        Statement::Query(q) => match &*q.body {
            sqlparser::ast::SetExpr::Select(select) => select
                .projection
                .iter()
                .map(|p| resolve_select_item(p, schema, &select.from))
                .collect(),
            e => eyre::bail!("unsupported {e}"),
        },
        e => eyre::bail!("unsupported {e}"),
    }
}

fn resolve_select_item(
    si: &sqlparser::ast::SelectItem,
    schema: &Schema,
    f: &[sqlparser::ast::TableWithJoins],
) -> Result<ColumnData, eyre::Error> {
    dbg!(si);
    match si {
        sqlparser::ast::SelectItem::UnnamedExpr(expr) => match expr {
            Expr::Identifier(id) => {
                let column_name = &id.value;
                let table = f
                    .first()
                    .map(|f| match &f.relation {
                        sqlparser::ast::TableFactor::Table { name, .. } => {
                            let table_name = &name.0.first().unwrap().as_ident().unwrap().value;
                            let table = schema.find_table_by_name(table_name).unwrap();

                            Ok(table)
                        }
                        e => eyre::bail!("unsupported {e}"),
                    })
                    .unwrap()
                    .unwrap();

                let column = table.find_by_col_name(column_name).expect(column_name);

                Ok(ColumnData {
                    name: column_name.clone(),
                    type_: Type::from_oid(column.type_oid).unwrap(),
                    is_nullable: column.nullable,
                    table_oid: Some(table.oid),
                    column_position: Some(column.position),
                })
            }
            Expr::CompoundIdentifier(ids) => {
                let &[ref source, ref column] = ids.as_slice() else {
                    eyre::bail!("unsupported more then 2 ids");
                };
                let column_name = &column.value;
                let table = f
                    .iter()
                    .filter_map(|f| match &f.relation {
                        sqlparser::ast::TableFactor::Table { name, .. } => {
                            let table_name = &name.0.first().unwrap().as_ident().unwrap().value;
                            (&source.value == table_name)
                                .then(|| schema.find_table_by_name(table_name))
                                .flatten()
                        }
                        _ => None,
                    })
                    .exactly_one()
                    .unwrap();
                let column = table.find_by_col_name(column_name).expect(column_name);
                Ok(ColumnData {
                    name: column_name.clone(),
                    type_: Type::from_oid(column.type_oid).unwrap(),
                    is_nullable: column.nullable,
                    table_oid: Some(table.oid),
                    column_position: Some(column.position),
                })
            }
            e => eyre::bail!("unsupported {e}"),
        },
        e => eyre::bail!("unsupported {e}"),
    }
}
