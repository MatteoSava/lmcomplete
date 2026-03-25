pub mod cli;
pub mod commands;
pub mod config;
pub mod context;
pub mod prompt;
pub mod provider;
pub mod redaction;
pub mod safety;
pub mod stats;

use anyhow::Result;

pub async fn run() -> Result<()> {
    let cli = cli::parse();

    match cli.command {
        cli::Commands::Expand(args) => commands::expand::run(args, cli.config.as_deref()).await,
        cli::Commands::Explain(args) => commands::explain::run(args, cli.config.as_deref()).await,
        cli::Commands::Audit(args) => commands::audit::run(args, cli.config.as_deref()),
        cli::Commands::Init(args) => commands::init::run(args),
        cli::Commands::Stats => commands::stats::run(),
    }
}
