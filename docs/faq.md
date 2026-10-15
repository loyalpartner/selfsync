# FAQ / 常见问题

## `--sync-url` 该怎么写？

写到主机和端口就行：`http://127.0.0.1:8080`。**不要带** `/command/`——浏览器会自己追加。带了反而走不通。

如果你部署在反向代理后面，确保代理把 POST body 完整透传到 selfsync，并且不要剥掉路径里的尾部斜杠。

## 重置数据库后浏览器同步不动了？

浏览器会在本地缓存 Nigori 加密状态，服务端数据库重置后，旧 Profile 里的客户端状态跟新建的服务器对不上。用一个全新的 Profile 测试就好：

```bash
google-chrome-stable --user-data-dir=/tmp/selfsync-test --sync-url=http://127.0.0.1:8080
```

确认新 Profile 同步正常后，再决定老 Profile 要不要重置（清掉本地同步数据）。

## 升级版本要不要删 `selfsync.db`？

不需要。schema 迁移在每次启动时自动跑（sea-orm-migration）。直接换二进制或者更新镜像即可，数据不会丢。

## Edge 多个用户的数据混在一起怎么办？

Edge 不会在协议层告诉服务器登录的是哪个账号——它发送的是 base64 编码的设备 GUID 而不是邮箱，所以同一个 selfsync 实例上的多个 Edge 用户会落到同一条记录里。

要分开就给每个 Edge 用户跑独立的实例：不同端口 / 不同 SQLite 文件 / 不同容器。Chrome 没这个问题，多用户在同一个实例上通过邮箱自动隔离。

详细的协议背景在 [architecture.md](architecture.md) 的 Edge 那一节。

## 端口被占用？想换端口？

通过 `SELFSYNC_ADDR` 改：

```bash
SELFSYNC_ADDR=0.0.0.0:9090 ./selfsync-server
```

Docker 用户改 `docker-compose.yml` 的 `ports` 映射就行，容器内默认还是 `0.0.0.0:8080`。

管理页的端口用 `SELFSYNC_ADMIN_ADDR` 改，默认 `127.0.0.1:8081`。

## 新设备一直同步不上？

如果管理页里开了「新设备需要审核」，新设备会停在「待审核」里，浏览器端看到的是同步报错并不断重试。去管理页点「批准」，浏览器下次重试就会恢复。

在浏览器里退出账号、关掉再打开同步、或换浏览器配置文件之后，设备 ID 会变，同一台电脑需要再批准一次。

## 想在外网访问怎么办？

selfsync 本身不做认证，**别直接开公网**。两种安全做法：

1. **零信任隧道**：Tailscale / Cloudflare Zero Trust / WireGuard。设备先入网再访问 selfsync。
2. **反向代理 + 鉴权**：Caddy / Nginx 前置加 HTTP Basic Auth、mTLS 或者 OIDC，再把请求转给 selfsync。但要注意 Chromium 的同步客户端不支持 Basic Auth；mTLS 是更靠谱的选项。
