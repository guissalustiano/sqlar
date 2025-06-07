use eyre::ContextCompat;
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncWriteExt},
};

mod code_analysis;
mod code_gen;
mod code_inference;
mod schema;
#[cfg(test)]
mod test;

#[derive(clap::Parser, Debug)]
#[command(version, about, long_about)]
struct Args {
    /// Path to translate sql files to rust
    path: std::path::PathBuf,

    /// Database connection url.
    /// Can also be pass as env "POSTGRES_URL"
    #[arg(short = 'u', long, value_name = "URL")]
    postgres_url: Option<String>,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    env_logger::init();
    let cli: Args = clap::Parser::parse();
    let url = cli
        .postgres_url
        .or(std::env::var("POSTGRES_URL").ok())
        .context("Missing postgres_url")?;
    let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    let futs = walkdir::WalkDir::new(cli.path)
        .into_iter()
        .map(|entry| async {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "sql") || !entry.metadata()?.is_file() {
                log::debug!("skipping {path:?}");
                return Ok(());
            }
            log::info!("translating {path:?}");
            let mut sql = File::open(&path).await?;
            let mut rs = File::create(path.with_extension("rs")).await?;
            translate_file(&client, &mut sql, &mut rs).await
        });
    futures::future::try_join_all(futs).await?;

    Ok(())
}

async fn translate_file<I, O>(
    client: &impl tokio_postgres::GenericClient,
    sql: &mut I,
    rs: &mut O,
) -> eyre::Result<()>
where
    I: AsyncReadExt + Unpin,
    O: AsyncWriteExt + Unpin,
{
    let mut stmts_raw = String::new();
    sql.read_to_string(&mut stmts_raw).await?;

    let code = code_gen::gen_file(client, stmts_raw).await?;
    rs.write_all(code.as_bytes()).await?;

    Ok(())
}
