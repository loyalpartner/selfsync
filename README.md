# selfsync

[中文文档](README.zh-CN.md)

Self-hosted Chrome Sync server. Keep your bookmarks, passwords, preferences, and other browser data in sync across devices — without sending anything to Google.

## How It Works

Chrome natively supports syncing to a custom server via the `--sync-url` flag. selfsync implements the Chrome Sync protocol and stores everything locally in a single SQLite file.

## Quick Start

### Option 1: Build from Source

```bash
# Build
cargo build --release

# Start the server
./target/release/selfsync-server

# Launch Chrome pointing to your server
google-chrome-stable --sync-url=http://127.0.0.1:8080
```

### Option 2: Docker Compose (Recommended)

```bash
docker compose up -d
```

One command. Data is automatically persisted to a Docker volume.

### Option 3: Docker

```bash
# Build the image
docker build -t selfsync .

# Run (data stored in ./data)
docker run -d -p 8080:8080 -v ./data:/data selfsync
```

### Start Syncing

1. Open Chrome with `--sync-url=http://127.0.0.1:8080`
2. Sign in with your Google account
3. Enable sync

Done. All your sync data now stays on your machine.

## Configuration

Environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `SELFSYNC_ADDR` | `127.0.0.1:8080` | Listen address |
| `SELFSYNC_DB` | `selfsync.db` | SQLite database path |
| `RUST_LOG` | `selfsync_server=info` | Log level |

When running via Docker, the database defaults to `/data/selfsync.db` and listens on `0.0.0.0:8080`.

## Multi-User Support (Optional)

By default, all data goes under a single anonymous user — perfectly fine for personal use.

For shared servers with multiple users, the server needs to know which Google account each sync request belongs to. Chrome does not send this information on its own, so selfsync uses an LD\_PRELOAD injector to intercept Chrome's sync traffic and tag each request with the user's email.

```bash
LD_PRELOAD=./target/release/libselfsync_payload.so google-chrome-stable
```

It hooks into Chrome at startup, reads the local profile data to figure out which Google account is active, and injects the corresponding email header into every sync request.

### Platform Support

| Platform | Single-user sync | Multi-user sync |
|----------|-----------------|-----------------|
| Linux | Yes | Yes (via LD\_PRELOAD) |
| macOS | Yes | Not yet |
| Windows | Yes | Not yet |
| iOS / Android | Not applicable | Not applicable |

**Why only Linux for multi-user?** Multi-user support requires injecting code into the Chrome process to intercept sync requests. On Linux this is done via `LD_PRELOAD`, a standard mechanism for hooking shared libraries. macOS and Windows have no direct equivalent — macOS has `DYLD_INSERT_LIBRARIES` but SIP blocks it for system-protected binaries, and Windows would require DLL injection techniques. Support for these platforms is planned but not yet implemented.

Single-user sync works on any platform — just launch Chrome with `--sync-url` and all data goes under the default anonymous user.

### Roadmap: Custom Chromium Build

We are planning to build a custom Chromium browser that natively sends user identity with sync requests. This would eliminate the need for LD\_PRELOAD hooking entirely — multi-user sync would work out of the box on all platforms without any injection.

## Things to Watch Out For

- **Do NOT include `/command/` in `--sync-url`**. Chrome appends it automatically. Just use `http://127.0.0.1:8080`.
- **Multi-user sync only works on Linux for now**. See [Platform Support](#platform-support) above.

## Building

Requires Rust 1.85+:

```bash
cargo build --release                        # Build everything
cargo build --release -p selfsync-server     # Server only
cargo build --release -p selfsync-payload    # Injector only
```

## Documentation

For implementation details, see the [docs/](docs/) directory:

- [architecture.md](docs/architecture.md) — Architecture and internals
- [account-mapping.md](docs/account-mapping.md) — Multi-user account mapping algorithm

## Prior Art

- Chromium `loopback_server.cc` — Reference sync server implementation

## License

[GPL-3.0](LICENSE)
