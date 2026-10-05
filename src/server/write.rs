use std::path::Path;

use grammers_client::message::InputMessage;
use grammers_session::types::PeerKind;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{
    ChatParams, ChatRef, MessageInfo, MessageParams, Messages, TelegramMcp, chat_info, fail,
    message_info, parse_link,
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
