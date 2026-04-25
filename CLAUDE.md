# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

**selfsync** — self-hosted Chrome / Edge sync solution. A Cargo workspace with two crates:

- **selfsync-server** — sync server (axum + sea-orm + SQLite). Handles `COMMIT` and `GET_UPDATES` via protobuf. User identity is `(email, browser_kind)` parsed from request.
- **selfsync-nigori** — Nigori encryption library (AES-128-CBC + HMAC-SHA256, PBKDF2/Scrypt key derivation).

## Build & Test

```bash
cargo build --release                        # Build all
cargo build --release -p selfsync-server     # Server only
cargo check                                  # Type check workspace
cargo clippy --all-targets -- -D warnings    # Strict lint
cargo test                                   # Run tests
```

## Project Structure

```
selfsync/
├── crates/
│   ├── nigori/          # Nigori encryption library
│   │   └── src/
│   │       ├── lib.rs       # Nigori struct: encrypt/decrypt/get_key_name
│   │       ├── keys.rs      # PBKDF2 and Scrypt key derivation
│   │       ├── stream.rs    # NigoriStream binary serialization
│   │       └── error.rs     # Error types
│   └── sync-server/     # Sync server
│       ├── proto/           # 92 Chromium .proto files
│       ├── build.rs         # prost-build proto compilation
│       └── src/
│           ├── main.rs      # axum server entry point + routes + CLI
│           ├── proto.rs     # Generated protobuf types
│           ├── auth.rs      # BrowserKind + ClientIdentity::from_request
│           ├── progress.rs  # Progress token encoding/decoding
│           ├── util.rs      # Shared utilities (gen_id, now_millis, BASE64)
│           ├── db/
│           │   ├── mod.rs           # connect() runs Migrator::up
│           │   ├── entity/          # sea-orm Models (user has browser() helper)
│           │   └── migrator/        # sea-orm-migration files
│           └── handler/
│               ├── mod.rs       # log_request middleware + Diagnostic stub
│               ├── sync.rs      # POST /command/ dispatch
│               ├── commit.rs    # COMMIT: create/update entities
│               ├── get_updates.rs # GET_UPDATES: fetch by version
│               ├── init.rs      # User initialization (Nigori + bookmarks per browser)
│               └── users.rs     # GET / user list page
```

## Sync Server

- **User identity**: `(email, browser_kind)` composite key. Edge `a@b.com` and Chromium `a@b.com` are distinct user rows by design — incompatible cryptographers and permanent folder sets.
- **Endpoints**:
  - `POST /command/`, `POST /chrome-sync/command/` — Chromium-family entry points
  - `POST /v1/feeds/me/syncEntities[/command][/]` — Edge entry point variants
  - `POST /sync/v1/diagnosticData/Diagnostic.SendCheckResult()[/]` — Edge MSA stub returning fixed 6-byte success envelope (required for BookmarkDataTypeController init only)
  - `GET /` — HTML user dashboard
  - `GET /healthz` — liveness check
- **Auth**: `auth::ClientIdentity::from_request` parses `(email, browser)` per request. Email from protobuf `share` (Chromium puts the signed-in account email; Edge puts a base64 cache_guid → fallback to `anonymous@localhost`). Browser from `X-AFS-ClientInfo: app=Microsoft Edge` (Edge sets this; vanilla Chromium omits it).
- **Storage**: SQLite (WAL mode), single `sync_entities` table. Per-user monotonic counter `users.next_version`.
- **Progress tokens**: `v1,{data_type_id},{version}` base64-encoded.
- **Schema migrations**: `sea-orm-migration` runs at startup (`Migrator::up`). Append new files in `db/migrator/`, never edit released ones. SQLite has no `ALTER TABLE DROP CONSTRAINT` so removing column-level UNIQUE requires a table rebuild via raw SQL.
- **Config env vars**: `SELFSYNC_DB` (default `selfsync.db`), `SELFSYNC_ADDR` (default `127.0.0.1:8080`).
- **User init** (per-browser branching in `handler/init.rs`):
  - Chromium → KEYSTORE_PASSPHRASE Nigori (server-built, PBKDF2-derived) + 4 bookmark permanent folders
  - Edge → empty IMPLICIT_PASSPHRASE Nigori (triggers client-side init via MSA keys) + 5 folders (extra: `workspace_bookmarks`)
- **Proto module**: `proto.rs` wraps generated code with `#[allow(clippy::all, dead_code, deprecated)]`.

## Chrome / Edge Sync Protocol Gotchas

- `--sync-url=http://host:port` — browser appends `/command/` automatically; do NOT include it
- `ClientToServerMessage.share` — Chromium puts user email; **Edge puts a base64 cache_guid** (root cause of Edge being single-user in selfsync)
- `ClientToServerResponse.error_code` must be explicitly set to `SUCCESS (0)` — proto default `UNKNOWN` is treated as error
- `ClientCommand` + `NewBagOfChips` populated on every response (real MSA does this; some clients use them for scheduling / health)
- `NigoriSpecifics.encrypt_everything` MUST be `false`: selfsync sends plaintext BookmarkSpecifics, setting true is a protocol contradiction that breaks BookmarkDataTypeProcessor initial merge
- `NigoriSpecifics.passphrase_type`: `IMPLICIT_PASSPHRASE = 1`, `KEYSTORE_PASSPHRASE = 2`, `CUSTOM_PASSPHRASE = 4`
- `store_birthday` must be the literal `"ProductionEnvironmentDefinition"` — Edge BookmarkDataTypeProcessor validates this exact value at initial merge
- Edge needs a 5th permanent bookmark folder `workspace_bookmarks` ("Workspaces") under root; missing it triggers `kBookmarksInitialMergePermanentEntitiesMissing` (the Type 64 trap)
- Browser caches Nigori state locally; after server DB reset, use a fresh profile (`--user-data-dir=/tmp/test`)
- NEW_CLIENT GetUpdates expects Nigori entity to exist; without it client stalls at "Initializing"
- GetUpdates response must include `encryption_keys` when `need_encryption_key=true` OR client subscribes Nigori OR response has KEYSTORE Nigori
- prost generates `EntitySpecifics.specifics_variant` (oneof), not flat fields like `bookmark`/`nigori`
- Proto field is `client_tag_hash` (not `client_defined_unique_tag`); `message_contents` is `i32` (not enum)
- Chromium proto imports use `components/sync/protocol/` prefix — strip when copying to local `proto/` dir

## Key Chromium Source References

Relevant paths in `~/modous/chromium/src/`:

- `components/sync/base/sync_util.cc::GetSyncServiceURL()` — reads `--sync-url`
- `components/sync/engine/sync_manager_impl.cc::MakeConnectionURL()` — appends `/command/`
- `components/sync/engine/net/url_translator.cc::AppendSyncQueryString()` — adds `client` / `client_id`
- `components/sync/engine/net/http_bridge.cc::MakeAsynchronousPost()` — HTTP request construction
- `components/sync/protocol/sync.proto` — `ClientToServerMessage` / `ClientToServerResponse`
- `components/sync/engine/loopback_server/loopback_server.cc` — reference sync server implementation
- `components/sync_bookmarks/bookmark_data_type_processor.cc::OnInitialUpdateReceived()` — Type 64 / `kBookmarksInitialMergePermanentEntitiesMissing` source
- `components/sync/nigori/nigori_sync_bridge_impl.cc::MergeFullSyncData()` — empty IMPLICIT Nigori → client-side init fall-through
