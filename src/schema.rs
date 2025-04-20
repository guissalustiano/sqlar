#![allow(dead_code)]
mod query;

use eyre::ContextCompat;

pub struct Column {
    pub name: String,
    pub type_oid: tokio_postgres::types::Oid,
    pub nullable: bool,
}

pub struct Table {
    pub oid: tokio_postgres::types::Oid,
    pub name: String,
    pub columns: Vec<Column>,
}
pub struct Schema {
    pub tables: Vec<Table>,
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
                    .map(|((name, type_oid), nullable)| Column {
                        name,
                        type_oid,
                        nullable,
                    })
                    .collect(),
            })
        })
        .collect::<eyre::Result<_>>()
        .map(|tables| Schema { tables })
}
