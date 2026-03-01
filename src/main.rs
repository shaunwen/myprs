mod bitbucket;
mod config;
mod tui;

use anyhow::Result;
use clap::Parser;
use config::{Config, PrStatus};

#[derive(Debug, Parser)]
#[command(
    name = "myprs",
    version,
    about = "Bitbucket PR TUI for your authored PRs"
)]
struct Cli {
    #[arg(long = "repo", help = "Repository in workspace/repo format", num_args = 1..)]
    repos: Vec<String>,
    #[arg(long)]
    email: Option<String>,
    #[arg(long = "api-token")]
    api_token: Option<String>,
    #[arg(long)]
    status: Option<PrStatus>,
    #[arg(long = "base-url")]
    base_url: Option<String>,
    #[arg(long = "auto-refresh-seconds")]
    auto_refresh_seconds: Option<u64>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut config = Config::load()?;

    config.apply_env_and_cli(
        cli.repos,
        cli.email,
        cli.api_token,
        cli.status,
        cli.base_url,
        cli.auto_refresh_seconds,
    )?;

    tui::run_app(config)
}
