# selfsync

[English](README.md)

自托管的 Chrome / Edge 同步服务器。把书签、密码、设置等浏览器数据同步到自己的机器上，不经过 Google 或 Microsoft。

> [!CAUTION]
> **不要把这个服务暴露到公网。** selfsync 没有做任何认证——只要能访问端口的人，就能读取甚至覆盖同步的全部数据，包括保存的密码。请仅在可信的私有环境下运行（局域网、NAS、家庭实验室），或者放在零信任隧道后面（Tailscale、Cloudflare Zero Trust、WireGuard）。

## 工作原理

Chromium 系浏览器原生支持把同步数据发到自定义服务器（`--sync-url` 参数）。selfsync 实现了 Chrome 的同步协议，用一个 SQLite 文件把数据存在本地。

每条用户记录使用复合键 `(email, browser_kind)`：Edge 上的 `alice@example.com` 和 Chrome 上的 `alice@example.com` 在数据库里是**两条独立**的用户行——因为两种浏览器的加密器不兼容、永久书签文件夹也不一样，强行共用一行会在每次提交时损坏同步状态。

## 浏览器支持

只测过 Chrome 和 Edge。其它 Chromium 系（Brave、Vivaldi、Arc、Opera 等）大概率能用——服务器会把所有没带 `X-AFS-ClientInfo: app=Microsoft Edge` 头的客户端按标准 Chromium 处理——但**未验证**。

| 浏览器 | 同步可用 | 多用户 | 备注 |
|---|---|---|---|
| Chrome | ✅ | ✅ 按邮箱 | 标准路径 |
| **Microsoft Edge** | ✅ | ❌ **仅单用户** | 见下文 |

### Edge 限制

Edge **不会在同步请求里发送登录账号的邮箱**。在 Chromium 把 Google 账号邮箱放进 protobuf `share` 字段的位置，Edge 放的是一个 base64 编码的设备 GUID。selfsync 没办法从那里恢复出真实账号，所以**所有 Edge 设备都会落到同一条共享的用户记录**（`anonymous@localhost`、`browser_kind=edge`）。

实际意味着：

- ✅ **同一个人的多台 Edge 设备** 之间能正常互相同步——共享记录正是想要的效果。
- ⚠️ **同一个 selfsync 实例上的多个 Edge 用户** 会共用一份合并后的数据集（书签、密码等会全部收敛到同一条记录里）。如果你需要用户隔离，请为每个用户跑一个独立的 selfsync 实例（不同端口 / DB 文件 / 容器）。
- ✅ **同一个人在 Edge 和 Chrome 上的账号** 会自动隔离——它们就是设计上互相独立的两条用户行（Nigori 加密方式不兼容）。

## 快速开始

### 方式一：Docker Compose（推荐）

```bash
docker compose up -d
```

数据自动持久化到 Docker volume。

### 方式二：Docker

```bash
docker build -t selfsync .
docker run -d -p 8080:8080 -v ./data:/data selfsync
```

### 方式三：源码编译

```bash
cargo build --release
./target/release/selfsync-server
```

### 把浏览器指向你的服务器

**Chrome / Chromium：**

```bash
google-chrome-stable --sync-url=http://127.0.0.1:8080
```

**Microsoft Edge：**

```bash
microsoft-edge --sync-url=http://127.0.0.1:8080
```

然后用任意账号登录、开启同步即可。浏览器会把本地的书签 / 密码 / 设置上传到 selfsync，不需要导出导入。

## 配置

<!-- AUTO-GENERATED:cli-env -->
| 变量 | 默认值 | 说明 |
|------|--------|------|
| `SELFSYNC_ADDR` | `127.0.0.1:8080` | 监听地址 |
| `SELFSYNC_DB` | `selfsync.db` | SQLite 数据库路径 |
| `RUST_LOG` | `selfsync_server=info,http=info` | 日志过滤（tracing-subscriber 语法） |
<!-- /AUTO-GENERATED -->

Docker 镜像里默认值被覆盖为 `0.0.0.0:8080` 和 `/data/selfsync.db`。

## 路由

<!-- AUTO-GENERATED:routes -->
| 路径 | 方法 | 用途 |
|---|---|---|
| `/` | GET | HTML 用户面板 |
| `/healthz` | GET | 存活检查（返回 `ok`） |
| `/command/`、`/command` | POST | Chrome 同步协议入口 |
| `/chrome-sync/command/`、`/chrome-sync/command` | POST | 备选路径（配合 `--sync-url=http://host:port/chrome-sync`） |
| `/v1/feeds/me/syncEntities[/command][/]` | POST | Edge 的同步路径变体 |
| `/sync/v1/diagnosticData/Diagnostic.SendCheckResult()[/]` | POST | Edge MSA 私有端点 stub（返回与真实 MSA 完全一致的 6 字节成功响应——`BookmarkDataTypeController` 初始化必需） |
| `/v1/diagnosticData/Diagnostic.SendCheckResult()[/]` | POST | 同上，备用前缀 |
<!-- /AUTO-GENERATED -->

## 注意事项

- **`--sync-url` 不要带 `/command/`**。浏览器会自己追加，写 `http://127.0.0.1:8080` 就行。
- **重置服务器数据库后**，需要用全新浏览器 Profile 测试（`--user-data-dir=/tmp/test`），避免本地缓存的旧状态冲突。
- **从 v0.1.1 升级**：直接换二进制即可。schema 迁移会自动跑——不需要删除 `selfsync.db`。

## 编译

需要 Rust 1.85+：

<!-- AUTO-GENERATED:cargo -->
| 命令 | 用途 |
|---|---|
| `cargo build --release` | 编译整个工作空间 |
| `cargo build --release -p selfsync-server` | 只编译服务器 |
| `cargo test` | 跑单元测试 |
| `cargo clippy --all-targets -- -D warnings` | 严格 lint 检查 |
<!-- /AUTO-GENERATED -->

## 架构

协议细节、浏览器特定的坑（Edge MSA Nigori、`workspace_bookmarks`）、数据库 schema、迁移机制等，参见 [docs/architecture.md](docs/architecture.md)。

## 参考

- Chromium `loopback_server.cc` — Chrome 内置的参考同步服务器实现

## 许可证

Copyright (C) 2026 Lee &lt;loyalpartner@163.com&gt;

[GPL-3.0-or-later](LICENSE)
