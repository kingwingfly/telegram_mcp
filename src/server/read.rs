use grammers_client::media::Media;
use grammers_client::peer::{Peer, Role};
use grammers_client::tl;
use grammers_session::types::PeerKind;
use jiff::Timestamp;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    ChatInfo, ChatParams, MessageParams, Messages, TelegramMcp, chat_info, fail, message_info,
};

#[derive(Deserialize, JsonSchema)]
struct PageParams {
    /// Maximum results (default 20, max 100).
    limit: Option<usize>,
    /// Number of results to skip, for paging.
    offset: Option<usize>,
}

#[derive(Serialize, JsonSchema)]
struct ChatSummary {
    #[serde(flatten)]
    chat: ChatInfo,
    unread_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_message_date: Option<String>,
    /// First 100 characters of the last message.
    #[serde(skip_serializing_if = "Option::is_none")]
    last_message_preview: Option<String>,
}

#[derive(Serialize, JsonSchema)]
struct Chats {
    chats: Vec<ChatSummary>,
}

#[derive(Deserialize, JsonSchema)]
struct GetMessagesParams {
    /// `me`, `@username`, a t.me link, or a numeric id from list_chats.
    chat: String,
    /// Maximum messages, newest first (default 20, max 100).
    limit: Option<usize>,
    /// Only messages older than this message id, for paging.
    before_id: Option<i32>,
    /// Fetch exactly these message ids instead; `limit` and `before_id` are ignored.
    ids: Option<Vec<i32>>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ContentFilter {
    Photos,
    Videos,
    PhotosAndVideos,
    Documents,
    Links,
    Gifs,
    Voice,
    Music,
    RoundVideos,
    Polls,
    Locations,
    Contacts,
    Pinned,
    Mentions,
}

#[derive(Deserialize, JsonSchema)]
struct SearchMessagesParams {
    /// Text to search for. May be empty when `filter` is set.
    #[serde(default)]
    query: String,
    /// Search only this chat. Omit to search all chats.
    chat: Option<String>,
    /// Only messages with this kind of content.
    filter: Option<ContentFilter>,
    /// Only messages sent by you. Requires `chat`; not supported in Saved Messages.
    #[serde(default)]
    from_me: bool,
    /// Only messages sent after this time (RFC 3339, or YYYY-MM-DD in UTC). Requires `chat`.
    since: Option<String>,
    /// Only messages sent before this time (RFC 3339, or YYYY-MM-DD in UTC). Requires `chat`.
    until: Option<String>,
    /// Maximum results, newest first (default 20, max 100).
    limit: Option<usize>,
    /// Only messages older than this message id, for paging. Requires `chat`.
    before_id: Option<i32>,
}

#[derive(Serialize, JsonSchema)]
struct ChatDetails {
    #[serde(flatten)]
    chat: ChatInfo,
    /// Bio or description.
    #[serde(skip_serializing_if = "Option::is_none")]
    about: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    members: Option<i32>,
}

#[derive(Deserialize, JsonSchema)]
struct ListParticipantsParams {
    /// `@username`, a t.me link, or a numeric id of a group or channel.
    chat: String,
    #[serde(flatten)]
    page: PageParams,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum MemberRole {
    Member,
    Creator,
    Admin,
    Banned,
    Left,
}

#[derive(Serialize, JsonSchema)]
struct Member {
    #[serde(flatten)]
    user: ChatInfo,
    role: MemberRole,
}

#[derive(Serialize, JsonSchema)]
struct Members {
    members: Vec<Member>,
}

#[derive(Serialize, JsonSchema)]
struct Downloaded {
    /// Absolute path of the saved file.
    path: String,
}

#[tool_router(router = read_tools, vis = "pub(super)")]
impl TelegramMcp {
    #[tool(
        description = "List your chats, most recent first, with unread counts and a preview of the last message.",
        annotations(read_only_hint = true, open_world_hint = true)
    )]
    async fn list_chats(
        &self,
        Parameters(params): Parameters<PageParams>,
    ) -> Result<Json<Chats>, String> {
        let offset = params.offset.unwrap_or(0);
        let limit = params.limit.unwrap_or(20).min(100);
        let mut dialogs = self.client.iter_dialogs().limit(offset + limit);
        let mut chats = Vec::with_capacity(limit);
        let mut skipped = 0;
        while let Some(dialog) = dialogs.next().await.map_err(fail)? {
            if skipped < offset {
                skipped += 1;
                continue;
            }
            let unread_count = match &dialog.raw {
                tl::enums::Dialog::Dialog(dialog) => dialog.unread_count,
                _ => 0,
            };
            let last = dialog.last_message.as_ref();
            chats.push(ChatSummary {
                chat: chat_info(dialog.peer()),
                unread_count,
                last_message_date: last.map(|message| message.date().to_string()),
                last_message_preview: last
                    .map(|message| message.text().chars().take(100).collect()),
            });
        }
        Ok(Json(Chats { chats }))
    }

