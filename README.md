# selfsync

Self-hosted Chrome Sync server. Keep your bookmarks, passwords, preferences, and other browser data in sync across devices — without sending anything to Google.

## How It Works

```
Chrome ──(--sync-url)──> selfsync-server ──> SQLite
```

Chrome natively supports pointing sync traffic to a custom server via the `--sync-url` flag. selfsync implements the Chrome Sync protocol (protobuf over HTTP) and stores everything locally in a single SQLite file.

## Quick Start

```bash
# Build
cargo build --release

# Start the server
./target/release/selfsync-server

# Launch Chrome pointing to your server
google-chrome-stable --sync-url=http://127.0.0.1:8080
```

Sign into Chrome with your Google account, enable sync, and you're done. All sync data stays on your machine.

## What's Inside

| Crate | Description |
|-------|-------------|
| **selfsync-server** | Sync server — axum, sea-orm, SQLite, 92 Chromium proto files |
| **selfsync-nigori** | Nigori encryption — AES-128-CBC + HMAC-SHA256, PBKDF2/Scrypt key derivation |
| **selfsync-payload** | LD_PRELOAD injector — auto-redirects Chrome sync + adds per-user email headers |

## Server

### Endpoints

| Route | Method | Description |
|-------|--------|-------------|
| `/command/` | POST | Chrome Sync protocol (protobuf) |
| `/chrome-sync/command/` | POST | Same as above (alternate path) |
| `/` | GET | User list (HTML) |

### Configuration

| Env Var | Default | Description |
|---------|---------|-------------|
| `SELFSYNC_ADDR` | `127.0.0.1:8080` | Listen address |
| `SELFSYNC_DB` | `selfsync.db` | SQLite database path |
| `RUST_LOG` | `selfsync_server=info` | Log level |

### How It Stores Data

- **SQLite** with WAL mode — single file, zero setup
- **Single `sync_entities` table** — no sharding, no complexity
- **Per-user version counter** for conflict detection
- **Nigori keystore passphrase** auto-generated on first sync

On first connect, the server automatically creates:
- Nigori encryption node (keystore passphrase)
- Bookmark root folders (Bookmark Bar, Other Bookmarks, Mobile Bookmarks)

### Authentication

Without the LD_PRELOAD payload, authentication is trivial: Chrome sends its Google OAuth token (which the server ignores), and all data goes under a default `anonymous@localhost` user. This is fine for single-user self-hosting.

With the payload, the proxy injects an `X-Sync-User-Email` header, enabling multi-user support.

## LD_PRELOAD Payload (Optional)

For multi-user setups or when you want automatic per-profile email identification:

```bash
LD_PRELOAD=./target/release/libselfsync_payload.so google-chrome-stable
```

The payload:
1. Hooks `__libc_start_main` to detect Chrome's browser process
2. Reads Chrome's Preferences to build a `cache_guid → email` mapping
3. Starts a local HTTP proxy that adds `X-Sync-User-Email` headers
4. Injects `--sync-url` pointing to the proxy

## Nigori Library

Standalone Rust implementation of the [Nigori protocol](https://www.cl.cam.ac.uk/~drt24/nigori/nigori-overview.pdf), compatible with Chromium's sync encryption:

```rust
use selfsync_nigori::{Nigori, KeyDerivationParams};

let nigori = Nigori::create_by_derivation(
    &KeyDerivationParams::pbkdf2(),
    "passphrase",
)?;

let encrypted = nigori.encrypt(b"secret data");
let decrypted = nigori.decrypt(&encrypted)?;
```

Validated against Chromium and [go-nigori](https://github.com/nicktcortes/nicktcortes) test vectors.

## Building

```bash
cargo build --release                        # Everything
cargo build --release -p selfsync-server     # Server only
cargo build --release -p selfsync-payload    # Payload .so only
cargo test                                   # Run tests
cargo clippy                                 # Lint
```

Requires Rust 2024 edition (1.85+) and `protoc` is NOT needed — proto compilation uses prost-build.

## Project Structure

```
selfsync/
├── crates/
│   ├── sync-server/     # Chrome sync server
│   │   ├── proto/       # 92 Chromium .proto files
│   │   └── src/
│   ├── nigori/          # Nigori encryption library
│   │   └── src/
│   └── payload/         # LD_PRELOAD Chrome injector
│       └── src/
└── docs/
```

## Prior Art

- Chromium's `components/sync/engine/loopback_server/loopback_server.cc` — Reference sync server implementation

## License

MIT
