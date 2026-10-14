mod auth;
mod config;
mod server;
mod telegram;

use std::env;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use crate::config::{Config, SESSION_STRING_ENV};

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
    /// Print the session as one line, for TELEGRAM_MCP_SESSION_STRING where no file can go.
    /// The line logs in as the account: pipe it into a secret store.
    ExportSession,
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
    // A session handed over as a line, where a file can't be shipped (a container whose
    // image must not hold the account). A file that is already there wins.
    if let Ok(line) = env::var(SESSION_STRING_ENV)
        && !line.trim().is_empty()
        && !config.session_path.exists()
    {
        auth::import_session(&config.session_path, &line).await?;
    }

    let command = cli
        .command
        .unwrap_or_else(|| Command::Serve(server::ServeArgs::parse_from(["serve"])));
    match command {
        Command::Login => auth::login(&config).await,
        Command::Status => auth::status(&config).await,
        Command::Logout => auth::logout(&config).await,
        Command::ExportSession => auth::export_session(&config).await,
        Command::Serve(args) => server::serve(&config, args).await,
    }
}
