use std::fs;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use grammers_client::peer::User;
use grammers_client::{Client, SenderPool};
use grammers_session::storages::SqliteSession;
use tokio::task::JoinHandle;

use crate::config::Config;

/// A connected client plus the task driving its network I/O.
pub struct Telegram {
    pub client: Client,
    /// Shared with the client; used to look up cached peer access hashes.
    pub session: Arc<SqliteSession>,
    runner: JoinHandle<()>,
}

impl Telegram {
    pub async fn connect(config: &Config) -> Result<Self> {
        let path = &config.session_path;
        // The session file holds the account's auth key, so keep it owner-only.
        let dir = path.parent().unwrap_or(Path::new("."));
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder
            .create(dir)
            .with_context(|| format!("failed to create {}", dir.display()))?;

        let session = SqliteSession::open(path)
            .await
            .with_context(|| format!("failed to open session file {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))
                .with_context(|| format!("failed to set permissions on {}", path.display()))?;
        }

        // Updates are not consumed yet; dropping the receiver is safe (sends are ignored).
        let session = Arc::new(session);
        let SenderPool { runner, handle, .. } = SenderPool::new(session.clone(), config.api_id);
        let client = Client::new(handle);
        let runner = tokio::spawn(runner.run());

        Ok(Self {
            client,
            session,
            runner,
        })
    }

    /// Disconnects and waits for the network task to finish.
    pub async fn shutdown(self) {
        self.client.disconnect();
        let _ = self.runner.await;
    }
}

/// Human-readable account label, e.g. `Jane Doe (@jane, id 12345)`.
pub fn describe(user: &User) -> String {
    let id = user.id().bare_id_unchecked();
    match user.username() {
        Some(username) => format!("{} (@{username}, id {id})", user.full_name()),
        None => format!("{} (id {id})", user.full_name()),
    }
}
