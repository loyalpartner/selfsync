<p align="center">
  <img src="lazycat/icon.png" alt="selfsync" width="120" />
</p>

<h1 align="center">selfsync</h1>

<p align="center"><em>自己的 Chrome / Edge 同步服务器。书签、密码、设置在自己的设备之间同步，数据只留在你自己的机器上。</em></p>

<p align="center">
  <a href="LICENSE"><img alt="License" src="https://img.shields.io/github/license/loyalpartner/selfsync"></a>
  <a href="https://github.com/loyalpartner/selfsync/releases"><img alt="Release" src="https://img.shields.io/github/v/release/loyalpartner/selfsync"></a>
  <a href="#懒猫一键安装"><img alt="Lazycat AppStore" src="https://img.shields.io/badge/Lazycat-cloud.lazycat.app.chromesync-orange"></a>
</p>

<p align="center"><a href="README.md">English</a> · <a href="README.zh-CN.md">中文</a></p>

## 功能

- 书签、密码、自动填充、历史、标签页、扩展和设置在 Chrome / Edge 之间同步。
- 数据只在你自己的硬件上。不需要 Google 账号，也不经过 Microsoft。
- 单个二进制 + 单个 SQLite 文件，复制一份即备份。
- 支持 Linux、macOS、Windows、Docker，或者懒猫一键安装。

## 安装

### Docker / Docker Compose

```bash
docker compose up -d
```

数据持久化到 Docker 命名卷。镜像发布在 `ghcr.io/loyalpartner/selfsync`。

### 预编译二进制

从 [GitHub Releases](https://github.com/loyalpartner/selfsync/releases) 下载对应平台的发布包——Linux（x86_64 / aarch64）、Windows、macOS（Intel / Apple Silicon）。解压后运行 `selfsync-server` 即可。

### 懒猫一键安装

家里有懒猫微服盒子的话，在自家 AppStore 搜「SelfSync」，或者直接打开 `https://appstore.<盒子名>.heiyu.space/#/shop/detail/cloud.lazycat.app.chromesync`（把 `<盒子名>` 换成你的设备名）。

> [!NOTE]
> selfsync 没有登录界面，能连到端口的东西都能读你的同步数据。请在家庭局域网、NAS 或家用环境里跑。想在外面访问就放在 Tailscale、WireGuard 或 Cloudflare Zero Trust 后面。

## 连接浏览器

启动浏览器时指向你的服务器，用任意账号登录、开启同步即可。本地数据会自动上传，不需要导出导入。

```bash
google-chrome-stable --sync-url=http://127.0.0.1:8080
microsoft-edge       --sync-url=http://127.0.0.1:8080
```

> **Edge 提示**：多人共用一个 selfsync 实例时数据会合并到一个账户（Edge 不会告诉服务器登录的是谁）。要让 Edge 多用户隔离就一人一个实例。Chrome 多用户在同一实例自动隔离。

### Linux 持久化参数

大多数 Linux 发行版包里的 Chromium / Chrome（Arch 的 `chromium`、AUR `google-chrome`、Manjaro 等）启动脚本会读取一个 flags 配置文件，省得每次启动都手动加 `--sync-url`。在对应文件里写一行即可：

- Chromium：`~/.config/chromium-flags.conf`
- Google Chrome：`~/.config/chrome-flags.conf`
- Microsoft Edge：`~/.config/microsoft-edge-flags.conf`
- Flatpak：上面同名文件，但路径在 `~/.var/app/<app-id>/config/` 下

```text
--sync-url=http://127.0.0.1:8080
```

重启浏览器生效。某些发行版的官方包（如 Debian 的 `google-chrome` deb）没有启动包装脚本，不会读这个文件——改用 `.desktop` 文件覆盖或自己写 wrapper 脚本。

### Android Chrome

设备用 ADB 连上后：

```bash
adb shell am set-debug-app --persistent com.android.chrome
adb shell 'echo "chrome --sync-url=http://<host>:8080" > /data/local/tmp/chrome-command-line'
adb shell am force-stop com.android.chrome
```

把 `<host>` 换成你的 selfsync 地址（局域网 IP、NAS 主机名、或者 Tailscale 名字）。Chrome 重启后打开 `chrome://sync-internals`，确认 `Sync Service URL` 已经变成你的地址即可。不用重新打包 APK。

## 配置

<!-- AUTO-GENERATED:cli-env -->
| 变量 | 默认值 | 说明 |
|------|--------|------|
| `SELFSYNC_ADDR` | `127.0.0.1:8080` | 监听地址 |
| `SELFSYNC_DB` | `selfsync.db` | SQLite 数据库路径 |
| `RUST_LOG` | `selfsync_server=info,http=info` | 日志过滤（tracing-subscriber 语法） |
<!-- /AUTO-GENERATED -->

Docker 镜像里默认值被覆盖为 `0.0.0.0:8080` 和 `/data/selfsync.db`。

## 开发

Rust 1.85+，`cargo build --release`。协议细节、HTTP 路由、数据库 schema、贡献说明都在 [docs/architecture.md](docs/architecture.md)。常见问题见 [docs/faq.md](docs/faq.md)。

## 许可

Copyright (C) 2026 Lee &lt;loyalpartner@163.com&gt;。 [GPL-3.0-or-later](LICENSE)。
