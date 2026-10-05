# telegram-mcp

An MCP server (stdio) that gives agents access to Telegram **as your user account**,
built on [`rmcp`](https://crates.io/crates/rmcp) and [`grammers`](https://codeberg.org/Lonami/grammers).

## Setup

1. Get an API ID and hash from <https://my.telegram.org/apps> and put them in `.env`:

   ```sh
   TELEGRAM_API_ID=123456
   TELEGRAM_API_HASH=0123456789abcdef0123456789abcdef
   ```

2. Log in once, interactively (phone number → login code → 2FA password if enabled):

   ```sh
   cargo run -- login
   cargo run -- status    # check which account is logged in
   cargo run -- logout    # sign out and delete the session
   ```

   The session is stored at `<data dir>/telegram-mcp/telegram.session`
   (`~/.local/share/...` on Linux), with owner-only permissions. Override it with
   `--session <PATH>` or `TELEGRAM_MCP_SESSION`. Treat this file like a password: it
   grants full access to the account.

## Connecting an MCP client

`serve` is the default command; it refuses to start until `login` has been completed.
MCP clients usually launch servers from another directory, so point at `.env` explicitly:

```json
{
  "mcpServers": {
    "telegram": {
      "command": "/path/to/telegram-mcp",
      "args": ["--env-file", "/path/to/telegram_mcp/.env", "serve"]
    }
  }
}
```

Read tools are always on. Add `"--access", "contacts"` after `"serve"` to also let the agent
join/leave chats and manage contacts, or `"--access", "send"` to enable every tool that acts as
you (it includes `contacts`). `--download-dir <PATH>` sets
where `download_media` saves files (default `<downloads dir>/telegram-mcp`).

Only `TELEGRAM_API_ID` is needed to serve; `TELEGRAM_API_HASH` is only used by `login`.
Logs go to stderr (`RUST_LOG` controls verbosity), since stdout carries the protocol.

## Tools

Chats are given as `me` (Saved Messages), `@username`, a numeric id from `list_chats`, or any
Telegram link (`t.me/…`, `telegram.me/…`, `tg://…`, including message links). Message text is
Markdown; messages also carry their buttons (inline and bot keyboards), link-preview URL, forward
source and t.me link. `click_button` presses a bot's callback button (next page, filters) and
returns the bot's answer, the message as edited by the bot, and any messages it sent.

| Read (always on) | `--access contacts` | `--access send` (includes contacts) |
|---|---|---|
| `list_chats`, `get_chat`, `list_participants` | `join_chat` | `send_message`, `send_file`, `edit_message`, `delete_messages` |
| `get_messages`, `get_replies`, `search_messages` | `leave_chat` | `forward_messages`, `send_reaction`, `pin_message`, `unpin_message` |
| `open_link`, `download_media`, `list_contacts` | `add_contact`, `delete_contact` | `mark_as_read`, `click_button` |

`get_chat` (and `open_link` on a chat) returns the chat's pinned messages, where groups and
channels usually keep their index, plus a user's profile channel or a channel's discussion
group. Messages show a `replies` count; `get_replies` reads a channel post's comments (from its
discussion group) or a group message's reply thread. Telegram rate-limits `@username` lookups,
so resolved usernames are cached per server run.

Write tools act as you, under your name. Messages from anyone who can reach you end up in
the agent's context, so only enable them for agents you trust with your account.
`send_file` can upload any file the server process can read.

## Notes

- `grammers` is used from its git repository; `Cargo.lock` pins the commit.
  Run `cargo update -p grammers-client -p grammers-session` to move to the latest one.
- The session's directory must be writable too: SQLite creates a journal file next
  to the session on every write. If it isn't, requests fail with a bare `session error`.
