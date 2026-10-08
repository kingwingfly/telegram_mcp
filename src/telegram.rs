use std::fs;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use grammers_client::peer::User;
use grammers_client::{Client, SenderPool};
use grammers_session::BoxFuture;
use grammers_session::storages::{SqliteSession, SqliteSessionError};
use grammers_session::types::{DcOption, PeerId, PeerInfo, UpdateState, UpdatesState};
use tokio::task::JoinHandle;

use crate::config::Config;

/// A connected client plus the task driving its network I/O.
pub struct Telegram {
    pub client: Client,
    /// Shared with the client; used to look up cached peer access hashes.
    pub session: Arc<Session>,
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

        let created = !path.exists();
        let session = SqliteSession::open(path)
            .await
            .with_context(|| format!("failed to open session file {}", path.display()))?;
        // Only on creation: an existing file's permissions are the owner's choice. Another
        // account may share it on purpose (e.g. through an ACL); it can't chmod a file it
        // doesn't own, and a chmod by the owner would reset the ACL mask and cut it off.
        #[cfg(unix)]
        if created {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))
                .with_context(|| format!("failed to set permissions on {}", path.display()))?;
        }

        // Updates are not consumed yet; dropping the receiver is safe (sends are ignored).
        let session = Arc::new(Session(session));
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

/// `SqliteSession` with the trait's one-by-one `cache_peers`. The SQLite batch version
/// (grammers git e7730ae) reuses its statement without a reset, so it writes only the first peer
/// of each batch and silently skips the rest: new peers are never stored, and cached ones never
/// get missing fields (such as an access hash) filled in. Access hashes of most chats in a
/// response were lost, and their ids failed in later requests.
pub struct Session(SqliteSession);

impl grammers_session::Session for Session {
    type Error = SqliteSessionError;

    fn home_dc_id(&self) -> Result<i32, Self::Error> {
        self.0.home_dc_id()
    }

    fn set_home_dc_id(&self, dc_id: i32) -> BoxFuture<'_, Result<(), Self::Error>> {
        self.0.set_home_dc_id(dc_id)
    }

    fn dc_option(&self, dc_id: i32) -> Result<Option<DcOption>, Self::Error> {
        self.0.dc_option(dc_id)
    }

    fn set_dc_option(&self, dc_option: &DcOption) -> BoxFuture<'_, Result<(), Self::Error>> {
        self.0.set_dc_option(dc_option)
    }

    fn peer(&self, peer: PeerId) -> BoxFuture<'_, Result<Option<PeerInfo>, Self::Error>> {
        self.0.peer(peer)
    }

    fn cache_peer(&self, peer: PeerInfo) -> BoxFuture<'_, Result<(), Self::Error>> {
        self.0.cache_peer(peer)
    }

    fn updates_state(&self) -> BoxFuture<'_, Result<UpdatesState, Self::Error>> {
        self.0.updates_state()
    }

    fn set_update_state(&self, update: UpdateState) -> BoxFuture<'_, Result<(), Self::Error>> {
        self.0.set_update_state(update)
    }
}
