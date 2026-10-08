use std::path::Path;
use std::time::Duration;

use grammers_client::message::{InputMessage, Message};
use grammers_client::peer::{Peer, User};
use grammers_client::tl;
use grammers_session::types::{PeerKind, PeerRef};
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    ButtonAction, ChatInfo, ChatRef, MessageInfo, Messages, TelegramMcp, chat_info, fail,
    message_buttons, message_info, parse_link,
};

#[derive(Deserialize, JsonSchema)]
struct NamedChatParams {
    /// `me`, `@username`, a t.me link, or a numeric id from list_chats.
    chat: String,
    /// The chat's name as list_chats or get_chat show it. Required when `chat` is a numeric id.
    chat_name: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct SendMessageParams {
    /// `me`, `@username`, a t.me link, or a numeric id from list_chats.
    chat: String,
    /// The chat's name as list_chats or get_chat show it. Required when `chat` is a numeric id.
    chat_name: Option<String>,
    /// Markdown: **bold**, _italic_, `code`, ```pre```, [link](https://…).
    text: String,
    /// Send as a reply to this message id.
    reply_to: Option<i32>,
    /// Required with `reply_to`: the start of that message's `text` as get_messages shows it,
    /// at least 20 characters or all of it (its `media` if it has no text).
    reply_to_text: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct SendFileParams {
    /// `me`, `@username`, a t.me link, or a numeric id from list_chats.
    chat: String,
    /// The chat's name as list_chats or get_chat show it. Required when `chat` is a numeric id.
    chat_name: Option<String>,
    /// Path of the local file to upload. .jpg/.png/.webp are sent as photos, anything else as a file.
    path: String,
    /// Markdown caption.
    caption: Option<String>,
    /// Send as a reply to this message id.
    reply_to: Option<i32>,
    /// Required with `reply_to`: the start of that message's `text` as get_messages shows it,
    /// at least 20 characters or all of it (its `media` if it has no text).
    reply_to_text: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct EditMessageParams {
    /// `me`, `@username`, a t.me link, or a numeric id from list_chats.
    chat: String,
    /// The chat's name as list_chats or get_chat show it. Required when `chat` is a numeric id.
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
    /// `me`, `@username`, a t.me link, or a numeric id from list_chats.
    chat: String,
    /// The chat's name as list_chats or get_chat show it. Required when `chat` is a numeric id.
    chat_name: Option<String>,
    messages: Vec<MessageRef>,
}

#[derive(Deserialize, JsonSchema)]
struct ForwardMessagesParams {
    /// Chat the messages are in.
    from_chat: String,
    /// Its name as list_chats or get_chat show it. Required when `from_chat` is a numeric id.
    from_chat_name: Option<String>,
    messages: Vec<MessageRef>,
    /// Chat to forward them to.
    to_chat: String,
    /// Its name as list_chats or get_chat show it. Required when `to_chat` is a numeric id.
    to_chat_name: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct SendReactionParams {
    /// `me`, `@username`, a t.me link, or a numeric id from list_chats.
    chat: String,
    /// The chat's name as list_chats or get_chat show it. Required when `chat` is a numeric id.
    chat_name: Option<String>,
    message_id: i32,
    /// The start of that message's `text` as get_messages shows it, at least 20 characters or
    /// all of it (its `media` if it has no text).
    message_text: String,
    /// A single emoji, e.g. 👍. Must be allowed in the chat.
    emoji: String,
}

#[derive(Deserialize, JsonSchema)]
struct NamedMessageParams {
    /// `me`, `@username`, a t.me link, or a numeric id from list_chats.
    chat: String,
    /// The chat's name as list_chats or get_chat show it. Required when `chat` is a numeric id.
    chat_name: Option<String>,
    message_id: i32,
    /// The start of that message's `text` as get_messages shows it, at least 20 characters or
    /// all of it (its `media` if it has no text).
    message_text: String,
}

#[derive(Deserialize, JsonSchema)]
struct ClickButtonParams {
    /// `me`, `@username`, a t.me link, or a numeric id of the chat with the message.
    chat: String,
    /// The chat's name as list_chats or get_chat show it. Required when `chat` is a numeric id.
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
    /// The user to add: `@username`, a t.me link, or a numeric id. Omit when adding by `phone`.
    user: Option<String>,
    /// The user's name as list_chats or get_chat show it. Required when `user` is a numeric id.
    user_name: Option<String>,
    /// Phone number in international format, to add someone by phone instead of `user`.
    phone: Option<String>,
    /// Name to save the contact under; defaults to their profile name. Required with `phone`.
    first_name: Option<String>,
    last_name: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct UserParams {
    /// `@username`, a t.me link, or a numeric id of a user.
    user: String,
    /// The user's name as list_chats or get_chat show it. Required when `user` is a numeric id.
    user_name: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct JoinChatParams {
    /// `@username` or t.me link of a public group/channel, or an invite link (t.me/+… or t.me/joinchat/…).
    chat: String,
    /// The chat's name as get_chat shows it, or for an invite link the `title` open_link shows.
    /// Required for invite links and numeric ids.
    chat_name: Option<String>,
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
        match name {
            Some(name) => {
                let actual = chat_info(&self.client.resolve_peer(peer).await.map_err(fail)?).name;
                if actual != name {
                    return Err(format!(
                        "{chat} is named {actual:?}, not {name:?}; nothing was done"
                    ));
                }
            }
            None if matches!(link.chat, ChatRef::Peer(_)) => {
                return Err(format!(
                    "{chat} is a numeric id: also pass `{name_field}`, its name as list_chats or get_chat show it, so the user can tell what it is"
                ));
            }
            None => {}
        }
        Ok(peer)
    }

    /// Fetches the messages and checks each against the text passed for it, so the user
    /// confirming the call sees what the messages say, not just their ids.
    async fn check_messages(
        &self,
        peer: PeerRef,
        messages: &[(i32, &str)],
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
                let start: String = actual.chars().take(40).collect();
                return Err(format!(
                    "message {id} starts {start:?}; pass at least its first {needed} characters as its text. Nothing was done"
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
        description = "Send a text message as the user.",
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
            .resolve_named(&params.chat, params.chat_name.as_deref(), "chat_name")
            .await?;
        if let Some(id) = params.reply_to {
            let text = params
                .reply_to_text
                .as_deref()
                .ok_or("`reply_to_text` is required with `reply_to`")?;
            self.check_messages(peer, &[(id, text)]).await?;
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
        description = "Upload a local file and send it to a chat as the user, with an optional caption.",
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
        let peer = self
            .resolve_named(&params.chat, params.chat_name.as_deref(), "chat_name")
            .await?;
        if let Some(id) = params.reply_to {
            let text = params
                .reply_to_text
                .as_deref()
                .ok_or("`reply_to_text` is required with `reply_to`")?;
            self.check_messages(peer, &[(id, text)]).await?;
        }
        let uploaded = self
            .client
            .upload_file(&params.path)
            .await
            .map_err(|e| format!("cannot read {}: {e}", params.path))?;
        let message = InputMessage::new()
            .markdown(params.caption.unwrap_or_default())
            .reply_to(params.reply_to);
        let extension = Path::new(&params.path)
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
        description = "Edit the text of one of the user's own messages.",
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
            .resolve_named(&params.chat, params.chat_name.as_deref(), "chat_name")
            .await?;
        self.check_messages(peer, &[(params.message_id, &params.message_text)])
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
        description = "Delete messages for everyone where possible. Irreversible.",
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
            .resolve_named(&params.chat, params.chat_name.as_deref(), "chat_name")
            .await?;
        let messages: Vec<(i32, &str)> = params
            .messages
            .iter()
            .map(|message| (message.id, message.text.as_str()))
            .collect();
        self.check_messages(peer, &messages).await?;
        let ids: Vec<i32> = messages.iter().map(|&(id, _)| id).collect();
        let deleted = self
            .client
            .delete_messages(peer, &ids)
            .await
            .map_err(fail)?;
        Ok(format!("deleted {deleted} messages"))
    }

    #[tool(
        description = "Forward messages from one chat to another as the user.",
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
            .resolve_named(
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
        self.check_messages(source, &messages).await?;
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
        description = "React to a message with an emoji, replacing the user's previous reaction.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = true
        )
    )]
    async fn send_reaction(
        &self,
        Parameters(params): Parameters<SendReactionParams>,
    ) -> Result<String, String> {
        let peer = self
            .resolve_named(&params.chat, params.chat_name.as_deref(), "chat_name")
            .await?;
        self.check_messages(peer, &[(params.message_id, &params.message_text)])
            .await?;
        self.client
            .send_reactions(peer, params.message_id, params.emoji.as_str())
            .await
            .map_err(fail)?;
        Ok(format!(
            "reacted {} to message {}",
            params.emoji, params.message_id
        ))
    }

    #[tool(
        description = "Pin a message in a chat.",
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
            .resolve_named(&params.chat, params.chat_name.as_deref(), "chat_name")
            .await?;
        self.check_messages(peer, &[(params.message_id, &params.message_text)])
            .await?;
        self.client
            .pin_message(peer, params.message_id)
            .await
            .map_err(fail)?;
        Ok(format!("pinned message {}", params.message_id))
    }

    #[tool(
        description = "Unpin a message in a chat.",
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
            .resolve_named(&params.chat, params.chat_name.as_deref(), "chat_name")
            .await?;
        self.check_messages(peer, &[(params.message_id, &params.message_text)])
            .await?;
        self.client
            .unpin_message(peer, params.message_id)
            .await
            .map_err(fail)?;
        Ok(format!("unpinned message {}", params.message_id))
    }

    #[tool(
        description = "Click a button under a message as the user, e.g. a bot's next page or filter: returns the bot's answer, the message after the click, and messages the click produced. Link and copy buttons are only reported, not opened.",
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
            .resolve_named(&params.chat, params.chat_name.as_deref(), "chat_name")
            .await?;
        let not_empty = |message: &&Message| !matches!(message.raw, tl::enums::Message::Empty(_));
        let message = self
            .check_messages(peer, &[(params.message_id, &params.message_text)])
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
        description = "Mark all messages in a chat as read.",
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
            .resolve_named(&params.chat, params.chat_name.as_deref(), "chat_name")
            .await?;
        self.client.mark_as_read(peer).await.map_err(fail)?;
        Ok(format!("marked {} as read", params.chat))
    }
}

/// Joining/leaving chats and managing contacts: `serve --access contacts`.
#[tool_router(router = contacts_tools, vis = "pub(super)")]
impl TelegramMcp {
    #[tool(
        description = "Join a group or channel as the user: @username, t.me link, or invite link. Public chats can be read without joining.",
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
                let name = params.chat_name.as_deref().ok_or(
                    "an invite link shows no name: also pass `chat_name`, the `title` open_link shows for it",
                )?;
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
                if title != name {
                    return Err(format!(
                        "{} is for {title:?}, not {name:?}; nothing was done",
                        params.chat
                    ));
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
        description = "Leave a group or channel as the user.",
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
        description = "Add someone to the user's Telegram contacts, by @username/id/link or by phone number. The user's own phone number is not shared.",
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
        description = "Remove someone from the user's Telegram contacts.",
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
