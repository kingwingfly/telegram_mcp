use std::env;
use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

const SESSION_ENV: &str = "TELEGRAM_MCP_SESSION";

/// Telegram API hash. Never printed: `Debug` is redacted and there is no `Display`.
pub struct ApiHash(String);

impl ApiHash {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ApiHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiHash(<redacted>)")
    }
}

#[derive(Debug)]
pub struct Config {
    pub api_id: i32,
    api_hash: Option<ApiHash>,
    pub session_path: PathBuf,
    /// The `.env` file that was loaded, if any.
    pub env_path: Option<PathBuf>,
}

impl Config {
    /// Loads `.env` (from `env_file`, or searched upward from the current directory),
    /// then reads `TELEGRAM_API_ID`, `TELEGRAM_API_HASH` and the session path from the environment.
    pub fn load(env_file: Option<&Path>, session: Option<PathBuf>) -> Result<Self> {
        let env_path = match env_file {
            Some(path) => {
                dotenvy::from_path(path)
                    .with_context(|| format!("failed to load env file {}", path.display()))?;
                Some(path.to_owned())
            }
            None => match dotenvy::dotenv() {
                Ok(path) => Some(path),
                Err(e) if e.not_found() => None,
                Err(e) => return Err(e).context("failed to load .env"),
            },
        };

        let api_id = env::var("TELEGRAM_API_ID")
            .map_err(|_| anyhow!("TELEGRAM_API_ID is not set (put it in .env or the environment)"))?
            .trim()
            .parse()
            .context("TELEGRAM_API_ID must be an integer")?;

        let api_hash = env::var("TELEGRAM_API_HASH")
            .ok()
            .map(|h| h.trim().to_owned())
            .filter(|h| !h.is_empty())
            .map(ApiHash);

        let session_path = match session.or_else(|| env::var_os(SESSION_ENV).map(PathBuf::from)) {
            Some(path) => path,
            None => dirs::data_dir()
                .context("cannot determine a data directory; pass --session")?
                .join("telegram-mcp")
                .join("telegram.session"),
        };

        Ok(Self {
            api_id,
            api_hash,
            session_path,
            env_path,
        })
    }

    /// The API hash is only needed to request a login code.
    pub fn api_hash(&self) -> Result<&ApiHash> {
        self.api_hash.as_ref().ok_or_else(|| {
            anyhow!("TELEGRAM_API_HASH is not set (put it in .env or the environment)")
        })
    }
}
