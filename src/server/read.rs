use grammers_client::media::Media;
use grammers_client::peer::{Peer, Role, User};
use grammers_client::tl;
use grammers_session::types::{PeerId, PeerKind, PeerRef};
use jiff::Timestamp;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    ChatInfo, ChatParams, ChatRef, MessageInfo, MessageParams, Messages, TelegramMcp, chat_info,
    fail, message_info, parse_link, parse_reaction, reaction_text,
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
    /// Only messages newer than this message id, oldest first (e.g. to read on from a linked message).
    after_id: Option<i32>,
    /// Fetch exactly these message ids instead; `limit`, `before_id` and `after_id` are ignored.
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

#[derive(Deserialize, JsonSchema)]
struct GetRepliesParams {
    /// Chat of the post or message: `me`, `@username`, a t.me link, or a numeric id.
    chat: String,
    /// The channel post or group message whose replies to read.
    message_id: i32,
    /// Maximum replies, newest first (default 20, max 100).
    limit: Option<usize>,
    /// Only replies older than this id, for paging.
    before_id: Option<i32>,
    /// Only replies newer than this id, oldest first.
    after_id: Option<i32>,
}

#[derive(Deserialize, JsonSchema)]
struct GetReactionsParams {
    /// `me`, `@username`, a t.me link, or a numeric id from list_chats.
    chat: String,
    message_id: i32,
    /// Only this reaction, as the message's `reactions` show it.
    reaction: Option<String>,
    /// Maximum results, newest first (default 20, max 100).
    limit: Option<usize>,
    /// The previous page's `next_offset`.
    offset: Option<String>,
}

#[derive(Serialize, JsonSchema)]
struct Reactions {
    /// Number of reactions (of `reaction`, if given) on the message.
    total: i32,
    reactions: Vec<PeerReaction>,
    /// Pass as `offset` for the next page.
    #[serde(skip_serializing_if = "Option::is_none")]
    next_offset: Option<String>,
}

#[derive(Serialize, JsonSchema)]
struct PeerReaction {
    /// Who reacted.
    #[serde(flatten)]
    peer: ChatInfo,
    reaction: String,
    /// RFC 3339, UTC.
    date: String,
}

#[derive(Serialize, JsonSchema)]
struct Contacts {
    contacts: Vec<ChatInfo>,
}

#[derive(Serialize, JsonSchema)]
struct Replies {
    /// Number of replies in the whole thread.
    total: i32,
    /// For channel posts these live in the linked discussion group (see each `chat_id`).
    messages: Vec<MessageInfo>,
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
    /// Channel shown on a user's profile.
    #[serde(skip_serializing_if = "Option::is_none")]
    personal_channel: Option<ChatInfo>,
    /// A channel's discussion group, or a discussion group's channel.
    #[serde(skip_serializing_if = "Option::is_none")]
    linked_chat: Option<ChatInfo>,
    /// Pinned messages, newest first (up to 5). In groups and channels these often index
    /// the chat's content.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pinned_messages: Vec<MessageInfo>,
}

#[derive(Deserialize, JsonSchema)]
struct OpenLinkParams {
    /// A t.me / telegram.me / tg:// link to a chat, user, message or invite, or an @username.
    link: String,
}

#[derive(Serialize, JsonSchema)]
struct OpenedLink {
    /// The linked chat or user; for message links, the chat the message is in.
    #[serde(skip_serializing_if = "Option::is_none")]
    chat: Option<ChatDetails>,
    /// The linked message.
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<MessageInfo>,
    /// What an invite link leads to. Nothing is joined.
    #[serde(skip_serializing_if = "Option::is_none")]
    invite: Option<InvitePreview>,
}

