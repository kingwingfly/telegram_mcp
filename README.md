# telegram-mcp

An MCP server (stdio or HTTP) that gives agents access to Telegram **as your user account**,
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
   (`~/.local/share/...` on Linux) and created with owner-only permissions; later runs keep
   whatever permissions or ACLs you give it. Override it with
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

Read tools are always on. Add `"--access", "<groups>"` after `"serve"` to enable tools that act
as you, comma-separated: `contacts` (join/leave chats, manage contacts) and/or `send`
(everything else), e.g. `"--access", "contacts,send"`. `--send-to <PATTERNS>` limits the
`send` group to some chats (see below). `--download-dir <PATH>` sets where `download_media`
saves files (default `<downloads dir>/telegram-mcp`), and `--max-download-mb <MB>` the largest
file it saves (default 20 MiB). `--upload-dir <PATH>` is the only folder `send_file` sends
files from; without it, there is no `send_file`.

Only `TELEGRAM_API_ID` is needed to serve; `TELEGRAM_API_HASH` is only used by `login`.
Logs go to stderr (`RUST_LOG` controls verbosity), since stdout carries the protocol.

### Over HTTP

`serve --listen <ADDR>` serves MCP over Streamable HTTP at `http://<ADDR>/mcp` instead of
stdio, until Ctrl-C. One running server can then be shared by several clients, including
ones that can't read the session file:

```sh
telegram-mcp --env-file /path/to/telegram_mcp/.env serve --listen 127.0.0.1:8000
```

```json
{
  "mcpServers": {
    "telegram": { "type": "http", "url": "http://127.0.0.1:8000/mcp" }
  }
}
```

- **There is no authentication.** Whoever can connect to the address can use the account
  with the `--access` the server was started with, so keep it on a loopback address and
  reach it from other machines through something that authenticates (e.g. an SSH tunnel).
- Clients must address the server by a loopback name (`localhost`, `127.0.0.1`, `::1`) or by
  the IP in `--listen`; requests with any other `Host` get `403`. This stops web pages from
  reaching it through DNS rebinding. To accept connections on a network address, listen on
  that IP rather than `0.0.0.0`.
- `download_media` saves files, and `send_file` reads them, on the machine running the server.

## Tools

Chats are given as `me` (Saved Messages), `@username`, a numeric id from `list_chats`, or any
Telegram link (`t.me/…`, `telegram.me/…`, `tg://…`, including message links). Message text is
Markdown; messages also carry their buttons (inline and bot keyboards), reaction counts,
link-preview URL, forward source and t.me link. `get_reactions` lists who reacted (not in
channels); `send_reaction` sets your reactions to a message, and an empty list removes them.
`click_button` presses a bot's callback button (next page, filters) and returns the bot's
answer, the message as edited by the bot, and any messages it sent.

| Read (always on) | `--access contacts` | `--access send` |
|---|---|---|
| `list_chats`, `get_chat`, `list_participants` | `join_chat` | `send_message`, `send_file` (also needs `--upload-dir`), `edit_message`, `delete_messages` |
| `get_messages`, `get_replies`, `get_reactions`, `search_messages` | `leave_chat` | `forward_messages`, `send_reaction`, `pin_message`, `unpin_message` |
| `open_link`, `download_media`, `list_contacts` | `add_contact`, `delete_contact` | `mark_as_read`, `click_button` |

`get_chat` (and `open_link` on a chat) returns the chat's pinned messages, where groups and
channels usually keep their index, plus a user's profile channel or a channel's discussion
group. Messages show a `replies` count; `get_replies` reads a channel post's comments (from its
discussion group) or a group message's reply thread. Telegram rate-limits `@username` lookups,
so resolved usernames are cached per server run.

Write tools act as you, under your name. Messages from anyone who can reach you end up in
the agent's context, so only enable them for agents you trust with your account.
`send_file` uploads only regular files inside `--upload-dir` (symlinks and `..` are resolved
before the check), and isn't offered at all without that flag. The folder must not hold the
session file or `.env`: `serve` refuses to start if it does. Give it the same folder as
`--download-dir` to let the agent re-send what `download_media` saved.

`--send-to` limits every `send` tool (sending, editing, deleting, forwarding into, reacting,
pinning, marking read, clicking buttons) to chats matching one of its patterns,
comma-separated: a chat's `@username`, its numeric id, or `me` for Saved Messages, with `*`
for any characters. For example, `--access send --send-to @search_bot,me` lets the agent talk
to one bot and keep notes, and nothing else. Usernames are matched rather than names, which
anyone can copy. Read tools and `--access contacts` are not limited by it.

Telegram's service chat (`777000`, "Telegram") receives the account's login codes, and
whoever reads a code can log in as you. Every tool refuses that chat, and `list_chats` and
`search_messages` leave it out, unless `serve` is given `--allow-service-chat`. Set a 2FA
password on the account anyway.

## Notes

- `grammers` is used from its git repository; `Cargo.lock` pins the commit.
  Run `cargo update -p grammers-client -p grammers-session` to move to the latest one.
- The session's directory must be writable too: SQLite creates a journal file next
  to the session on every write. If it isn't, requests fail with a bare `session error`.
