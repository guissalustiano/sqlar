#![allow(dead_code)]
mod query;

use eyre::ContextCompat;

#[derive(Debug)]
pub struct Column {
    pub name: String,
    pub type_oid: tokio_postgres::types::Oid,
    pub nullable: bool,
    pub position: i16,
    pub is_unique: bool,
}

#[derive(Debug)]
pub struct Table {
    pub oid: tokio_postgres::types::Oid,
    pub name: String,
    pub columns: Vec<Column>,
}
impl Table {
    pub(crate) fn find_by_col_id(&self, column_id: i16) -> Option<&Column> {
        self.columns.iter().find(|c| c.position == column_id)
    }

    pub(crate) fn find_by_col_name(&self, column_name: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.name == column_name)
    }
}
#[derive(Debug)]
pub struct Schema {
    pub tables: Vec<Table>,
}
impl Schema {
    pub(crate) fn find_column_by_id(
        &self,
        table_oid: tokio_postgres::types::Oid,
        column_id: i16,
    ) -> Option<&Column> {
        self.tables.iter().find_map(|t| {
            (t.oid == table_oid)
                .then(|| t.find_by_col_id(column_id))
                .flatten()
        })
    }

    pub(crate) fn find_table_by_name(&self, name: &str) -> Option<&Table> {
        self.tables.iter().find(|t| t.name == name)
    }
}

pub async fn load_schema(c: &impl tokio_postgres::GenericClient) -> eyre::Result<Schema> {
    query::load_schema(c)
        .await?
        .into_iter()
        .map(|r| {
            Ok(Table {
                oid: r.oid.context("oid")?,
                name: r.table.context("table")?,
                columns: r
                    .column
                    .context("column")?
                    .into_iter()
                    .zip(r.type_oid.context("type_oid")?)
                    .zip(r.nullable.context("nullable")?)
                    .zip(r.column_position.context("column_position")?)
                    .zip(r.has_unique_index.context("has_unique_index")?)
                    .map(
                        |((((name, type_oid), nullable), position), is_unique)| Column {
                            name,
                            type_oid,
                            nullable,
                            position,
                            is_unique,
                        },
                    )
                    .collect(),
            })
        })
        .collect::<eyre::Result<_>>()
        .map(|tables| Schema { tables })
}
