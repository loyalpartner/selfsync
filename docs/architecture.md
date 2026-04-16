# 架构与技术细节

## 整体架构

```
Chrome ──(LD_PRELOAD)──> 本地代理 ──(X-Sync-User-Email)──> selfsync-server ──> SQLite
```

## 项目结构

```
selfsync/
├── crates/
│   ├── sync-server/     # Chrome 同步服务器
│   │   ├── proto/       # 92 个 Chromium .proto 协议文件
│   │   ├── build.rs     # prost-build 编译 proto
│   │   └── src/
│   │       ├── main.rs      # axum 服务入口
│   │       ├── proto.rs     # 生成的 protobuf 类型
│   │       ├── auth.rs      # X-Sync-User-Email 中间件
│   │       ├── progress.rs  # 进度 token 编解码
│   │       ├── db/          # 数据库层 (sea-orm + SQLite)
│   │       └── handler/     # 请求处理 (commit, get_updates)
│   ├── nigori/          # Nigori 加密库
│   │   └── src/
│   │       ├── lib.rs       # Nigori: encrypt/decrypt/get_key_name
│   │       ├── keys.rs      # PBKDF2 / Scrypt 密钥派生
│   │       ├── stream.rs    # NigoriStream 二进制序列化
│   │       └── error.rs     # 错误类型
│   └── payload/         # LD_PRELOAD 注入器
│       └── src/
│           ├── lib.rs       # __libc_start_main hook, argv 注入
│           ├── mapping.rs   # cache_guid → email 映射
│           └── proxy.rs     # HTTP 代理, 添加用户 header
└── docs/
    ├── architecture.md      # 本文件
    └── account-mapping.md   # 账号映射算法
```

## 同步服务器

### API 端点

| 路由 | 方法 | 说明 |
|------|------|------|
| `/command/` | POST | Chrome Sync 协议 (protobuf) |
| `/chrome-sync/command/` | POST | 同上（备选路径） |
| `/` | GET | 用户列表 (HTML) |

### 存储

- SQLite + WAL 模式，单文件，零配置
- `sync_entities` 单表存储所有同步实体
- 每用户单调递增版本号，用于冲突检测

### 用户初始化

首次同步时自动创建：
- Nigori 加密节点（keystore passphrase）
- 书签根文件夹（书签栏、其他书签、移动书签、阅读清单）

### 认证

- 读取 `X-Sync-User-Email` 请求头识别用户
- 无 payload 时，默认用户为 `anonymous@localhost`（单用户够用）
- 有 payload 时，代理自动注入邮箱 header，支持多用户

## LD_PRELOAD Payload

1. Hook `__libc_start_main`，检测 Chrome 主进程（跳过子进程和非 Chrome 二进制）
2. 读取 `--user-data-dir`，扫描所有 Profile 的 Preferences 文件
3. 构建 `cache_guid → email` 映射表（算法见 [account-mapping.md](account-mapping.md)）
4. 启动本地 HTTP 代理（动态端口），注入 `--sync-url` 指向代理
5. 代理从 URL 的 `client_id` 参数查找邮箱，添加 `X-Sync-User-Email` header 后转发

## Nigori 加密库

Rust 实现的 [Nigori 协议](https://www.cl.cam.ac.uk/~drt24/nigori/nigori-overview.pdf)，兼容 Chromium 同步加密：

- AES-128-CBC 加密 + HMAC-SHA256 认证
- 支持 PBKDF2 和 Scrypt 密钥派生
- 已通过 Chromium 和 [go-nigori](https://github.com/nicktcortes/nicktcortes) 测试向量验证

## Chrome Sync 协议注意事项

- `--sync-url=http://host:port` — Chrome 自动追加 `/command/`，URL 里不要带
- `ClientToServerResponse.error_code` 必须显式设为 `SUCCESS (0)`，proto 默认值是 `UNKNOWN`，Chrome 会当作错误
- `NigoriSpecifics.passphrase_type`：`KEYSTORE_PASSPHRASE = 2`、`CUSTOM_PASSPHRASE = 4`，值错了会报 "Needs passphrase"
- Chrome 会在本地缓存 Nigori 状态，服务端数据库重置后需要用全新 Profile 测试
- 重置数据库后必须用 `--user-data-dir=/tmp/test` 启动新 Profile

## Chromium 源码参考

`~/modous/chromium/src/` 中的关键路径：

- `components/sync/base/sync_util.cc` — `GetSyncServiceURL()`，读取 `--sync-url`
- `components/sync/engine/sync_manager_impl.cc` — `MakeConnectionURL()`，追加 `/command/`
- `components/sync/engine/net/url_translator.cc` — `AppendSyncQueryString()`，追加 `client` 和 `client_id` 参数
- `components/sync/protocol/sync.proto` — `ClientToServerMessage`、`ClientToServerResponse`
- `components/sync/engine/loopback_server/loopback_server.cc` — 参考同步服务器实现
