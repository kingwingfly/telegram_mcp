use std::time::Duration;

use grammers_client::message::{InputMessage, Message};
use grammers_client::peer::{Peer, User};
use grammers_client::tl;
use grammers_session::types::{PeerKind, PeerRef};
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{serde_json, tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    ButtonAction, ChatInfo, ChatRef, MessageInfo, Messages, TelegramMcp, chat_info, fail,
    message_buttons, message_info, parse_link, parse_reaction,
};

#[derive(Deserialize, JsonSchema)]
struct NamedChatParams {
    /// The chat: `me`, `@username`, a t.me link, or a numeric id from list_chats.
    /// A numeric id is never enough alone: put `chat_name` in the same call.
    chat: String,
    /// The chat's `name`, copied exactly from list_chats or get_chat. Always pass it when
    /// `chat` is a numeric id: the call is refused without it.
    chat_name: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct SendMessageParams {
    /// The chat: `me`, `@username`, a t.me link, or a numeric id from list_chats.
    /// A numeric id is never enough alone: put `chat_name` in the same call.
    chat: String,
    /// The chat's `name`, copied exactly from list_chats or get_chat. Always pass it when
    /// `chat` is a numeric id: the call is refused without it.
    chat_name: Option<String>,
    /// Markdown: **bold**, _italic_, `code`, ```pre```, [link](https://…).
    text: String,
    /// Send as a reply to this message id. Put `reply_to_text` in the same call.
    reply_to: Option<i32>,
    /// Always pass it with `reply_to`: the start of that message's `text` as get_messages
    /// shows it, at least 20 characters or all of it (its `media` if it has no text).
    reply_to_text: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct SendFileParams {
    /// The chat: `me`, `@username`, a t.me link, or a numeric id from list_chats.
    /// A numeric id is never enough alone: put `chat_name` in the same call.
    chat: String,
    /// The chat's `name`, copied exactly from list_chats or get_chat. Always pass it when
    /// `chat` is a numeric id: the call is refused without it.
    chat_name: Option<String>,
    /// Path of the file to upload, absolute or relative to the server's upload folder. Only
    /// files in that folder can be sent: any other path is refused, and the refusal names that
    /// folder. When download_media saves there, the `path` it returned can be passed as it is.
    /// .jpg/.png/.webp are sent as photos, anything else as a file.
    path: String,
    /// Markdown caption.
    caption: Option<String>,
    /// Send as a reply to this message id. Put `reply_to_text` in the same call.
    reply_to: Option<i32>,
    /// Always pass it with `reply_to`: the start of that message's `text` as get_messages
    /// shows it, at least 20 characters or all of it (its `media` if it has no text).
    reply_to_text: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct EditMessageParams {
    /// The chat: `me`, `@username`, a t.me link, or a numeric id from list_chats.
    /// A numeric id is never enough alone: put `chat_name` in the same call.
    chat: String,
    /// The chat's `name`, copied exactly from list_chats or get_chat. Always pass it when
    /// `chat` is a numeric id: the call is refused without it.
    chat_name: Option<String>,
    message_id: i32,
    /// The start of that message's `text` as get_messages shows it, at least 20 characters or
    /// all of it (its `media` if it has no text).
    message_text: String,
    /// New Markdown text.
    text: String,
}

#[derive(Deserialize, JsonSchema)]
struct MessageRef {
    id: i32,
    /// The start of the message's `text` as get_messages shows it, at least 20 characters or
    /// all of it (its `media` if it has no text).
    text: String,
}

#[derive(Deserialize, JsonSchema)]
struct MessagesParams {
    /// The chat: `me`, `@username`, a t.me link, or a numeric id from list_chats.
    /// A numeric id is never enough alone: put `chat_name` in the same call.
    chat: String,
    /// The chat's `name`, copied exactly from list_chats or get_chat. Always pass it when
    /// `chat` is a numeric id: the call is refused without it.
    chat_name: Option<String>,
    messages: Vec<MessageRef>,
}

#[derive(Deserialize, JsonSchema)]
struct ForwardMessagesParams {
    /// Chat the messages are in: `me`, `@username`, a t.me link, or a numeric id from
    /// list_chats. A numeric id is never enough alone: put `from_chat_name` in the same call.
    from_chat: String,
    /// That chat's `name`, copied exactly from list_chats or get_chat. Always pass it when
    /// `from_chat` is a numeric id: the call is refused without it.
    from_chat_name: Option<String>,
    messages: Vec<MessageRef>,
    /// Chat to forward them to, given like `from_chat`. A numeric id is never enough alone:
    /// put `to_chat_name` in the same call.
    to_chat: String,
    /// That chat's `name`, copied exactly from list_chats or get_chat. Always pass it when
    /// `to_chat` is a numeric id: the call is refused without it.
    to_chat_name: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct SendReactionParams {
    /// The chat: `me`, `@username`, a t.me link, or a numeric id from list_chats.
    /// A numeric id is never enough alone: put `chat_name` in the same call.
    chat: String,
    /// The chat's `name`, copied exactly from list_chats or get_chat. Always pass it when
    /// `chat` is a numeric id: the call is refused without it.
    chat_name: Option<String>,
    message_id: i32,
    /// The start of that message's `text` as get_messages shows it, at least 20 characters or
    /// all of it (its `media` if it has no text).
    message_text: String,
    /// The user's reactions after the call, replacing theirs: emoji such as 👍, or `custom:<id>`
    /// as a message's `reactions` show them. Empty removes them. Each must be allowed in the
    /// chat; more than one, and custom emoji, need Telegram Premium.
    reactions: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
struct NamedMessageParams {
    /// The chat: `me`, `@username`, a t.me link, or a numeric id from list_chats.
    /// A numeric id is never enough alone: put `chat_name` in the same call.
    chat: String,
    /// The chat's `name`, copied exactly from list_chats or get_chat. Always pass it when
    /// `chat` is a numeric id: the call is refused without it.
    chat_name: Option<String>,
    message_id: i32,
    /// The start of that message's `text` as get_messages shows it, at least 20 characters or
    /// all of it (its `media` if it has no text).
    message_text: String,
}

#[derive(Deserialize, JsonSchema)]
struct ClickButtonParams {
    /// The chat with the message: `me`, `@username`, a t.me link, or a numeric id from
    /// list_chats. A numeric id is never enough alone: put `chat_name` in the same call.
    chat: String,
    /// The chat's `name`, copied exactly from list_chats or get_chat. Always pass it when
    /// `chat` is a numeric id: the call is refused without it.
    chat_name: Option<String>,
    message_id: i32,
    /// The start of that message's `text` as get_messages shows it, at least 20 characters or
    /// all of it (its `media` if it has no text).
    message_text: String,
    /// The button's text: an exact match, else a unique partial match.
    text: String,
    /// The button's `index` from the message's `buttons`, to pick one of several buttons with
    /// the same text.
    index: Option<usize>,
}

#[derive(Serialize, JsonSchema, Default)]
struct ClickResult {
    /// The bot's answer to the click (popup or alert text), or the text a copy button copies.
    #[serde(skip_serializing_if = "Option::is_none")]
    answer: Option<String>,
    /// A link to open: the button's own, or one the bot answered with. Nothing was opened.
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    /// The clicked message after the click; bots often edit it in place (next page, filters).
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<MessageInfo>,
    /// Messages the chat received after the click, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    new_messages: Vec<MessageInfo>,
}

#[derive(Deserialize, JsonSchema)]
struct AddContactParams {
    /// The person to add: `@username`, a t.me link, or a numeric id. Omit when adding by
    /// `phone`. A numeric id is never enough alone: put `user_name` in the same call.
    user: Option<String>,
    /// That person's `name`, copied exactly from list_chats, list_contacts or get_chat. Always
    /// pass it when `user` is a numeric id: the call is refused without it.
    user_name: Option<String>,
    /// Phone number in international format, to add someone by phone instead of `user`.
    phone: Option<String>,
    /// Name to save the contact under; defaults to their profile name. Required with `phone`.
    first_name: Option<String>,
    last_name: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct UserParams {
    /// The person: `@username`, a t.me link, or a numeric id. A numeric id is never enough
    /// alone: put `user_name` in the same call.
    user: String,
    /// That person's `name`, copied exactly from list_chats, list_contacts or get_chat. Always
    /// pass it when `user` is a numeric id: the call is refused without it.
    user_name: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct JoinChatParams {
    /// `@username` or t.me link of a public group/channel, or an invite link (t.me/+… or
    /// t.me/joinchat/…). An invite link is never enough alone, and neither is a numeric id:
    /// put `chat_name` in the same call.
    chat: String,
    /// The chat's `name`, copied exactly from get_chat, or for an invite link the `title`
    /// open_link shows. Always pass it with an invite link or a numeric id: the call is
    /// refused without it.
    chat_name: Option<String>,
}

/// The refusal of a call whose name for a chat or user is missing, or is not `actual`, the
/// name it really has. It gives that name and spells out the whole next call: told only
/// which argument is missing, a weaker model sends that argument and nothing else.
fn unnamed(what: &str, actual: &str, given: Option<&str>, name_field: &str) -> String {
    let actual = serde_json::Value::from(actual);
    let found = match given {
        Some(given) => format!("not {}", serde_json::Value::from(given)),
        None => "and it has to come with that name".to_owned(),
    };
    format!(
        "Nothing was done: {what} is {actual}, {found}. If {actual} is the one you mean, make \
         the same call again with every argument it had, and with \"{name_field}\": {actual}. \
         A call with only `{name_field}` in it fails."
    )
}

impl TelegramMcp {
    /// `resolve` for tools that act as the user. Their permission prompts show only the
    /// arguments, so a numeric id needs the chat's name beside it, and a name passed must match.
    async fn resolve_named(
        &self,
        chat: &str,
        name: Option<&str>,
        name_field: &str,
    ) -> Result<PeerRef, String> {
        let link = parse_link(chat)?;
        let peer = self.resolve_chat(&link.chat).await?;
        let numeric = matches!(link.chat, ChatRef::Peer(_));
        if name.is_none() && !numeric {
            return Ok(peer);
        }
        let actual = chat_info(&self.client.resolve_peer(peer).await.map_err(fail)?).name;
        if name == Some(actual.as_str()) {
            return Ok(peer);
        }
        let what = if numeric {
            format!("the numeric id {chat}")
        } else {
            chat.to_owned()
        };
        Err(unnamed(&what, &actual, name, name_field))
    }

    /// `resolve_named` for the send group, which `--send-to` limits to the chats it matches.
    /// Usernames, not names, are matched: anyone can take any name, but a username is owned.
    async fn resolve_send(
        &self,
        chat: &str,
        name: Option<&str>,
        name_field: &str,
    ) -> Result<PeerRef, String> {
        let peer = self.resolve_named(chat, name, name_field).await?;
        let Some(patterns) = &self.send_to else {
            return Ok(peer);
        };
        let options = glob::MatchOptions {
            case_sensitive: false,
            ..Default::default()
        };
        let matches = |name: &str| {
            patterns
                .iter()
                .any(|pattern| pattern.matches_with(name, options))
        };
        if matches(&peer.id.bot_api_dialog_id_unchecked().to_string())
            || (peer.id == self.me.id && matches("me"))
        {
            return Ok(peer);
        }
        let resolved = self.client.resolve_peer(peer).await.map_err(fail)?;
        if resolved
            .username()
            .into_iter()
            .chain(resolved.usernames())
            // Telegram gives usernames without the `@`, which patterns may have or not.
            .any(|username| matches(username) || matches(&format!("@{username}")))
        {
            return Ok(peer);
        }
        let allowed: Vec<&str> = patterns.iter().map(glob::Pattern::as_str).collect();
        Err(format!(
            "Nothing was done: this server only acts in chats matching {} (a chat's @username, \
             its numeric id, or `me`), and {chat} is not one of them.",
            allowed.join(", ")
        ))
    }

    /// Fetches the messages and checks each against the text passed for it, so the user
    /// confirming the call sees what the messages say, not just their ids. `text_field` is
    /// how the argument holding that text is called in the tool, for the refusal.
    async fn check_messages(
        &self,
        peer: PeerRef,
        messages: &[(i32, &str)],
        text_field: &str,
    ) -> Result<Vec<Message>, String> {
        let ids: Vec<i32> = messages.iter().map(|&(id, _)| id).collect();
        let found = self
            .client
            .get_messages_by_id(peer, &ids)
            .await
            .map_err(fail)?;
        let mut checked = Vec::with_capacity(found.len());
        for (&(id, text), message) in messages.iter().zip(found) {
            let message = message
                .filter(|message| !matches!(message.raw, tl::enums::Message::Empty(_)))
                .ok_or(format!("message {id} not found"))?;
            let info = message_info(&message, self.me);
            let actual = if info.text.is_empty() {
                info.media.unwrap_or_default()
            } else {
                info.text
            };
            let needed = actual.chars().count().min(20);
            if !actual.starts_with(text) || text.chars().count() < needed {
                let start = serde_json::Value::from(actual.chars().take(40).collect::<String>());
                return Err(format!(
                    "Nothing was done: message {id} starts {start}, and {text_field} has to be at \
                     least the first {needed} characters of that, copied exactly. Make the same \
                     call again with every argument it had, and with that one put right. A call \
                     with only that argument in it fails."
                ));
            }
            checked.push(message);
        }
        Ok(checked)
    }
}

/// Everything else that acts as the user: `serve --access send`.
#[tool_router(router = send_tools, vis = "pub(super)")]
impl TelegramMcp {
    #[tool(
        description = "Send a text message as the user. If `chat` is a numeric id, `chat_name` must be in the same call. Example: {\"chat\": \"-1001234567890\", \"chat_name\": \"Family\", \"text\": \"On my way\"}",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = true
        )
    )]
    async fn send_message(
        &self,
        Parameters(params): Parameters<SendMessageParams>,
    ) -> Result<Json<MessageInfo>, String> {
        let peer = self
            .resolve_send(&params.chat, params.chat_name.as_deref(), "chat_name")
            .await?;
        if let Some(id) = params.reply_to {
            // Left out, the text is refused like a wrong one, which says what it has to be.
            let text = params.reply_to_text.as_deref().unwrap_or_default();
            self.check_messages(peer, &[(id, text)], "`reply_to_text`")
                .await?;
        }
        let message = InputMessage::new()
            .markdown(&params.text)
            .reply_to(params.reply_to);
        let sent = self
            .client
            .send_message(peer, message)
            .await
            .map_err(fail)?;
        Ok(Json(message_info(&sent, self.me)))
    }

    #[tool(
        description = "Upload a file from the server's upload folder and send it to a chat as the user, with an optional caption. Only files in that folder can be sent; any other `path` is refused, and the refusal names the folder. If `chat` is a numeric id, `chat_name` must be in the same call. Example, with a `path` as download_media returns it: {\"chat\": \"-1001234567890\", \"chat_name\": \"Family\", \"path\": \"/home/user/Downloads/telegram-mcp/-1001234567890_45_menu.pdf\"}",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = true
        )
    )]
    async fn send_file(
        &self,
        Parameters(params): Parameters<SendFileParams>,
    ) -> Result<Json<MessageInfo>, String> {
        // Checked before anything talks to Telegram, so a refused call costs nothing. The route
        // is removed without `--upload-dir`; this refusal is in case that ever breaks.
        let Some(dir) = &self.upload_dir else {
            return Err("send_file is off: the server was started without --upload-dir".to_owned());
        };
        // Symlinks and `..` are resolved first, so the file checked is the file sent. A relative
        // path starts in the upload folder, not in the server's working directory.
        let checked = async {
            let path = tokio::fs::canonicalize(dir.join(&params.path)).await.ok()?;
            if !path.starts_with(dir) || !tokio::fs::metadata(&path).await.ok()?.is_file() {
                return None;
            }
            let file = tokio::fs::File::open(&path).await.ok()?;
            Some((path, file))
        }
        .await;
        let Some((path, mut file)) = checked else {
            // One refusal for every failure (missing, outside the folder, not a file,
            // unreadable), so it tells nothing about paths outside the folder.
            let downloads = tokio::fs::canonicalize(&self.download_dir)
                .await
                .unwrap_or_else(|_| self.download_dir.clone());
            let hint = if downloads.starts_with(dir) {
                " download_media saves there: pass the `path` it returned as it is."
            } else {
                ""
            };
            return Err(format!(
                "Nothing was sent: send_file only sends files in {}.{hint} Make the same call \
                 again with every argument it had, and with a `path` in that folder.",
                dir.display()
            ));
        };
        let peer = self
            .resolve_send(&params.chat, params.chat_name.as_deref(), "chat_name")
            .await?;
        if let Some(id) = params.reply_to {
            // Left out, the text is refused like a wrong one, which says what it has to be.
            let text = params.reply_to_text.as_deref().unwrap_or_default();
            self.check_messages(peer, &[(id, text)], "`reply_to_text`")
                .await?;
        }
        let size = file.metadata().await.map_err(fail)?.len();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let uploaded = self
            .client
            .upload_stream(&mut file, size as usize, name.into_owned())
            .await
            .map_err(fail)?;
        let message = InputMessage::new()
            .markdown(params.caption.unwrap_or_default())
            .reply_to(params.reply_to);
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let message = match extension.as_str() {
            "jpg" | "jpeg" | "png" | "webp" => message.photo(uploaded),
            _ => message.document(uploaded),
        };
        let sent = self
            .client
            .send_message(peer, message)
            .await
            .map_err(fail)?;
        Ok(Json(message_info(&sent, self.me)))
    }

    #[tool(
        description = "Edit the text of one of the user's own messages. If `chat` is a numeric id, `chat_name` must be in the same call. Example: {\"chat\": \"-1001234567890\", \"chat_name\": \"Family\", \"message_id\": 45, \"message_text\": \"On my way, there in ten\", \"text\": \"There in five\"}",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = true
        )
    )]
    async fn edit_message(
        &self,
        Parameters(params): Parameters<EditMessageParams>,
    ) -> Result<String, String> {
        let peer = self
            .resolve_send(&params.chat, params.chat_name.as_deref(), "chat_name")
            .await?;
        self.check_messages(
            peer,
            &[(params.message_id, &params.message_text)],
            "`message_text`",
        )
        .await?;
        self.client
            .edit_message(
                peer,
                params.message_id,
                InputMessage::new().markdown(&params.text),
            )
            .await
            .map_err(fail)?;
        Ok(format!("edited message {}", params.message_id))
    }

    #[tool(
        description = "Delete messages for everyone where possible. Irreversible. If `chat` is a numeric id, `chat_name` must be in the same call. Example: {\"chat\": \"-1001234567890\", \"chat_name\": \"Family\", \"messages\": [{\"id\": 45, \"text\": \"On my way, there in ten\"}]}",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = true
        )
    )]
    async fn delete_messages(
        &self,
        Parameters(params): Parameters<MessagesParams>,
    ) -> Result<String, String> {
        let peer = self
            .resolve_send(&params.chat, params.chat_name.as_deref(), "chat_name")
            .await?;
        let messages: Vec<(i32, &str)> = params
            .messages
            .iter()
            .map(|message| (message.id, message.text.as_str()))
            .collect();
        self.check_messages(peer, &messages, "the `text` beside its `id` in `messages`")
            .await?;
        let ids: Vec<i32> = messages.iter().map(|&(id, _)| id).collect();
        let deleted = self
            .client
            .delete_messages(peer, &ids)
            .await
            .map_err(fail)?;
        Ok(format!("deleted {deleted} messages"))
    }

    #[tool(
        description = "Forward messages from one chat to another as the user. Each chat given as a numeric id needs its name in the same call: `from_chat_name` with `from_chat`, `to_chat_name` with `to_chat`. Example: {\"from_chat\": \"-1001234567890\", \"from_chat_name\": \"Family\", \"messages\": [{\"id\": 45, \"text\": \"On my way, there in ten\"}], \"to_chat\": \"123456789\", \"to_chat_name\": \"Bea Miller\"}",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = true
        )
    )]
    async fn forward_messages(
        &self,
        Parameters(params): Parameters<ForwardMessagesParams>,
    ) -> Result<Json<Messages>, String> {
        let source = self
            .resolve_named(
                &params.from_chat,
                params.from_chat_name.as_deref(),
                "from_chat_name",
            )
            .await?;
        let destination = self
            .resolve_send(
                &params.to_chat,
                params.to_chat_name.as_deref(),
                "to_chat_name",
            )
            .await?;
        let messages: Vec<(i32, &str)> = params
            .messages
            .iter()
            .map(|message| (message.id, message.text.as_str()))
            .collect();
        self.check_messages(
            source,
            &messages,
            "the `text` beside its `id` in `messages`",
        )
        .await?;
        let ids: Vec<i32> = messages.iter().map(|&(id, _)| id).collect();
        let forwarded = self
            .client
            .forward_messages(destination, &ids, source)
            .await
            .map_err(fail)?;
        Ok(Json(Messages {
            messages: forwarded
                .iter()
                .flatten()
                .map(|message| message_info(message, self.me))
                .collect(),
        }))
    }

    #[tool(
        description = "Set the user's reactions to a message, replacing their previous ones; an empty list removes them. If `chat` is a numeric id, `chat_name` must be in the same call. Example: {\"chat\": \"-1001234567890\", \"chat_name\": \"Family\", \"message_id\": 45, \"message_text\": \"On my way, there in ten\", \"reactions\": [\"👍\"]}",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn send_reaction(
        &self,
        Parameters(params): Parameters<SendReactionParams>,
    ) -> Result<String, String> {
        let reactions = params
            .reactions
            .iter()
            .map(|reaction| match parse_reaction(reaction)? {
                tl::enums::Reaction::Paid => {
                    Err("paid reactions spend Telegram Stars; not supported".to_owned())
                }
                reaction => Ok(reaction),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let peer = self
            .resolve_send(&params.chat, params.chat_name.as_deref(), "chat_name")
            .await?;
        self.check_messages(
            peer,
            &[(params.message_id, &params.message_text)],
            "`message_text`",
        )
        .await?;
        self.client
            .send_reactions(peer, params.message_id, reactions)
            .await
            .map_err(fail)?;
        Ok(if params.reactions.is_empty() {
            format!("removed your reactions from message {}", params.message_id)
        } else {
            format!(
                "reacted {} to message {}",
                params.reactions.join(" "),
                params.message_id
            )
        })
    }

    #[tool(
        description = "Pin a message in a chat. If `chat` is a numeric id, `chat_name` must be in the same call. Example: {\"chat\": \"-1001234567890\", \"chat_name\": \"Family\", \"message_id\": 45, \"message_text\": \"On my way, there in ten\"}",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = true
        )
    )]
    async fn pin_message(
        &self,
        Parameters(params): Parameters<NamedMessageParams>,
    ) -> Result<String, String> {
        let peer = self
            .resolve_send(&params.chat, params.chat_name.as_deref(), "chat_name")
            .await?;
        self.check_messages(
            peer,
            &[(params.message_id, &params.message_text)],
            "`message_text`",
        )
        .await?;
        self.client
            .pin_message(peer, params.message_id)
            .await
            .map_err(fail)?;
        Ok(format!("pinned message {}", params.message_id))
    }

    #[tool(
        description = "Unpin a message in a chat. If `chat` is a numeric id, `chat_name` must be in the same call. Example: {\"chat\": \"-1001234567890\", \"chat_name\": \"Family\", \"message_id\": 45, \"message_text\": \"On my way, there in ten\"}",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = true
        )
    )]
    async fn unpin_message(
        &self,
        Parameters(params): Parameters<NamedMessageParams>,
    ) -> Result<String, String> {
        let peer = self
            .resolve_send(&params.chat, params.chat_name.as_deref(), "chat_name")
            .await?;
        self.check_messages(
            peer,
            &[(params.message_id, &params.message_text)],
            "`message_text`",
        )
        .await?;
        self.client
            .unpin_message(peer, params.message_id)
            .await
            .map_err(fail)?;
        Ok(format!("unpinned message {}", params.message_id))
    }

    #[tool(
        description = "Click a button under a message as the user, e.g. a bot's next page or filter: returns the bot's answer, the message after the click, and messages the click produced. Link and copy buttons are only reported, not opened. If `chat` is a numeric id, `chat_name` must be in the same call. Example: {\"chat\": \"123456789\", \"chat_name\": \"Shop Bot\", \"message_id\": 45, \"message_text\": \"Choose a category\", \"text\": \"Next page\"}",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = true
        )
    )]
    async fn click_button(
        &self,
        Parameters(params): Parameters<ClickButtonParams>,
    ) -> Result<Json<ClickResult>, String> {
        let peer = self
            .resolve_send(&params.chat, params.chat_name.as_deref(), "chat_name")
            .await?;
        let not_empty = |message: &&Message| !matches!(message.raw, tl::enums::Message::Empty(_));
        let message = self
            .check_messages(
                peer,
                &[(params.message_id, &params.message_text)],
                "`message_text`",
            )
            .await?
            .remove(0);

        let buttons = message_buttons(&message);
        if buttons.is_empty() {
            return Err(format!("message {} has no buttons", params.message_id));
        }
        let text = params.text.trim();
        let position = match params.index {
            // `index` only picks between buttons with the same text; the text is what the user sees.
            Some(index) => buttons
                .get(index)
                .filter(|(button, _)| button.text.trim() == text)
                .map(|_| index),
            None => buttons
                .iter()
                .position(|(button, _)| button.text.trim() == text)
                .or_else(|| {
                    let text = text.to_lowercase();
                    let mut partial = buttons
                        .iter()
                        .enumerate()
                        .filter(|(_, (button, _))| button.text.to_lowercase().contains(&text))
                        .map(|(position, _)| position);
                    partial.next().filter(|_| partial.next().is_none())
                }),
        };
        let Some(position) = position else {
            let available: Vec<String> = buttons
                .iter()
                .map(|(button, _)| format!("[{}] {}", button.index, button.text))
                .collect();
            return Err(format!(
                "no such button; available: {}",
                available.join(", ")
            ));
        };
        let (button, action) = buttons
            .into_iter()
            .nth(position)
            .expect("position is in range");

        let callback_data = match action {
            ButtonAction::Link(url) => {
                return Ok(Json(ClickResult {
                    url: Some(url),
                    ..Default::default()
                }));
            }
            ButtonAction::Copy(text) => {
                return Ok(Json(ClickResult {
                    answer: Some(text),
                    ..Default::default()
                }));
            }
            ButtonAction::Unsupported(reason) => return Err(reason.to_owned()),
            ButtonAction::Callback {
                requires_password: true,
                ..
            } => {
                return Err(
                    "this button asks for the account's 2FA password; not supported".to_owned(),
                );
            }
            ButtonAction::Callback { data, .. } => Some(data),
            ButtonAction::Keyboard => None,
        };

        // The newest message before the click tells which messages the click produced.
        let mut latest = self.client.iter_messages(peer).limit(1);
        let last_id = latest.next().await.map_err(fail)?.map_or(0, |m| m.id());

        let mut result = ClickResult::default();
        match callback_data {
            Some(data) => {
                let answer = self
                    .client
                    .invoke(&tl::functions::messages::GetBotCallbackAnswer {
                        game: false,
                        peer: (&peer).into(),
                        msg_id: message.id(),
                        data: Some(data),
                        password: None,
                    })
                    .await;
                match answer {
                    Ok(tl::enums::messages::BotCallbackAnswer::Answer(answer)) => {
                        result.answer = answer.message;
                        result.url = answer.url;
                    }
                    // The bot may still act on the click after failing to answer in time.
                    Err(error) if error.is("BOT_RESPONSE_TIMEOUT") => {
                        result.answer =
                            Some("(the bot did not answer the click in time)".to_owned());
                    }
                    Err(error) => return Err(fail(error)),
                }
            }
            None => {
                self.client
                    .send_message(peer, InputMessage::new().text(&button.text))
                    .await
                    .map_err(fail)?;
            }
        }

        // Bots usually edit the message or reply right after answering.
        tokio::time::sleep(Duration::from_secs(1)).await;
        result.message = self
            .client
            .get_messages_by_id(peer, &[message.id()])
            .await
            .map_err(fail)?
            .iter()
            .flatten()
            .find(not_empty)
            .map(|message| message_info(message, self.me));
        let mut newer = self
            .client
            .iter_messages(peer)
            .reverse(true)
            .offset_id(last_id)
            .limit(10);
        while let Some(message) = newer.next().await.map_err(fail)? {
            result.new_messages.push(message_info(&message, self.me));
        }
        Ok(Json(result))
    }

    #[tool(
        description = "Mark all messages in a chat as read. If `chat` is a numeric id, `chat_name` must be in the same call. Example: {\"chat\": \"-1001234567890\", \"chat_name\": \"Family\"}",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn mark_as_read(
        &self,
        Parameters(params): Parameters<NamedChatParams>,
    ) -> Result<String, String> {
        let peer = self
            .resolve_send(&params.chat, params.chat_name.as_deref(), "chat_name")
            .await?;
        self.client.mark_as_read(peer).await.map_err(fail)?;
        Ok(format!("marked {} as read", params.chat))
    }
}

