<p align="center">
  <img src="lazycat/icon.png" alt="selfsync" width="120" />
</p>

<h1 align="center">selfsync</h1>

<p align="center"><em>Your own Chrome &amp; Edge sync server. Bookmarks, passwords, settings — synced between your devices, stored only on your machine.</em></p>

<p align="center">
  <a href="LICENSE"><img alt="License" src="https://img.shields.io/github/license/loyalpartner/selfsync"></a>
  <a href="https://github.com/loyalpartner/selfsync/releases"><img alt="Release" src="https://img.shields.io/github/v/release/loyalpartner/selfsync"></a>
  <a href="#lazycat-one-click"><img alt="Lazycat AppStore" src="https://img.shields.io/badge/Lazycat-cloud.lazycat.app.chromesync-orange"></a>
</p>

<p align="center"><a href="README.md">English</a> · <a href="README.zh-CN.md">中文</a></p>

## What you get

- Bookmarks, passwords, autofill, history, tabs, extensions and settings synced between your Chrome / Edge installs.
- Data stays on your hardware. No Google account, no Microsoft account.
- Single binary, single SQLite file. Back it up by copying one file.
- Runs on Linux, macOS, Windows, Docker, or one click on Lazycat.

## Install

### Docker / Docker Compose

```bash
docker compose up -d
```

Data persists in a named Docker volume. The image is published at `ghcr.io/loyalpartner/selfsync`.

### Pre-built binary

Grab a release for your platform from [GitHub Releases](https://github.com/loyalpartner/selfsync/releases) — Linux (x86_64 / aarch64), Windows, macOS (Intel / Apple Silicon). Unpack and run `selfsync-server`.

### Lazycat one-click

If you run a Lazycat box, search "SelfSync" in your AppStore, or open it directly at `https://appstore.<your-box>.heiyu.space/#/shop/detail/cloud.lazycat.app.chromesync` (replace `<your-box>` with your device name).

> [!NOTE]
> selfsync has no login screen — anything that can reach the port can read your synced data. Run it on your home LAN, NAS, or homelab. To access it from outside your network, put it behind Tailscale, WireGuard, or Cloudflare Zero Trust.

## Connect your browser

Launch your browser pointed at the server, sign in with any account, and turn sync on. No export/import needed — your local data uploads automatically.

```bash
google-chrome-stable --sync-url=http://127.0.0.1:8080
microsoft-edge       --sync-url=http://127.0.0.1:8080
```

> **Edge note**: if multiple people share one selfsync instance with Edge, their data merges into one profile (Edge doesn't tell the server which account is signed in). For separate Edge users, run a separate instance per person. Chrome users on the same instance are kept separate automatically.

### Android Chrome

With the device plugged in via ADB:

```bash
adb shell am set-debug-app --persistent com.android.chrome
adb shell 'echo "chrome --sync-url=http://<host>:8080" > /data/local/tmp/chrome-command-line'
adb shell am force-stop com.android.chrome
```

Replace `<host>` with your selfsync host (LAN IP, NAS hostname, or Tailscale name). After Chrome relaunches, open `chrome://sync-internals` and confirm `Sync Service URL` shows your address. No APK rebuild needed.

## Configuration

<!-- AUTO-GENERATED:cli-env -->
| Variable | Default | Description |
|----------|---------|-------------|
| `SELFSYNC_ADDR` | `127.0.0.1:8080` | TCP address to bind |
| `SELFSYNC_DB` | `selfsync.db` | SQLite database path |
| `RUST_LOG` | `selfsync_server=info,http=info` | Log filter (tracing-subscriber syntax) |
<!-- /AUTO-GENERATED -->

In the Docker image the defaults are overridden to `0.0.0.0:8080` and `/data/selfsync.db`.

## For developers

Rust 1.85+, `cargo build --release`. Protocol details, HTTP routes, schema, and contributor notes live in [docs/architecture.md](docs/architecture.md). Common gotchas in [docs/faq.md](docs/faq.md).

## License

Copyright (C) 2026 Lee &lt;loyalpartner@163.com&gt;. Licensed under [GPL-3.0-or-later](LICENSE).
