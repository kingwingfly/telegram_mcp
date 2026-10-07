# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

A stdio MCP server that exposes Telegram to agents **as the user's own account** (not a bot), built on `rmcp` (server) and `grammers` (MTProto client). The user logs in once with the CLI; the MCP server reuses that session.

## Coding rules

- **No functions or methods unless they are reused.** Code used in one place stays inline at that place. Extract a helper only when a second caller exists.
- **No tests unless the process is complex.** Don't add tests for straightforward code (simple mappings, thin wrappers, argument plumbing).

## Commands

```sh
cargo build
cargo clippy --all-targets        # must stay warning-free
cargo fmt
cargo test                        # single test: cargo test <name>

cargo run -- login                # interactive login (dialoguer; needs a real TTY)
cargo run -- status               # which account the session belongs to
cargo run -- logout               # sign out + delete session file
cargo run                         # = `serve`: MCP over stdio, read-only tools
cargo run -- serve --access contacts       # also join/leave chats, add/delete contacts
cargo run -- serve --access contacts,send  # groups are independent; combine them
```

`serve` also takes `--download-dir <PATH>` (where `download_media` saves; default `<downloads dir>/telegram-mcp`). Global flags: `--env-file <PATH>` (otherwise `.env` is searched upward from the cwd) and `--session <PATH>` (or `TELEGRAM_MCP_SESSION`; default `<data dir>/telegram-mcp/telegram.session`).

To smoke-test against the real account, pipe JSON-RPC lines into it (tool calls go after the handshake):

```sh
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_chat","arguments":{"chat":"me"}}}' \
  | cargo run -q -- serve
```

Test write tools only in Saved Messages (`"chat": "me"`), and delete what you create.

## Architecture

- `main.rs`: clap CLI; dispatches to `auth` (login/status/logout) or `server` (serve, the default).
- `config.rs`: loads `.env`, reads `TELEGRAM_API_ID` (always needed) and `TELEGRAM_API_HASH` (only needed by `login`, to request a code). The hash is wrapped in `ApiHash`, whose `Debug` is redacted. Never print or log it.
- `telegram.rs`: `Telegram::connect` opens the SQLite session (owner-only permissions) wrapped in `Session` (see Gotchas), builds a grammers `SenderPool`, spawns its runner task, and returns the `Client`. Always call `Telegram::shutdown()` on every path, including errors, so the runner stops cleanly. The pattern used is `let result = ...; tg.shutdown().await; result`. The pool's update receiver is dropped because updates are not consumed yet.
- `server.rs`: `TelegramMcp` (the `ServerHandler`), `serve`, and what the tool modules share: `parse_link` (every chat/link format → `ChatRef` + optional message id), `resolve` / `resolve_chat`, output types (`ChatInfo`, `MessageInfo`, `Messages`), shared params, and the `chat_info` / `message_info` / `fail` helpers. `serve` refuses to start unless the session is authorized.
- `server/read.rs`: `read_tools` router (always registered) plus `chat_details`, shared by `get_chat` and `open_link` (full info, profile/linked chat, pinned messages).
- Raw API calls (`client.invoke`): grammers can't build `Message`/`Peer` maps from them (`PeerMap` has no public constructor). Call `self.cache_peers(&chats, &users)` on every raw response so the ids tools report stay usable; to get full messages, take the ids and refetch with `get_messages_by_id` (as `get_replies` does with `messages.getReplies`).
- `server/write.rs`: `contacts_tools` (join/leave chats, add/delete contacts) and `send_tools`, registered independently by `serve --access contacts,send` (`server::Access`). Each router is a `#[tool_router(router = …, vis = "pub(super)")]` impl. A new tool goes in the router matching what it changes on Telegram, and gets `annotations(...)` hints like its neighbours.
- Tool conventions:
  - Every tool takes `chat` as a string and calls `self.resolve(...)`, which accepts anything `parse_link` does. Numeric ids need the peer's access hash, which `resolve` reads from the session cache; the client fills that cache automatically whenever peers are seen, e.g. by `list_chats`. Usernames are cached in memory (`TelegramMcp::usernames`) because `contacts.resolveUsername` hits FLOOD_WAIT quickly.
  - Tools return `Result<Json<T>, String>` (or `Result<String, String>` for plain acknowledgements). `Err(String)` becomes an `isError` result the agent can read; map errors with `.map_err(fail)`. `Json<T>` must wrap an object, never a bare `Vec`.
  - Output fields left out with `skip_serializing_if` must be `Option` or also carry `#[serde(default)]` (e.g. `Vec` fields). Otherwise schemars lists them as `required` in the output schema, and clients that validate (Claude Code does) reject the whole result.
  - Buttons: `message_buttons` decodes inline and reply-keyboard markup once, as (`Button` for output, `ButtonAction` for `click_button`); add new button types there.
  - Message text is Markdown in both directions (grammers `markdown` feature: `message.markdown_text()` / `InputMessage::markdown`).

## Gotchas

- **stdout is the MCP protocol channel.** All logging goes to stderr via `tracing` (`RUST_LOG` controls it; grammers' `log` output is bridged). Never `println!` from the serve path.
- **grammers is a git dependency** (codeberg `Lonami/grammers`), pinned by `Cargo.lock`. Use the source in `~/.cargo/git/checkouts/` as the API reference, not docs.rs or crates.io 0.10.0, which differ. `grammers-session` is a direct dependency only to enable its `sqlite-storage` feature (grammers-client disables it).
- **The session directory must be writable**, not just the file: SQLite creates a journal next to it on every write, and grammers writes on every connect. When it can't, grammers reports only a bare `session error` (the inner cause is hidden).
- `rmcp`'s `Implementation::from_build_env()` reports `rmcp`'s own crate name. Use `Implementation::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))`.
- grammers marks `Media`, `Role` and similar enums `#[non_exhaustive]`, and new TL variants appear upstream (e.g. `Community`). Matches need a wildcard arm.
- `get_messages_by_id` returns placeholders (`tl::enums::Message::Empty`) for deleted or missing ids; filter them out.
- Saved Messages: Telegram reports `out = false` there, so `message_info` also treats "sender is me" as outgoing. `from_me` search returns nothing there, and reactions are Premium-only.
- grammers' SQLite `cache_peers` (git e7730ae) stores only the first new peer of each batch, which grammers itself calls for every response. `telegram::Session` wraps `SqliteSession` and falls back to the trait's one-by-one default; keep using it until upstream fixes that.
- `delete_dialog` leaves channels/groups but *deletes the history* of private chats; `leave_chat` refuses users for that reason.
- Inline buttons use the newer layout: `KeyboardInlineButtonRow` → `KeyboardInlineButton { text, type: InlineButtonType }`.
- `PeerId::self_user()` is a sentinel id; `resolve_peer` can't match it. `me` resolves to the real `PeerRef` captured at startup (`TelegramMcp::me`).
- dialoguer prompts are blocking; they run through `spawn_blocking` (the `ask` helper in `auth.rs`) so the grammers runner keeps going.
