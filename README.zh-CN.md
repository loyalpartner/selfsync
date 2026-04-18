# selfsync

[English](README.md)

自托管的 Chrome 同步服务器。把书签、密码、设置等浏览器数据同步到自己的机器上，不经过 Google。

## 工作原理

Chrome 本身就支持把同步数据发到自定义服务器（`--sync-url` 参数）。selfsync 实现了 Chrome 的同步协议，用一个 SQLite 文件把数据存在本地。多用户天然支持——Chrome 每次同步请求都会带上登录账号的邮箱。

## 快速开始

### 方式一：源码编译

```bash
# 编译
cargo build --release

# 启动服务器
./target/release/selfsync-server

# 打开 Chrome，指向你的服务器
google-chrome-stable --sync-url=http://127.0.0.1:8080
```

### 方式二：Docker Compose（推荐）

```bash
docker compose up -d
```

一条命令搞定，数据自动持久化。

### 方式三：Docker

```bash
# 构建镜像
docker build -t selfsync .

# 运行（数据保存在 ./data 目录）
docker run -d -p 8080:8080 -v ./data:/data selfsync
```

### 开始同步

1. 打开 Chrome（记得加 `--sync-url=http://127.0.0.1:8080`）
2. 登录 Google 账号
3. 开启同步

搞定。你的同步数据现在全部存在本地了。

### 迁移已有的数据

已经在用 Google 同步、想把数据迁过来？什么都不用做：

1. 启动浏览器时加上 `--sync-url=http://127.0.0.1:8080`
2. 点头像 → 开启同步（如果已开启，关掉再开一次）

Chrome 会把本地缓存的书签、密码、设置全部上传到你的 selfsync 服务器，不需要导出导入。

## 配置

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `SELFSYNC_ADDR` | `127.0.0.1:8080` | 监听地址 |
| `SELFSYNC_DB` | `selfsync.db` | 数据库文件路径 |
| `RUST_LOG` | `selfsync_server=info` | 日志级别 |

Docker 方式下，数据库默认在 `/data/selfsync.db`，监听 `0.0.0.0:8080`。

## 多用户

多用户自动支持。Chrome 在每个同步请求的 protobuf `share` 字段里会带上当前登录的 Google 账号邮箱，服务器据此为每个用户创建独立的数据空间，无需额外配置。

## 注意事项

- **`--sync-url` 不要带 `/command/`**。Chrome 会自己追加，写 `http://127.0.0.1:8080` 就行。
- **重置服务器数据库后**，需要用全新 Chrome Profile 测试（`--user-data-dir=/tmp/test`），避免本地缓存的旧状态冲突。

## 编译

需要 Rust 1.85+：

```bash
cargo build --release                        # 全部编译
cargo build --release -p selfsync-server     # 只编译服务器
cargo test                                   # 运行测试
```

## 参考

- Chromium `loopback_server.cc` — Chrome 内置的参考同步服务器实现

## 许可证

[GPL-3.0](LICENSE)
