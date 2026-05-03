# 架构与技术细节

## 整体架构

```
┌─ Chrome / Brave / Vivaldi ─┐
│                            │
│  --sync-url=http://...     ├──┐
│                            │  │      ┌──────────────────────┐      ┌─────────┐
└────────────────────────────┘  ├─POST─►   selfsync-server    │──────►  SQLite │
                                │      │                      │      └─────────┘
┌─ Microsoft Edge ───────────┐  │      │  axum + sea-orm      │
│                            │  │      │                      │
│  --sync-url=http://...     ├──┘      │  identity =          │
│                            │         │   (email, browser)   │
└────────────────────────────┘         └──────────────────────┘
```

`auth::ClientIdentity::from_request` 解析 `(email, browser_kind)`：
- `email` 来自 protobuf `share` 字段。Chromium 系发邮箱；Edge 发的是 base64 cache_guid，会 fallback 到 `anonymous@localhost`。
- `browser_kind` 来自 HTTP `X-AFS-ClientInfo` 头中的 `app=...` 字段。Edge 设 `app=Microsoft Edge`；Chromium 系不带这个头。

## 项目结构

```
selfsync/
├── crates/
│   ├── sync-server/     # Chrome / Edge 同步服务器
│   │   ├── proto/       # 92 个 Chromium .proto 协议文件
│   │   ├── build.rs     # prost-build 编译 proto
│   │   └── src/
│   │       ├── main.rs       # axum 服务入口、CLI、路由
│   │       ├── proto.rs      # 生成的 protobuf 类型
│   │       ├── auth.rs       # BrowserKind + ClientIdentity 解析
│   │       ├── progress.rs   # 进度 token 编解码
│   │       ├── util.rs       # 共享工具 (gen_id, now_millis 等)
│   │       ├── db/
│   │       │   ├── mod.rs       # 连接池、启动时跑 Migrator::up
│   │       │   ├── entity/      # sea-orm Model（user / sync_entity）
│   │       │   └── migrator/    # sea-orm-migration 迁移文件
│   │       └── handler/
│   │           ├── mod.rs       # log_request 中间件 + Diagnostic stub
│   │           ├── sync.rs      # POST /command/ 分发
│   │           ├── commit.rs    # COMMIT: 创建/更新实体
│   │           ├── get_updates.rs # GET_UPDATES: 按版本查询
│   │           ├── init.rs      # 用户初始化（Nigori + 永久书签文件夹）
│   │           └── users.rs     # GET / 用户列表页
│   └── nigori/          # Nigori 加密库
│       └── src/
│           ├── lib.rs       # Nigori: encrypt/decrypt/get_key_name
│           ├── keys.rs      # PBKDF2 / Scrypt 密钥派生
│           ├── stream.rs    # NigoriStream 二进制序列化
│           └── error.rs     # 错误类型
```

## 同步服务器

### 用户身份模型

`users` 表唯一键是复合 `(email, browser_kind)`。Edge 与 Chrome 上的同邮箱是两条独立的用户行，原因：

- **加密器不兼容**：Chromium 用服务器下发的 keystore key；Edge 用 MSA-managed keys，会拒绝服务器构造的 keybag。
- **永久书签文件夹不同**：Chromium 期望 4 个标准文件夹；Edge 多一个 `workspace_bookmarks`（"Workspaces"），缺失会触发 `kBookmarksInitialMergePermanentEntitiesMissing`（Type 64）。

### 用户初始化（一次性）

`handler/init.rs::initialize_user_data` 根据 `user.browser()` 选择初始化策略：

| 资源 | Chromium | Edge |
|---|---|---|
| Nigori | KEYSTORE_PASSPHRASE，服务器构造 keybag、PBKDF2 派生 | 空 IMPLICIT_PASSPHRASE，触发客户端用自己的 MSA keys 初始化 |
| 永久书签文件夹 | bookmark_bar / other_bookmarks / synced_bookmarks（+ root） | 上面 4 个 + workspace_bookmarks |
| store_birthday | 固定字符串 `ProductionEnvironmentDefinition`（与真实 MSA 一致，Edge 在初始合并时校验） | 同左 |

### 存储

- SQLite + WAL 模式，单文件，零配置
- `sync_entities` 单表存储所有同步实体；`(user_id, id_string)` 唯一
- 每用户单调递增版本号 `users.next_version`，用于乐观并发与 GET_UPDATES 进度 token