    #[tool(
        description = "Get messages from a chat, newest first, or specific messages by id.",
        annotations(read_only_hint = true, open_world_hint = true)
    )]
    async fn get_messages(
        &self,
        Parameters(params): Parameters<GetMessagesParams>,
    ) -> Result<Json<Messages>, String> {
        let peer = self.resolve(&params.chat).await?;
        let messages = match params.ids {
            Some(ids) => self
                .client
                .get_messages_by_id(peer, &ids)
                .await
                .map_err(fail)?
                .iter()
                .flatten()
                // Deleted or missing ids come back as empty placeholders.
                .filter(|message| !matches!(message.raw, tl::enums::Message::Empty(_)))
                .map(|message| message_info(message, self.me))
                .collect(),
            None => {
                let mut iter = self
                    .client
                    .iter_messages(peer)
                    .limit(params.limit.unwrap_or(20).min(100));
                if let Some(id) = params.before_id {
                    iter = iter.offset_id(id);
                }
                let mut messages = Vec::new();
                while let Some(message) = iter.next().await.map_err(fail)? {
                    messages.push(message_info(&message, self.me));
                }
                messages
            }
        };
        Ok(Json(Messages { messages }))
    }

    #[tool(
        description = "Search messages by text and/or content type, in one chat or across all chats. Newest first.",
        annotations(read_only_hint = true, open_world_hint = true)
    )]
    async fn search_messages(
        &self,
        Parameters(params): Parameters<SearchMessagesParams>,
    ) -> Result<Json<Messages>, String> {
        use tl::enums::MessagesFilter as F;
        let filter = params.filter.map(|filter| match filter {
            ContentFilter::Photos => F::InputMessagesFilterPhotos,
            ContentFilter::Videos => F::InputMessagesFilterVideo,
            ContentFilter::PhotosAndVideos => F::InputMessagesFilterPhotoVideo,
            ContentFilter::Documents => F::InputMessagesFilterDocument,
            ContentFilter::Links => F::InputMessagesFilterUrl,
            ContentFilter::Gifs => F::InputMessagesFilterGif,
            ContentFilter::Voice => F::InputMessagesFilterVoice,
            ContentFilter::Music => F::InputMessagesFilterMusic,
            ContentFilter::RoundVideos => F::InputMessagesFilterRoundVideo,
            ContentFilter::Polls => F::InputMessagesFilterPoll,
            ContentFilter::Locations => F::InputMessagesFilterGeo,
            ContentFilter::Contacts => F::InputMessagesFilterContacts,
            ContentFilter::Pinned => F::InputMessagesFilterPinned,
            ContentFilter::Mentions => F::InputMessagesFilterMyMentions,
        });
        let parse_date = |value: &str| {
            value
                .parse::<Timestamp>()
                .or_else(|_| {
                    let date: jiff::civil::Date = value.parse()?;
                    Ok::<_, jiff::Error>(date.to_zoned(jiff::tz::TimeZone::UTC)?.timestamp())
                })
                .map_err(|_| format!("invalid date {value:?}; use RFC 3339 or YYYY-MM-DD"))
        };
        let limit = params.limit.unwrap_or(20).min(100);

        let mut messages = Vec::new();
        if let Some(chat) = &params.chat {
            let peer = self.resolve(chat).await?;
            let mut iter = self
                .client
                .search_messages(peer)
                .query(&params.query)
                .limit(limit);
            if let Some(filter) = filter {
                iter = iter.filter(filter);
            }
            if params.from_me {
                iter = iter.sent_by_self();
            }
            if let Some(since) = &params.since {
                iter = iter.min_date(parse_date(since)?);
            }
            if let Some(until) = &params.until {
                iter = iter.max_date(parse_date(until)?);
            }
            if let Some(id) = params.before_id {
                iter = iter.offset_id(id);
            }
            while let Some(message) = iter.next().await.map_err(fail)? {
                messages.push(message_info(&message, self.me));
            }
        } else {
            if params.from_me
                || params.since.is_some()
                || params.until.is_some()
                || params.before_id.is_some()
            {
                return Err("from_me, since, until and before_id require `chat`".to_owned());
            }
            let mut iter = self
                .client
                .search_all_messages()
                .query(&params.query)
                .limit(limit);
            if let Some(filter) = filter {
                iter = iter.filter(filter);
            }
            while let Some(message) = iter.next().await.map_err(fail)? {
                messages.push(message_info(&message, self.me));
            }
        }
        Ok(Json(Messages { messages }))
    }

    #[tool(
        description = "Get details of a user, group or channel: name, kind, username, bio/description and member count.",
        annotations(read_only_hint = true, open_world_hint = true)
    )]
    async fn get_chat(
        &self,
        Parameters(params): Parameters<ChatParams>,
    ) -> Result<Json<ChatDetails>, String> {
        let peer_ref = self.resolve(&params.chat).await?;
        let peer = self.client.resolve_peer(peer_ref).await.map_err(fail)?;
        let (about, members) = if peer_ref.id.kind() == PeerKind::User {
            let tl::enums::users::UserFull::Full(full) = self
                .client
                .invoke(&tl::functions::users::GetFullUser {
                    id: (&peer_ref).into(),
                })
                .await
                .map_err(fail)?;
            let tl::enums::UserFull::Full(user) = full.full_user;
            (user.about, None)
        } else {
            let full = if peer_ref.id.kind() == PeerKind::Chat {
                self.client
                    .invoke(&tl::functions::messages::GetFullChat {
                        chat_id: peer_ref.id.bare_id_unchecked(),
                    })
                    .await
            } else {
                self.client
                    .invoke(&tl::functions::channels::GetFullChannel {
                        channel: (&peer_ref).into(),
                    })
                    .await
            };
            let tl::enums::messages::ChatFull::Full(full) = full.map_err(fail)?;
            match full.full_chat {
                tl::enums::ChatFull::Full(chat) => {
                    let members = match chat.participants {
                        tl::enums::ChatParticipants::Participants(list) => {
                            Some(list.participants.len() as i32)
                        }
                        tl::enums::ChatParticipants::Forbidden(_) => None,
                    };
                    (Some(chat.about), members)
                }
                tl::enums::ChatFull::ChannelFull(channel) => {
                    (Some(channel.about), channel.participants_count)
                }
                tl::enums::ChatFull::CommunityFull(community) => (Some(community.about), None),
            }
        };
        Ok(Json(ChatDetails {
            chat: chat_info(&peer),
            about: about.filter(|about| !about.is_empty()),
            members,
        }))
    }

    #[tool(
        description = "List members of a group or channel with their role. Channels and groups that hide their member list only show admins.",
        annotations(read_only_hint = true, open_world_hint = true)
    )]
    async fn list_participants(
        &self,
        Parameters(params): Parameters<ListParticipantsParams>,
    ) -> Result<Json<Members>, String> {
        let peer = self.resolve(&params.chat).await?;
        let offset = params.page.offset.unwrap_or(0);
        let limit = params.page.limit.unwrap_or(20).min(100);
        let mut participants = self.client.iter_participants(peer);
        let mut members = Vec::with_capacity(limit);
        let mut skipped = 0;
        while members.len() < limit
            && let Some(participant) = participants.next().await.map_err(fail)?
        {
            if skipped < offset {
                skipped += 1;
                continue;
            }
            let role = match participant.role {
                Role::User(_) => MemberRole::Member,
                Role::Creator(_) => MemberRole::Creator,
                Role::Admin(_) => MemberRole::Admin,
                Role::Banned(_) => MemberRole::Banned,
                Role::Left(_) => MemberRole::Left,
                _ => MemberRole::Member,
            };
            members.push(Member {
                user: chat_info(&Peer::User(participant.user)),
                role,
            });
        }
        Ok(Json(Members { members }))
    }

    #[tool(
        description = "Download the photo or file attached to a message into the server's download folder.",
        annotations(read_only_hint = true, open_world_hint = true)
    )]
    async fn download_media(
        &self,
        Parameters(params): Parameters<MessageParams>,
    ) -> Result<Json<Downloaded>, String> {
        let peer = self.resolve(&params.chat).await?;
        let message = self
            .client
            .get_messages_by_id(peer, &[params.message_id])
            .await
            .map_err(fail)?
            .into_iter()
            .flatten()
            .find(|message| !matches!(message.raw, tl::enums::Message::Empty(_)))
            .ok_or(format!("message {} not found", params.message_id))?;
        let name = match message.media() {
            Some(Media::Photo(_)) => "photo.jpg".to_owned(),
            Some(Media::Document(document)) => match document.name() {
                Some(name) if !name.is_empty() => name.replace(['/', '\\'], "_"),
                _ => {
                    let mime = document.mime_type().unwrap_or_default();
                    format!("file.{}", mime.split('/').nth(1).unwrap_or("bin"))
                }
            },
            _ => return Err("this message has no photo or file".to_owned()),
        };
        tokio::fs::create_dir_all(&self.download_dir)
            .await
            .map_err(fail)?;
        // Prefixing chat and message ids keeps names unique and inside the folder.
        let path = self.download_dir.join(format!(
            "{}_{}_{name}",
            message.peer_id().bot_api_dialog_id_unchecked(),
            message.id()
        ));
        message.download_media(&path).await.map_err(fail)?;
        Ok(Json(Downloaded {
            path: path.display().to_string(),
        }))
    }
}
