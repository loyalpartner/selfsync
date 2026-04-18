# selfsync

[中文文档](README.zh-CN.md)

Self-hosted Chrome Sync server. Keep your bookmarks, passwords, preferences, and other browser data in sync across devices — without sending anything to Google.

## How It Works

Chrome natively supports syncing to a custom server via the `--sync-url` flag. selfsync implements the Chrome Sync protocol and stores everything locally in a single SQLite file. Multi-user support works out of the box — Chrome sends the signed-in account email with every sync request.

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

### Migrating Existing Data

Already syncing to Google and want to bring your data over? Nothing special to do:

1. Launch the browser with `--sync-url=http://127.0.0.1:8080`
2. Click your avatar → turn sync on (or off and back on if it's already enabled)

Chrome uploads its local cache of bookmarks, passwords, and settings to your selfsync server. No export/import needed.

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `SELFSYNC_ADDR` | `127.0.0.1:8080` | Server listen address |
| `SELFSYNC_DB` | `selfsync.db` | SQLite database path |
| `RUST_LOG` | `selfsync_server=info` | Log level |

When running via Docker, the database defaults to `/data/selfsync.db` and listens on `0.0.0.0:8080`.

## Multi-User

Multi-user works automatically. Chrome includes the signed-in Google account email in every sync request (via the protobuf `share` field). The server uses this to create separate data stores per user — no additional configuration needed.

## Things to Watch Out For

- **Do NOT include `/command/` in `--sync-url`**. Chrome appends it automatically. Just use `http://127.0.0.1:8080`.
- **After resetting the server database**, use a fresh Chrome profile (`--user-data-dir=/tmp/test`) to avoid stale sync state.

## Building

Requires Rust 1.85+:

```bash
cargo build --release                        # Build everything
cargo build --release -p selfsync-server     # Server only
cargo test                                   # Run tests
```

## Prior Art

- Chromium `loopback_server.cc` — Reference sync server implementation

## License

Copyright (C) 2026 Lee &lt;loyalpartner@163.com&gt;

Licensed under [GPL-3.0-or-later](LICENSE).