### Schema 迁移

由 `sea-orm-migration` 维护，`db/migrator/` 下每个迁移一个文件：

| 版本 | 内容 |
|---|---|
| `m20260101_000001_initial` | v0.1.1 schema：`users`（email UNIQUE）+ `sync_entities` + 索引 |
| `m20260425_000001_users_add_browser_kind` | 加 `browser_kind` 列；rebuild `users` 去掉 column-level UNIQUE；建复合唯一索引 `(email, browser_kind)`；旧用户 backfill 为 `chromium` |

启动时自动应用未应用的迁移，`seaql_migrations` 表追踪状态。**升级直接换二进制**，不需要删 db。

### 路由

| 路径 | 用途 |
|---|---|
| `/command/`、`/chrome-sync/command/` | Chromium 系标准入口 |
| `/v1/feeds/me/syncEntities[/command][/]` | Edge 同步入口（Edge 把 `--sync-url` 后面追加 `/command/`） |
| `/sync/v1/diagnosticData/Diagnostic.SendCheckResult()[/]`、`/v1/diagnosticData/Diagnostic.SendCheckResult()[/]` | Edge MSA 私有端点 stub。返回真实 MSA 的 6 字节 `0a 04 08 01 10 01` 成功响应。仅 BookmarkDataTypeController 在初始化时强依赖；其他 data type 不挂这个端点。 |

未匹配的请求由 axum 默认返回 405/404（在中间件 `log_request` 里被记录），便于早期发现新端点。

## Nigori 加密库

Rust 实现的 [Nigori 协议](https://www.cl.cam.ac.uk/~drt24/nigori/nigori-overview.pdf)，兼容 Chromium 同步加密：

- AES-128-CBC 加密 + HMAC-SHA256 认证
- 支持 PBKDF2 和 Scrypt 密钥派生
- 已通过 Chromium 测试向量验证

## Chrome / Edge Sync 协议要点

- `--sync-url=http://host:port` — 浏览器自动追加 `/command/`，URL 里不要带
- `ClientToServerMessage.share` —— Chromium 系是用户邮箱；Edge 是 base64 cache_guid（**Edge 单用户的根因**）
- `ClientToServerResponse.error_code` 必须显式设为 `SUCCESS (0)`，proto 默认值是 `UNKNOWN`，客户端会当成错误
- `ClientCommand` + `NewBagOfChips` 在每个响应都带，真实 MSA 也是这样做的
- `NigoriSpecifics.passphrase_type`：`KEYSTORE_PASSPHRASE = 2`、`CUSTOM_PASSPHRASE = 4`、`IMPLICIT_PASSPHRASE = 1`
- `NigoriSpecifics.encrypt_everything` 必须 `false`：selfsync 的 BookmarkSpecifics 是明文，设 true 是协议矛盾，会让 BookmarkDataTypeProcessor 在初始合并时挂掉
- 浏览器会在本地缓存 Nigori 状态，服务端数据库重置后需要用全新 Profile（`--user-data-dir=...`）

## Chromium 源码参考

`~/modous/chromium/src/` 中的关键路径：

- `components/sync/base/sync_util.cc::GetSyncServiceURL()` — 读取 `--sync-url`
- `components/sync/engine/sync_manager_impl.cc::MakeConnectionURL()` — 追加 `/command/`
- `components/sync/engine/net/url_translator.cc::AppendSyncQueryString()` — 追加 `client` / `client_id` 参数
- `components/sync/protocol/sync.proto` — `ClientToServerMessage` / `ClientToServerResponse`
- `components/sync/engine/loopback_server/loopback_server.cc` — 参考同步服务器实现
- `components/sync_bookmarks/bookmark_data_type_processor.cc::OnInitialUpdateReceived()` — Type 64 / `kBookmarksInitialMergePermanentEntitiesMissing` 的判定点
- `components/sync/nigori/nigori_sync_bridge_impl.cc::MergeFullSyncData()` — Edge 用空 IMPLICIT Nigori 触发的 client-init fall-through 路径

## HTTP 路由

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

尾部斜杠由 `NormalizePathLayer` 统一处理，浏览器追加的 `/command/` 也能正确路由到对应 handler。

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

## 参考

- Chromium `loopback_server.cc` — Chrome 内置的参考同步服务器实现
