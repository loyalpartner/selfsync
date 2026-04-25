# selfsync

[中文文档](README.zh-CN.md)

Self-hosted Chrome / Edge sync server. Keep your bookmarks, passwords, preferences, and other browser data in sync across devices — without sending anything to Google or Microsoft.

> [!CAUTION]
> **Do NOT expose this server to the public internet.** selfsync performs no authentication — anyone who can reach the port can read and overwrite synced data, including saved passwords. Run it on trusted private networks (LAN, NAS, home lab) or behind a zero-trust tunnel (Tailscale, Cloudflare Zero Trust, WireGuard).

## How It Works

Chromium-family browsers natively support syncing to a custom server via the `--sync-url` flag. selfsync implements the Chrome Sync protocol and stores everything locally in a single SQLite file.

Per-user records use the composite key `(email, browser_kind)`: Edge with `alice@example.com` and Chrome with `alice@example.com` are **separate** user rows in the database, because the two browsers use incompatible cryptographers and ship different permanent bookmark folders. Trying to share one row would corrupt sync state on every commit.

## Browser Support

Only Chrome and Edge are tested. Other Chromium-derived browsers (Brave, Vivaldi, Arc, Opera, …) are likely to work because the server treats anything without an `X-AFS-ClientInfo: app=Microsoft Edge` header as standard Chromium — but we have not verified them.

| Browser | Sync works | Multi-user | Notes |
|---|---|---|---|
| Chrome | ✅ | ✅ per email | Standard path |
| **Microsoft Edge** | ✅ | ❌ **single user only** | See below |

### Edge Limitation

Edge **does not send the signed-in account email** in its sync requests. Where vanilla Chromium puts your Google account email in the protobuf `share` field, Edge puts a base64-encoded device GUID. selfsync has no way to recover the real account from that, so **all Edge devices fall into one shared user record** (`anonymous@localhost`, `browser_kind=edge`).

What this means in practice:

- ✅ **Multiple Edge devices for the same person** sync correctly with each other — the shared record is exactly what you want.
- ⚠️ **Multiple Edge users on the same selfsync instance** end up sharing one merged dataset (their bookmarks, passwords, etc. all converge into the same record). If you need user separation, run one selfsync instance per user (different port / DB file / container).
- ✅ **Edge and Chrome on the same person's account** are kept isolated — they are independent user rows by design (incompatible Nigori encryption).

## Quick Start

### Option 1: Docker Compose (recommended)

```bash
docker compose up -d
```

Data is persisted to a Docker volume.

### Option 2: Docker

```bash
docker build -t selfsync .
docker run -d -p 8080:8080 -v ./data:/data selfsync
```

### Option 3: Build from source

```bash
cargo build --release
./target/release/selfsync-server
```

### Point your browser at the server

**Chrome / Chromium:**

```bash
google-chrome-stable --sync-url=http://127.0.0.1:8080
```

**Microsoft Edge:**

```bash
microsoft-edge --sync-url=http://127.0.0.1:8080
```

Then sign in (any account) and turn sync on. Chrome will upload its local bookmarks / passwords / settings to selfsync. No export/import needed.

## Configuration

<!-- AUTO-GENERATED:cli-env -->
| Variable | Default | Description |
|----------|---------|-------------|
| `SELFSYNC_ADDR` | `127.0.0.1:8080` | TCP address to bind |
| `SELFSYNC_DB` | `selfsync.db` | SQLite database path |
| `RUST_LOG` | `selfsync_server=info,http=info` | Log filter (tracing-subscriber syntax) |
<!-- /AUTO-GENERATED -->

In the Docker image the defaults are overridden to `0.0.0.0:8080` and `/data/selfsync.db`.

## Endpoints

<!-- AUTO-GENERATED:routes -->
| Path | Method | Purpose |
|---|---|---|
| `/` | GET | HTML user dashboard |
| `/healthz` | GET | Liveness check (returns `ok`) |
| `/command/`, `/command` | POST | Chrome sync protocol entry point |
| `/chrome-sync/command/`, `/chrome-sync/command` | POST | Alternate path (works with `--sync-url=http://host:port/chrome-sync`) |
| `/v1/feeds/me/syncEntities[/command][/]` | POST | Edge sync path variants |
| `/sync/v1/diagnosticData/Diagnostic.SendCheckResult()[/]` | POST | Edge MSA private endpoint stub (returns the same 6-byte success envelope as real MSA — required for `BookmarkDataTypeController` initialization) |
| `/v1/diagnosticData/Diagnostic.SendCheckResult()[/]` | POST | Same, alternate prefix |
<!-- /AUTO-GENERATED -->

## Things to Watch Out For

- **Do NOT include `/command/` in `--sync-url`**. The browser appends it. Use `http://127.0.0.1:8080`.
- **After resetting the server database**, use a fresh browser profile (`--user-data-dir=/tmp/test`) to avoid stale local sync state.
- **Upgrading from v0.1.1**: just replace the binary. Schema migrations run automatically — no need to delete `selfsync.db`.

## Building

Requires Rust 1.85+.

<!-- AUTO-GENERATED:cargo -->
| Command | Purpose |
|---|---|
| `cargo build --release` | Build the workspace |
| `cargo build --release -p selfsync-server` | Build the server only |
| `cargo test` | Run unit tests |
| `cargo clippy --all-targets -- -D warnings` | Strict lint check |
<!-- /AUTO-GENERATED -->

## Architecture

For protocol details, browser-specific quirks (Edge MSA Nigori, `workspace_bookmarks`), database schema, and migration internals, see [docs/architecture.md](docs/architecture.md).

## Prior Art

- Chromium `loopback_server.cc` — Reference sync server implementation

## License

Copyright (C) 2026 Lee &lt;loyalpartner@163.com&gt;

Licensed under [GPL-3.0-or-later](LICENSE).
