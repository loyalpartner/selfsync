# 架构与技术细节

## 整体架构

```
Chrome ──(--sync-url)──> selfsync-server ──> SQLite
                              ↑
                    msg.share = 用户邮箱
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
│   │       ├── auth.rs      # 默认邮箱常量
│   │       ├── progress.rs  # 进度 token 编解码
│   │       ├── util.rs      # 共享工具 (gen_id, now_millis 等)
│   │       ├── db/          # 数据库层 (sea-orm + SQLite)
│   │       └── handler/     # 请求处理
│   │           ├── sync.rs      # POST /command/ 分发
│   │           ├── commit.rs    # COMMIT: 创建/更新实体
│   │           ├── get_updates.rs # GET_UPDATES: 按版本查询
│   │           ├── init.rs      # 用户初始化 (Nigori + 书签)
│   │           └── users.rs     # GET / 用户列表页
│   └── nigori/          # Nigori 加密库
│       └── src/
│           ├── lib.rs       # Nigori: encrypt/decrypt/get_key_name
│           ├── keys.rs      # PBKDF2 / Scrypt 密钥派生
│           ├── stream.rs    # NigoriStream 二进制序列化
│           └── error.rs     # 错误类型
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
- 书签根文件夹（书签栏、其他书签、移动书签）

### 认证

Chrome 在每个同步请求的 `ClientToServerMessage.share` 字段中发送登录账号的邮箱。服务器直接读取该字段识别用户，无需额外认证机制。`share` 为空时默认为 `anonymous@localhost`。

## Nigori 加密库

Rust 实现的 [Nigori 协议](https://www.cl.cam.ac.uk/~drt24/nigori/nigori-overview.pdf)，兼容 Chromium 同步加密：

- AES-128-CBC 加密 + HMAC-SHA256 认证
- 支持 PBKDF2 和 Scrypt 密钥派生
- 已通过 Chromium 测试向量验证

## Chrome Sync 协议注意事项

- `--sync-url=http://host:port` — Chrome 自动追加 `/command/`，URL 里不要带
- `ClientToServerMessage.share` 包含用户邮箱——无需外部认证
- `ClientToServerResponse.error_code` 必须显式设为 `SUCCESS (0)`，proto 默认值是 `UNKNOWN`
- `NigoriSpecifics.passphrase_type`：`KEYSTORE_PASSPHRASE = 2`、`CUSTOM_PASSPHRASE = 4`
- Chrome 会在本地缓存 Nigori 状态，服务端数据库重置后需要用全新 Profile 测试

## Chromium 源码参考

`~/modous/chromium/src/` 中的关键路径：

- `components/sync/base/sync_util.cc` — `GetSyncServiceURL()`，读取 `--sync-url`
- `components/sync/engine/sync_manager_impl.cc` — `MakeConnectionURL()`，追加 `/command/`
- `components/sync/engine/net/url_translator.cc` — `AppendSyncQueryString()`，追加参数
- `components/sync/protocol/sync.proto` — `ClientToServerMessage`、`ClientToServerResponse`
- `components/sync/engine/loopback_server/loopback_server.cc` — 参考同步服务器实现
