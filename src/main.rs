mod auth;
mod config;
mod server;
mod telegram;

use std::path::PathBuf;

use anyhow::{Context, Result};
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
    /// Run the MCP server over stdio (the default).
    Serve {
        /// Tools acting as you, on top of the read tools: `contacts` (join/leave chats,
        /// add/delete contacts) or `send` (contacts plus send, edit, delete, forward, react,
        /// pin, click buttons) [default: read only]
        #[arg(long, value_enum, value_name = "GROUP")]
        access: Option<server::Access>,
        /// Where download_media saves files [default: <downloads dir>/telegram-mcp]
        #[arg(long, value_name = "PATH")]
        download_dir: Option<PathBuf>,
    },
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

    let command = cli.command.unwrap_or(Command::Serve {
        access: None,
        download_dir: None,
    });
    match command {
        Command::Login => auth::login(&config).await,
        Command::Status => auth::status(&config).await,
        Command::Logout => auth::logout(&config).await,
        Command::Serve {
            access,
            download_dir,
        } => {
            let download_dir = match download_dir {
                Some(dir) => dir,
                None => dirs::download_dir()
                    .or_else(dirs::data_dir)
                    .context("cannot determine a download directory; pass --download-dir")?
                    .join("telegram-mcp"),
            };
            // Paths are reported to the agent, so make them absolute.
            let download_dir = std::path::absolute(download_dir)?;
            server::serve(&config, access, download_dir).await
        }
    }
}
