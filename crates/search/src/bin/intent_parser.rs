use std::io::{self, Read};

use anyhow::{bail, Context, Result};
use data_search::intent::IntentParser;

fn read_query() -> Result<String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        return Ok(args.join(" "));
    }

    let mut stdin = String::new();
    io::stdin()
        .read_to_string(&mut stdin)
        .context("read query from stdin")?;
    let query = stdin.trim().to_string();

    if query.is_empty() {
        bail!("usage: cargo run -p data-search --bin intent_parser -- \"<nl query>\"");
    }

    Ok(query)
}

#[tokio::main]
async fn main() -> Result<()> {
    let query = read_query()?;
    let parser = IntentParser::default();
    let profile = parser.profile(&query).await?;

    Ok(())
}