#[derive(Serialize, JsonSchema)]
struct InvitePreview {
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    about: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    members: Option<i32>,
    already_member: bool,
    /// Set when the chat is readable now (already a member, or the invite allows a preview).
    #[serde(skip_serializing_if = "Option::is_none")]
    chat_id: Option<i64>,
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
        description = "Get messages from a chat, newest first (or oldest first after a given message), or specific messages by id.",
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
                match (params.before_id, params.after_id) {
                    (Some(_), Some(_)) => {
                        return Err("pass either before_id or after_id, not both".to_owned());
                    }
                    (Some(id), None) => iter = iter.offset_id(id),
                    (None, Some(id)) => iter = iter.offset_id(id).reverse(true),
                    (None, None) => {}
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
        description = "Read the comments under a channel post, or the reply thread of a group message or forum topic. Messages show a `replies` count when there is something to read.",
        annotations(read_only_hint = true, open_world_hint = true)
    )]
    async fn get_replies(
        &self,
        Parameters(params): Parameters<GetRepliesParams>,
    ) -> Result<Json<Replies>, String> {
        let peer = self.resolve(&params.chat).await?;
        let limit = params.limit.unwrap_or(20).min(100) as i32;
        let (offset_id, add_offset, min_id) = match (params.before_id, params.after_id) {
            (Some(_), Some(_)) => {
                return Err("pass either before_id or after_id, not both".to_owned());
            }
            (Some(id), None) => (id, 0, 0),
            // The page of `limit` replies just above `id` (as grammers' reverse iteration does).
            (None, Some(id)) => (id, -limit, id),
            (None, None) => (0, 0, 0),
        };
        let response = self
            .client
            .invoke(&tl::functions::messages::GetReplies {
                peer: (&peer).into(),
                msg_id: params.message_id,
                offset_id,
                offset_date: 0,
                add_offset,
                limit,
                max_id: 0,
                min_id,
                hash: 0,
            })
            .await
            .map_err(fail)?;
        let (raw, chats, users, total) = match response {
            tl::enums::messages::Messages::Messages(page) => {
                let total = page.messages.len() as i32;
                (page.messages, page.chats, page.users, total)
            }
            tl::enums::messages::Messages::Slice(page) => {
                (page.messages, page.chats, page.users, page.count)
            }
            tl::enums::messages::Messages::ChannelMessages(page) => {
                (page.messages, page.chats, page.users, page.count)
            }
            tl::enums::messages::Messages::NotModified(_) => {
                (Vec::new(), Vec::new(), Vec::new(), 0)
            }
        };
        self.cache_peers(&chats, &users).await?;

        // grammers can only build full messages from its own requests, so refetch the replies
        // by id from the chat they live in, whose access hash comes with this response.
        let mut ids = Vec::with_capacity(raw.len());
        let mut thread_chat = None;
        for message in raw {
            if let tl::enums::Message::Message(message) = message {
                thread_chat.get_or_insert(PeerId::from(message.peer_id));
                ids.push(message.id);
            }
        }
        let Some(thread_chat) = thread_chat else {
            return Ok(Json(Replies {
                total,
                messages: Vec::new(),
            }));
        };
        let thread_peer = if thread_chat == peer.id {
            peer
        } else {
            let chat = chats
                .into_iter()
                .map(|chat| Peer::from_raw(&self.client, chat))
                .find(|chat| chat.id() == thread_chat)
                .ok_or("the replies' chat was missing from Telegram's response")?;
            chat.to_ref()
                .await
                .map_err(fail)?
                .ok_or("the replies' chat is not accessible")?
        };
        if params.after_id.is_some() {
            ids.reverse();
        }
        let messages = self
            .client
            .get_messages_by_id(thread_peer, &ids)
            .await
            .map_err(fail)?
            .iter()
            .flatten()
            .filter(|message| !matches!(message.raw, tl::enums::Message::Empty(_)))
            .map(|message| message_info(message, self.me))
            .collect();
        Ok(Json(Replies { total, messages }))
    }

    #[tool(
        description = "List who reacted to a message and with what, newest first. Private chats and groups only: channels show just the counts in each message's `reactions`.",
        annotations(read_only_hint = true, open_world_hint = true)
    )]
    async fn get_reactions(
        &self,
        Parameters(params): Parameters<GetReactionsParams>,
    ) -> Result<Json<Reactions>, String> {
        let peer = self.resolve(&params.chat).await?;
        let tl::enums::messages::MessageReactionsList::List(list) = self
            .client
            .invoke(&tl::functions::messages::GetMessageReactionsList {
                peer: (&peer).into(),
                id: params.message_id,
                reaction: params.reaction.as_deref().map(parse_reaction).transpose()?,
                offset: params.offset,
                limit: params.limit.unwrap_or(20).min(100) as i32,
            })
            .await
            .map_err(|error| {
                if error.is("BROADCAST_FORBIDDEN") {
                    "channels don't show who reacted, only the counts in each message's `reactions`"
                        .to_owned()
                } else {
                    fail(error)
                }
            })?;
        self.cache_peers(&list.chats, &list.users).await?;
        let peers: Vec<Peer> = list
            .chats
            .into_iter()
            .map(|chat| Peer::from_raw(&self.client, chat))
            .chain(
                list.users
                    .into_iter()
                    .map(|user| Peer::User(User::from_raw(&self.client, user))),
            )
            .collect();
        let reactions = list
            .reactions
            .into_iter()
            .filter_map(|tl::enums::MessagePeerReaction::Reaction(reaction)| {
                let id = PeerId::from(reaction.peer_id);
                Some(PeerReaction {
                    peer: chat_info(peers.iter().find(|peer| peer.id() == id)?),
                    reaction: reaction_text(&reaction.reaction),
                    date: Timestamp::from_second(reaction.date.into())
                        .expect("i32 seconds are in range")
                        .to_string(),
                })
            })
            .collect();
        Ok(Json(Reactions {
            total: list.count,
            reactions,
            next_offset: list.next_offset,
        }))
    }

    #[tool(
        description = "List the user's Telegram contacts.",
        annotations(read_only_hint = true, open_world_hint = true)
    )]
    async fn list_contacts(&self) -> Result<Json<Contacts>, String> {
        let users = match self
            .client
            .invoke(&tl::functions::contacts::GetContacts { hash: 0 })
            .await
            .map_err(fail)?
        {
            tl::enums::contacts::Contacts::Contacts(list) => list.users,
            tl::enums::contacts::Contacts::NotModified => Vec::new(),
        };
        self.cache_peers(&[], &users).await?;
        Ok(Json(Contacts {
            contacts: users
                .into_iter()
                .map(|user| chat_info(&Peer::User(User::from_raw(&self.client, user))))
                .collect(),
        }))
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
        description = "Open a group, channel or user: name, kind, username, bio/description, member count, profile channel, linked discussion chat, and pinned messages (often an index of the chat's content).",
        annotations(read_only_hint = true, open_world_hint = true)
    )]
    async fn get_chat(
        &self,
        Parameters(params): Parameters<ChatParams>,
    ) -> Result<Json<ChatDetails>, String> {
        let peer = self.resolve(&params.chat).await?;
        Ok(Json(self.chat_details(peer).await?))
    }

    #[tool(
        description = "Open a Telegram link (t.me/…, telegram.me/…, tg://…) or @username, e.g. one found in a message: returns the linked message, the chat or user (with pinned messages), or what an invite link leads to without joining. Not for web links.",
        annotations(read_only_hint = true, open_world_hint = true)
    )]
    async fn open_link(
        &self,
        Parameters(params): Parameters<OpenLinkParams>,
    ) -> Result<Json<OpenedLink>, String> {
        let link = parse_link(&params.link)?;
        if let ChatRef::Invite(hash) = link.chat {
            let invite = self
                .client
                .invoke(&tl::functions::messages::CheckChatInvite { hash })
                .await
                .map_err(fail)?;
            let already_member = matches!(invite, tl::enums::ChatInvite::Already(_));
            let preview = match invite {
                tl::enums::ChatInvite::Already(tl::types::ChatInviteAlready { chat })
                | tl::enums::ChatInvite::Peek(tl::types::ChatInvitePeek { chat, .. }) => {
                    self.cache_peers(std::slice::from_ref(&chat), &[]).await?;
                    let chat = chat_info(&Peer::from_raw(&self.client, chat));
                    InvitePreview {
                        title: chat.name,
                        about: None,
                        members: None,
                        already_member,
                        chat_id: Some(chat.id),
                    }
                }
                tl::enums::ChatInvite::Invite(invite) => InvitePreview {
                    title: invite.title,
                    about: invite.about,
                    members: Some(invite.participants_count),
                    already_member,
                    chat_id: None,
                },
            };
            return Ok(Json(OpenedLink {
                chat: None,
                message: None,
                invite: Some(preview),
            }));
        }

        let peer = self.resolve_chat(&link.chat).await?;
        let message = match link.message_id {
            Some(id) => Some(
                self.client
                    .get_messages_by_id(peer, &[id])
                    .await
                    .map_err(fail)?
                    .iter()
                    .flatten()
                    .find(|message| !matches!(message.raw, tl::enums::Message::Empty(_)))
                    .map(|message| message_info(message, self.me))
                    .ok_or(format!("message {id} not found"))?,
            ),
            None => None,
        };
        Ok(Json(OpenedLink {
            chat: Some(self.chat_details(peer).await?),
            message,
            invite: None,
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

impl TelegramMcp {
    async fn chat_details(&self, peer_ref: PeerRef) -> Result<ChatDetails, String> {
        let peer = self.client.resolve_peer(peer_ref).await.map_err(fail)?;
        // Full-info responses list the profile channel / linked chat among their `chats`.
        let find_chat = |chats: Vec<tl::enums::Chat>, id: Option<i64>| {
            let id = id?;
            chats
                .into_iter()
                .map(|chat| Peer::from_raw(&self.client, chat))
                .find(|peer| peer.id().bare_id() == Some(id))
                .map(|peer| chat_info(&peer))
        };
        let (about, members, personal_channel, linked_chat) =
            if peer_ref.id.kind() == PeerKind::User {
                let tl::enums::users::UserFull::Full(full) = self
                    .client
                    .invoke(&tl::functions::users::GetFullUser {
                        id: (&peer_ref).into(),
                    })
                    .await
                    .map_err(fail)?;
                self.cache_peers(&full.chats, &full.users).await?;
                let tl::enums::UserFull::Full(user) = full.full_user;
                let personal_channel = find_chat(full.chats, user.personal_channel_id);
                (user.about, None, personal_channel, None)
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
                self.cache_peers(&full.chats, &full.users).await?;
                match full.full_chat {
                    tl::enums::ChatFull::Full(chat) => {
                        let members = match chat.participants {
                            tl::enums::ChatParticipants::Participants(list) => {
                                Some(list.participants.len() as i32)
                            }
                            tl::enums::ChatParticipants::Forbidden(_) => None,
                        };
                        (Some(chat.about), members, None, None)
                    }
                    tl::enums::ChatFull::ChannelFull(channel) => {
                        let linked_chat = find_chat(full.chats, channel.linked_chat_id);
                        (
                            Some(channel.about),
                            channel.participants_count,
                            None,
                            linked_chat,
                        )
                    }
                    tl::enums::ChatFull::CommunityFull(community) => {
                        (Some(community.about), None, None, None)
                    }
                }
            };

        let mut pinned = self
            .client
            .search_messages(peer_ref)
            .filter(tl::enums::MessagesFilter::InputMessagesFilterPinned)
            .limit(5);
        let mut pinned_messages = Vec::new();
        while let Some(message) = pinned.next().await.map_err(fail)? {
            pinned_messages.push(message_info(&message, self.me));
        }

        Ok(ChatDetails {
            chat: chat_info(&peer),
            about: about.filter(|about| !about.is_empty()),
            members,
            personal_channel,
            linked_chat,
            pinned_messages,
        })
    }
}