/// Joining/leaving chats and managing contacts: `serve --access contacts`.
#[tool_router(router = contacts_tools, vis = "pub(super)")]
impl TelegramMcp {
    #[tool(
        description = "Join a group or channel as the user: @username, t.me link, or invite link. Public chats can be read without joining. An invite link or a numeric id must come with `chat_name` in the same call; for an invite link that is the `title` open_link shows. Example: {\"chat\": \"https://t.me/+AbCdEfGhIjKlMnOp\", \"chat_name\": \"Book Club\"}",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = true
        )
    )]
    async fn join_chat(
        &self,
        Parameters(params): Parameters<JoinChatParams>,
    ) -> Result<String, String> {
        let joined = match parse_link(&params.chat)?.chat {
            ChatRef::Invite(hash) => {
                let invite = self
                    .client
                    .invoke(&tl::functions::messages::CheckChatInvite { hash })
                    .await
                    .map_err(fail)?;
                let title = match invite {
                    tl::enums::ChatInvite::Already(tl::types::ChatInviteAlready { chat })
                    | tl::enums::ChatInvite::Peek(tl::types::ChatInvitePeek { chat, .. }) => {
                        chat_info(&Peer::from_raw(&self.client, chat)).name
                    }
                    tl::enums::ChatInvite::Invite(invite) => invite.title,
                };
                let name = params.chat_name.as_deref();
                if name != Some(title.as_str()) {
                    let what = format!("the invite link {}", params.chat);
                    return Err(unnamed(&what, &title, name, "chat_name"));
                }
                self.client.accept_invite_link(&params.chat).await
            }
            _ => {
                let peer = self
                    .resolve_named(&params.chat, params.chat_name.as_deref(), "chat_name")
                    .await?;
                self.client.join_chat(peer).await
            }
        };
        Ok(match joined.map_err(fail)? {
            Some(peer) => {
                let chat = chat_info(&peer);
                format!("joined {} (id {})", chat.name, chat.id)
            }
            None => format!("joined {}", params.chat),
        })
    }

    #[tool(
        description = "Leave a group or channel as the user. If `chat` is a numeric id, `chat_name` must be in the same call. Example: {\"chat\": \"-1001234567890\", \"chat_name\": \"Family\"}",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = true
        )
    )]
    async fn leave_chat(
        &self,
        Parameters(params): Parameters<NamedChatParams>,
    ) -> Result<String, String> {
        let peer = self
            .resolve_named(&params.chat, params.chat_name.as_deref(), "chat_name")
            .await?;
        // For private chats grammers' delete_dialog deletes the history instead.
        if peer.id.kind() == PeerKind::User {
            return Err("leave_chat only works for groups and channels".to_owned());
        }
        self.client.delete_dialog(peer).await.map_err(fail)?;
        Ok(format!(
            "left {}",
            params.chat_name.as_deref().unwrap_or(&params.chat)
        ))
    }

    #[tool(
        description = "Add someone to the user's Telegram contacts, by @username/id/link or by phone number. The user's own phone number is not shared. If `user` is a numeric id, `user_name` must be in the same call. Example: {\"user\": \"123456789\", \"user_name\": \"Bea Miller\"}",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = true
        )
    )]
    async fn add_contact(
        &self,
        Parameters(params): Parameters<AddContactParams>,
    ) -> Result<Json<ChatInfo>, String> {
        let contact = match (params.user, params.phone) {
            (Some(user), None) => {
                let peer = self
                    .resolve_named(&user, params.user_name.as_deref(), "user_name")
                    .await?;
                let Peer::User(found) = self.client.resolve_peer(peer).await.map_err(fail)? else {
                    return Err(format!("{user} is not a user"));
                };
                // Without names this would overwrite the name an existing contact is saved under.
                if found.contact() && params.first_name.is_none() && params.last_name.is_none() {
                    return Ok(Json(chat_info(&Peer::User(found))));
                }
                let first_name = params
                    .first_name
                    .unwrap_or_else(|| found.first_name().unwrap_or_default().to_owned());
                let last_name = params
                    .last_name
                    .unwrap_or_else(|| found.last_name().unwrap_or_default().to_owned());
                self.client
                    .invoke(&tl::functions::contacts::AddContact {
                        add_phone_privacy_exception: false,
                        id: (&peer).into(),
                        first_name,
                        last_name,
                        phone: String::new(),
                        note: None,
                    })
                    .await
                    .map_err(fail)?;
                Peer::User(found)
            }
            (None, Some(phone)) => {
                let first_name = params
                    .first_name
                    .ok_or("first_name is required when adding by phone")?;
                let tl::enums::contacts::ImportedContacts::Contacts(imported) = self
                    .client
                    .invoke(&tl::functions::contacts::ImportContacts {
                        contacts: vec![
                            tl::types::InputPhoneContact {
                                client_id: 0,
                                phone: phone
                                    .chars()
                                    .filter(|c| c.is_ascii_digit() || *c == '+')
                                    .collect(),
                                first_name,
                                last_name: params.last_name.unwrap_or_default(),
                                note: None,
                            }
                            .into(),
                        ],
                    })
                    .await
                    .map_err(fail)?;
                self.cache_peers(&[], &imported.users).await?;
                let user = imported.users.into_iter().next().ok_or(
                    "no Telegram account found for this number (it may not exist, or hide itself from phone search)",
                )?;
                Peer::User(User::from_raw(&self.client, user))
            }
            _ => return Err("pass exactly one of `user` or `phone`".to_owned()),
        };
        Ok(Json(chat_info(&contact)))
    }

    #[tool(
        description = "Remove someone from the user's Telegram contacts. If `user` is a numeric id, `user_name` must be in the same call. Example: {\"user\": \"123456789\", \"user_name\": \"Bea Miller\"}",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn delete_contact(
        &self,
        Parameters(params): Parameters<UserParams>,
    ) -> Result<String, String> {
        let peer = self
            .resolve_named(&params.user, params.user_name.as_deref(), "user_name")
            .await?;
        if peer.id.kind() != PeerKind::User {
            return Err(format!("{} is not a user", params.user));
        }
        self.client
            .invoke(&tl::functions::contacts::DeleteContacts {
                id: vec![(&peer).into()],
            })
            .await
            .map_err(fail)?;
        Ok(format!("removed {} from contacts", params.user))
    }
}
