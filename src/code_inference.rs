use sqlparser::ast::{Expr, Statement};
use tokio_postgres::types::Type;

use crate::{code_analysis::ColumnData, schema::Schema};

pub(crate) fn infer_output(stmt: &Statement, schema: &Schema) -> eyre::Result<Vec<ColumnData>> {
    match stmt {
        Statement::Query(q) => match &*q.body {
            sqlparser::ast::SetExpr::Select(select) => select
                .projection
                .iter()
                .map(|p| match p {
                    sqlparser::ast::SelectItem::UnnamedExpr(expr) => match expr {
                        Expr::Identifier(id) => {
                            let column_name = &id.value;
                            let table = select
                                .from
                                .first()
                                .map(|f| match &f.relation {
                                    sqlparser::ast::TableFactor::Table { name, .. } => {
                                        let table_name =
                                            &name.0.first().unwrap().as_ident().unwrap().value;
                                        let table = schema.find_table_by_name(table_name).unwrap();

                                        Ok(table)
                                    }
                                    e => eyre::bail!("unsupported {e}"),
                                })
                                .unwrap()
                                .unwrap();

                            let column = table.find_by_col_name(column_name).expect(column_name);

                            Ok(ColumnData {
                                name: id.value.clone(),
                                type_: Type::from_oid(column.type_oid).unwrap(),
                                is_nullable: column.nullable,
                                table_oid: Some(table.oid),
                                column_position: Some(column.position),
                            })
                        }
                        e => eyre::bail!("unsupported {e}"),
                    },
                    e => eyre::bail!("unsupported {e}"),
                })
                .collect(),
            e => eyre::bail!("unsupported {e}"),
        },
        e => eyre::bail!("unsupported {e}"),
    }
}

#[cfg(test)]
mod test {
    use sqlparser::ast::Statement;
    use tokio_postgres::types::Type;

    use crate::{
        code_analysis::ColumnData,
        schema::{Column, Schema, Table},
    };

    use super::infer_output;

    impl std::fmt::Debug for ColumnData {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "{}: {}{}",
                self.name,
                self.type_,
                if self.is_nullable { "?" } else { "" }
            )
        }
    }

    fn schema() -> Schema {
        Schema {
            tables: vec![Table {
                oid: 1,
                name: String::from("film"),
                columns: vec![
                    Column {
                        name: "film_id".to_string(),
                        type_oid: Type::INT4.oid(),
                        nullable: false,
                        position: 1,
                        is_unique: true,
                    },
                    Column {
                        name: "title".to_string(),
                        type_oid: Type::TEXT.oid(),
                        nullable: false,
                        position: 2,
                        is_unique: false,
                    },
                    Column {
                        name: "description".to_string(),
                        type_oid: Type::TEXT.oid(),
                        nullable: true,
                        position: 3,
                        is_unique: false,
                    },
                ],
            }],
        }
    }

    fn parse(r: &str) -> Statement {
        sqlparser::parser::Parser::parse_sql(&sqlparser::dialect::PostgreSqlDialect {}, r)
            .unwrap()
            .first()
            .unwrap()
            .clone()
    }

    #[test]
    fn basic() {
        let schema = schema();
        let stmt = parse("SELECT film_id, title, description FROM film");

        let ts = infer_output(&stmt, &schema).unwrap();
        insta::assert_debug_snapshot!(ts, @r"
        [
            film_id: int4,
            title: text,
            description: text?,
        ]
        ");
    }
}
