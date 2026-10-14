mod auth;
mod config;
mod server;
mod telegram;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use crate::config::Config;

/// MCP server for Telegram, acting as your user account.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Load TELEGRAM_API_ID / TELEGRAM_API_HASH from this file instead of searching for `.env`.
    #[arg(long, global = true, value_name = "PATH")]
    env_file: Option<PathBuf>,

    /// Session file [env: TELEGRAM_MCP_SESSION] [default: <data dir>/telegram-mcp/telegram.session]
    #[arg(long, global = true, value_name = "PATH")]
    session: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Log in interactively (phone number, code, 2FA password).
    Login,
    /// Show which account the session is logged in as.
    Status,
    /// Sign out and delete the session file.
    Logout,
    /// Run the MCP server (the default): over stdio, or over HTTP with `--listen`.
    Serve(server::ServeArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // stdout carries the MCP protocol, so logs must go to stderr.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("warn,telegram_mcp=info")),
        )
        .init();

    let config = Config::load(cli.env_file.as_deref(), cli.session)?;

    let command = cli
        .command
        .unwrap_or_else(|| Command::Serve(server::ServeArgs::parse_from(["serve"])));
    match command {
        Command::Login => auth::login(&config).await,
        Command::Status => auth::status(&config).await,
        Command::Logout => auth::logout(&config).await,
        Command::Serve(args) => server::serve(&config, args).await,
    }
}
