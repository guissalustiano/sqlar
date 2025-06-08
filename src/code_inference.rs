use std::collections::HashMap;

use eyre::eyre;
use sqlparser::ast::{
    CharacterLength, Expr, FromTable, JoinOperator, SelectItem, Statement, TableObject,
    TimezoneInfo,
};
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
    ts: &[sqlparser::ast::TableWithJoins],
) -> Result<ColumnData, eyre::Error> {
    let tables = ts
        .iter()
        .flat_map(|t| {
            std::iter::once(resolve_tables(schema, &t.relation)).chain(t.joins.iter().map(|t| {
                let schema = match &t.join_operator {
                    JoinOperator::Join(_) | JoinOperator::Inner(_) | JoinOperator::CrossJoin => {
                        schema
                    }
                    JoinOperator::Left(_) | JoinOperator::LeftOuter(_) => {
                        Box::leak(Box::new(schema.all_nullable()))
                    }
                    _ => todo!(),
                };
                resolve_tables(schema, &t.relation)
            }))
        })
        .collect::<HashMap<_, _>>();

    let columns = tables
        .values()
        .flat_map(|t| t.columns.iter().map(move |c| (c.name.as_str(), (t, c))))
        .collect::<HashMap<_, _>>();

    match si {
        SelectItem::UnnamedExpr(expr) => resolve_expr(&tables, &columns, expr),
        SelectItem::ExprWithAlias { expr, alias } => {
            resolve_expr(&tables, &columns, expr).map(|c| c.with_name(alias.value.clone()))
        }
        e => eyre::bail!("unsupported {e}"),
    }
}

fn resolve_expr(
    tables: &HashMap<&str, &crate::schema::Table>,
    columns: &HashMap<&str, (&&crate::schema::Table, &crate::schema::Column)>,
    expr: &Expr,
) -> Result<ColumnData, eyre::Error> {
    match expr {
        Expr::Identifier(id) => {
            let (_table, column) = columns.get(id.value.as_str()).expect(&id.value);
            Ok(ColumnData {
                name: column.name.clone(),
                type_: Type::from_oid(column.type_oid).unwrap(),
                is_nullable: column.nullable,
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
            })
        }
        Expr::Cast {
            kind: _,
            expr,
            data_type,
            format: _,
        } => resolve_expr(tables, columns, expr).map(|c| c.with_type(to_pg_type(data_type))),
        Expr::Value(v) => {
            let (type_, is_nullable) = match &v.value {
                sqlparser::ast::Value::Number(v, _) => {
                    if v.parse::<i32>().is_ok() {
                        (Type::INT4, false)
                    } else {
                        (Type::NUMERIC, false)
                    }
                }
                sqlparser::ast::Value::SingleQuotedString(_)
                | sqlparser::ast::Value::DollarQuotedString(_)
                | sqlparser::ast::Value::EscapedStringLiteral(_)
                | sqlparser::ast::Value::UnicodeStringLiteral(_)
                | sqlparser::ast::Value::SingleQuotedByteStringLiteral(_)
                | sqlparser::ast::Value::DoubleQuotedByteStringLiteral(_)
                | sqlparser::ast::Value::NationalStringLiteral(_)
                | sqlparser::ast::Value::HexStringLiteral(_)
                | sqlparser::ast::Value::DoubleQuotedString(_) => (Type::TEXT, false),
                sqlparser::ast::Value::Boolean(_) => (Type::BOOL, false),
                sqlparser::ast::Value::Null => (Type::TEXT, true), // TODO: This should be a never type
                sqlparser::ast::Value::Placeholder(_) => todo!("place_holder_prepare"),
                _ => {
                    unreachable!("not supported on postgres")
                }
            };
            Ok(ColumnData {
                type_,
                name: format!("_{}", v.value),
                is_nullable,
            })
        }
        e => eyre::bail!("unsupported {e}"),
    }
}

fn to_pg_type(data_type: &sqlparser::ast::DataType) -> Type {
    use sqlparser::ast::DataType::*;
    match data_type {
        Char(None)
        | Char(Some(CharacterLength::IntegerLength { length: 1, unit: _ }))
        | Character(None)
        | Character(Some(CharacterLength::IntegerLength { length: 1, unit: _ })) => Type::CHAR,
        Char(_) | Character(_) => Type::BPCHAR,
        CharacterVarying(_) | CharVarying(_) | Varchar(_) => Type::VARCHAR,
        Uuid => Type::UUID,
        Bytea => Type::BYTEA,
        Int2(None) | SmallInt(None) => todo!(),
        Int(None) | Int4(None) | Integer(None) => Type::INT4,
        Int8(None) | BigInt(None) => todo!(),
        Float4 | Real => Type::FLOAT4,
        Float8 | DoublePrecision => Type::FLOAT8,
        Bool | Boolean => Type::BOOL,
        Date => Type::DATE,
        Time(_, TimezoneInfo::None | TimezoneInfo::WithoutTimeZone) => Type::TIME,
        Time(_, TimezoneInfo::Tz | TimezoneInfo::WithTimeZone) => Type::TIMETZ,
        Timestamp(_, TimezoneInfo::None | TimezoneInfo::WithoutTimeZone) => Type::TIMESTAMP,
        Timestamp(_, TimezoneInfo::Tz | TimezoneInfo::WithTimeZone) => Type::TIMESTAMPTZ,
        Interval => Type::INTERVAL,
        JSON => Type::JSON,
        JSONB => Type::JSONB,
        Numeric(_) | Decimal(_) => Type::NUMERIC,
        Text => Type::TEXT,
        Bit(None) => Type::BIT,
        Bit(_) | BitVarying(_) | VarBit(_) => Type::VARBIT,
        Custom(_object_name, _items) => todo!("custom type"),
        Array(_array_elem_type_def) => todo!("array"),
        Enum(_, _) | Trigger => todo!("wtf is that"),
        Regclass => todo!("wtf is that?"),
        GeometricType(_) => todo!("wtf is that?"),
        Table(_) => unreachable!("not used in this context"),
        _ => {
            unreachable!("not supported on postgres")
        }
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
