use std::collections::HashMap;

use eyre::eyre;
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
    let tables = f
        .iter()
        .flat_map(|f| std::iter::once(resolve_tables(schema, &f.relation)))
        .collect::<HashMap<_, _>>();

    let columns = tables
        .values()
        .flat_map(|t| t.columns.iter().map(move |c| (c.name.as_str(), (t, c))))
        .collect::<HashMap<_, _>>();

    match si {
        sqlparser::ast::SelectItem::UnnamedExpr(expr) => match expr {
            Expr::Identifier(id) => {
                let (table, column) = columns.get(id.value.as_str()).expect(&id.value);
                Ok(ColumnData {
                    name: column.name.clone(),
                    type_: Type::from_oid(column.type_oid).unwrap(),
                    is_nullable: column.nullable,
                    table_oid: Some(table.oid),
                    column_position: Some(column.position),
                })
            }
            Expr::CompoundIdentifier(ids) => {
                let &[ref table_id, ref column_id] = ids.as_slice() else {
                    eyre::bail!("unsupported more then 2 ids");
                };
                let table = tables.get(table_id.value.as_str()).expect(&table_id.value);
                let column = table
                    .find_by_col_name(&column_id.value)
                    .expect(&column_id.value);
                Ok(ColumnData {
                    name: column.name.clone(),
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

fn resolve_tables<'a>(
    schema: &'a Schema,
    t: &'a sqlparser::ast::TableFactor,
) -> (&'a str, &'a crate::schema::Table) {
    match t {
        sqlparser::ast::TableFactor::Table { name, alias, .. } => {
            let table_name = name.0.first().unwrap().as_ident().unwrap().value.as_str();
            let table = schema
                .find_table_by_name(table_name)
                .ok_or_else(|| eyre!("table {table_name} not found on schema"))
                .unwrap();
            match alias {
                Some(alias) => (alias.name.value.as_str(), table),
                None => (table_name, table),
            }
        }

        _ => todo!(),
    }
}
