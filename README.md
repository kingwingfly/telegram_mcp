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

Add `"--allow-send"` after `"serve"` to enable the write tools. `--download-dir <PATH>` sets
where `download_media` saves files (default `<downloads dir>/telegram-mcp`).

Only `TELEGRAM_API_ID` is needed to serve; `TELEGRAM_API_HASH` is only used by `login`.
Logs go to stderr (`RUST_LOG` controls verbosity), since stdout carries the protocol.

## Tools

Chats are given as `me` (Saved Messages), `@username`, a t.me link, or a numeric id from
`list_chats`. Message text is Markdown.

| Read (always on) | Write (`serve --allow-send`) |
|---|---|
| `list_chats`, `get_chat`, `list_participants` | `send_message`, `send_file`, `edit_message`, `delete_messages` |
| `get_messages`, `search_messages` | `forward_messages`, `send_reaction`, `pin_message`, `unpin_message` |
| `download_media` | `mark_as_read`, `join_chat` |

Write tools act as you, under your name. Messages from anyone who can reach you end up in
the agent's context, so only enable them for agents you trust with your account.
`send_file` can upload any file the server process can read.

## Notes

- `grammers` is used from its git repository; `Cargo.lock` pins the commit.
  Run `cargo update -p grammers-client -p grammers-session` to move to the latest one.
- The session's directory must be writable too: SQLite creates a journal file next
  to the session on every write. If it isn't, requests fail with a bare `session error`.
