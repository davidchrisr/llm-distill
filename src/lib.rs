pub mod cli;
pub mod config_file;
pub mod distiller;
pub mod openai;
pub mod output_collector;
pub mod prompt;
pub mod settings;
pub mod text;

use anyhow::Result;
use clap::Parser;

pub async fn run() -> Result<()> {
    let cli = cli::Cli::parse();
    distiller::run(cli).await
}
