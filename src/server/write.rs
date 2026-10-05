use std::path::Path;

use grammers_client::message::InputMessage;
use grammers_client::peer::{Peer, User};
use grammers_client::tl;
use grammers_session::types::PeerKind;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{
    ChatInfo, ChatParams, ChatRef, MessageInfo, MessageParams, Messages, TelegramMcp, chat_info,
    fail, message_info, parse_link,
};

#[derive(Deserialize, JsonSchema)]
struct SendMessageParams {
    /// `me`, `@username`, a t.me link, or a numeric id from list_chats.
    chat: String,
    /// Markdown: **bold**, _italic_, `code`, ```pre```, [link](https://…).
    text: String,
    /// Send as a reply to this message id.
    reply_to: Option<i32>,
}

#[derive(Deserialize, JsonSchema)]
struct SendFileParams {
    /// `me`, `@username`, a t.me link, or a numeric id from list_chats.
    chat: String,
    /// Path of the local file to upload. .jpg/.png/.webp are sent as photos, anything else as a file.
    path: String,
    /// Markdown caption.
    caption: Option<String>,
    /// Send as a reply to this message id.
    reply_to: Option<i32>,
}

#[derive(Deserialize, JsonSchema)]
struct EditMessageParams {
    /// `me`, `@username`, a t.me link, or a numeric id from list_chats.
    chat: String,
    message_id: i32,
    /// New Markdown text.
    text: String,
}

#[derive(Deserialize, JsonSchema)]
struct MessageIdsParams {
    /// `me`, `@username`, a t.me link, or a numeric id from list_chats.
    chat: String,
    message_ids: Vec<i32>,
}

#[derive(Deserialize, JsonSchema)]
struct ForwardMessagesParams {
    /// Chat the messages are in.
    from_chat: String,
    message_ids: Vec<i32>,
    /// Chat to forward them to.
    to_chat: String,
}

#[derive(Deserialize, JsonSchema)]
struct SendReactionParams {
    /// `me`, `@username`, a t.me link, or a numeric id from list_chats.
    chat: String,
    message_id: i32,
    /// A single emoji, e.g. 👍. Must be allowed in the chat.
    emoji: String,
}

#[derive(Deserialize, JsonSchema)]
struct AddContactParams {
    /// The user to add: `@username`, a t.me link, or a numeric id. Omit when adding by `phone`.
    user: Option<String>,
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
}

#[derive(Deserialize, JsonSchema)]
struct JoinChatParams {
    /// `@username` or t.me link of a public group/channel, or an invite link (t.me/+… or t.me/joinchat/…).
    chat: String,
}

#[tool_router(router = write_tools, vis = "pub(super)")]
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
        let peer = self.resolve(&params.chat).await?;
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
        let peer = self.resolve(&params.chat).await?;
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
        let peer = self.resolve(&params.chat).await?;
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
        Parameters(params): Parameters<MessageIdsParams>,
    ) -> Result<String, String> {
        let peer = self.resolve(&params.chat).await?;
        let deleted = self
            .client
            .delete_messages(peer, &params.message_ids)
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
        let source = self.resolve(&params.from_chat).await?;
        let destination = self.resolve(&params.to_chat).await?;
        let forwarded = self
            .client
            .forward_messages(destination, &params.message_ids, source)
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
        let peer = self.resolve(&params.chat).await?;
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
        Parameters(params): Parameters<MessageParams>,
    ) -> Result<String, String> {
        let peer = self.resolve(&params.chat).await?;
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
        Parameters(params): Parameters<MessageParams>,
    ) -> Result<String, String> {
        let peer = self.resolve(&params.chat).await?;
        self.client
            .unpin_message(peer, params.message_id)
            .await
            .map_err(fail)?;
        Ok(format!("unpinned message {}", params.message_id))
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
                let peer = self.resolve(&user).await?;
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
        let peer = self.resolve(&params.user).await?;
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
        Parameters(params): Parameters<ChatParams>,
    ) -> Result<String, String> {
        let peer = self.resolve(&params.chat).await?;
        self.client.mark_as_read(peer).await.map_err(fail)?;
        Ok(format!("marked {} as read", params.chat))
    }
}

/// Entering and leaving chats: registered with `serve --allow-join` (or `--allow-send`).
#[tool_router(router = membership_tools, vis = "pub(super)")]
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
            ChatRef::Invite(_) => self.client.accept_invite_link(&params.chat).await,
            chat => {
                let peer = self.resolve_chat(&chat).await?;
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
        Parameters(params): Parameters<ChatParams>,
    ) -> Result<String, String> {
        let peer = self.resolve(&params.chat).await?;
        // For private chats grammers' delete_dialog deletes the history instead.
        if peer.id.kind() == PeerKind::User {
            return Err("leave_chat only works for groups and channels".to_owned());
        }
        self.client.delete_dialog(peer).await.map_err(fail)?;
        Ok(format!("left {}", params.chat))
    }
}
