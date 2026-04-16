# selfsync

[English](README.md)

自托管的 Chrome 同步服务器。把书签、密码、设置等浏览器数据同步到自己的机器上，不经过 Google。

## 工作原理

Chrome 本身就支持把同步数据发到自定义服务器（`--sync-url` 参数）。selfsync 实现了 Chrome 的同步协议，用一个 SQLite 文件把数据存在本地。

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

## 配置

通过环境变量配置：

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `SELFSYNC_ADDR` | `127.0.0.1:8080` | 监听地址 |
| `SELFSYNC_DB` | `selfsync.db` | 数据库文件路径 |
| `RUST_LOG` | `selfsync_server=info` | 日志级别 |

Docker 方式下，数据库默认在 `/data/selfsync.db`，监听 `0.0.0.0:8080`。

## 多用户支持（可选）

默认所有数据归到一个匿名用户，一个人用完全够了。

多人共用一台服务器时，服务器需要知道每个同步请求属于哪个 Google 账号。Chrome 本身不会发送这个信息，所以 selfsync 通过 LD\_PRELOAD 注入器劫持 Chrome 的同步流量，给每个请求打上用户邮箱标记。

```bash
LD_PRELOAD=./target/release/libselfsync_payload.so google-chrome-stable
```

注入器在 Chrome 启动时介入，读取本地配置文件识别当前登录的 Google 账号，然后在每个同步请求里注入对应的邮箱信息。

### 平台支持

| 平台 | 单用户同步 | 多用户同步 |
|------|-----------|-----------|
| Linux | 支持 | 支持（通过 LD\_PRELOAD） |
| macOS | 支持 | 暂不支持 |
| Windows | 支持 | 暂不支持 |
| iOS / Android | 不适用 | 不适用 |

**为什么多用户只支持 Linux？** 多用户需要往 Chrome 进程里注入代码来拦截同步请求。Linux 上可以用 `LD_PRELOAD` 这个标准机制来实现。macOS 虽然有类似的 `DYLD_INSERT_LIBRARIES`，但系统完整性保护（SIP）会阻止对受保护程序的注入；Windows 则需要 DLL 注入技术。这些平台的支持在规划中，但还没实现。

单用户同步在所有平台都能用——只要启动 Chrome 时加上 `--sync-url`，数据会归到默认的匿名用户下。

### 规划中：自编译 Chromium 浏览器

我们正在筹划编译一个定制的 Chromium 浏览器，让它在同步请求里直接带上用户身份信息。这样就完全不需要 LD\_PRELOAD 注入了——所有平台都能开箱即用地支持多用户同步。

## 注意事项

- **`--sync-url` 不要带 `/command/`**。Chrome 会自己追加，写 `http://127.0.0.1:8080` 就行。
- **多用户同步目前只支持 Linux**。详见上面的[平台支持](#平台支持)。

## 编译

需要 Rust 1.85+：

```bash
cargo build --release                        # 全部编译
cargo build --release -p selfsync-server     # 只编译服务器
cargo build --release -p selfsync-payload    # 只编译注入器
```

## 技术文档

实现细节见 [docs/](docs/) 目录：

- [architecture.md](docs/architecture.md) — 架构与技术细节
- [account-mapping.md](docs/account-mapping.md) — 多用户账号映射算法

## 参考

- Chromium `loopback_server.cc` — Chrome 内置的参考同步服务器实现

## 许可证

[GPL-3.0](LICENSE)
