# 设计：新设备审核（Device Approval）

> 状态：已实现（`device.rs`、`admin/`）。对应 issue [loyalpartner/selfsync#13](https://github.com/loyalpartner/selfsync/issues/13)。

## 背景

selfsync 没有任何认证：只要能访问到同步端口，并且知道某个账号的邮箱，就能以这个账号的身份拉取和覆盖全部同步数据。
README 的建议是只在内网 / 私有网络里运行，但 #13 的提出者因为公司和家里之间的网络限制做不到，只能把服务放到公网。

Chrome 同步客户端不支持 OIDC、basic auth、mTLS（#13 里都试过，mTLS 会报 `ERR_BAD_SSL_CLIENT_AUTH_CERT`），所以没办法在反向代理层加认证。

#13 原本提的是一个只控制"能否新建用户"的开关 `SELFSYNC_ALLOW_NEW_REG`，它有个漏洞：攻击者只要用**已经存在的邮箱**发请求，就会命中已有用户，开关根本拦不住。本设计把粒度从"用户"细化到"设备"：

- 管理界面上有一个审核开关。
- 开关打开后，没见过的设备一律进入「待审核」，同步请求被拒绝。
- 管理员在界面上批准之后，这台设备才能正常同步。

这样即使对方知道你的邮箱，换一台没批准过的设备也读不到数据。

## 设备怎么识别

Chromium 发的每一个同步请求，URL 后面都带着设备的 cache_guid：

```
POST /command/?client=Google+Chrome&client_id=<cache_guid>
```

来源（Chromium main 分支）：

- `components/sync/engine/engine_components_factory_impl.cc::BuildConnectionManager()` 把 `cache_guid` 作为 `client_id` 传给 `MakeConnectionURL()`
- `components/sync/engine/net/url_translator.cc::AppendSyncQueryString()` 把它拼成 `client_id` 参数

为什么不用 protobuf 里的字段：

| 字段 | 问题 |
|---|---|
| `ClientToServerMessage.invalidator_client_id` | M117 起废弃，不再填 |
| `CommitMessage.cache_guid` | 只在 COMMIT 里有；新设备的第一个请求是 GET_UPDATES，拿不到 |
| `ClientToServerMessage.share` | Chromium 填的是邮箱；Edge 填的是 cache_guid，但两者语义不一致 |

所以设备 ID 统一从 URL 的 `client_id` 参数里取。现有的 `log_request` 中间件已经在日志里打出了 `query`，验证时直接看日志就行。

**待验证**：Edge 和 Brave 是否同样带 `client_id`。它们都基于 Chromium，大概率一样，但需要实测。

### cache_guid 的特性

- **会变**：在浏览器里退出账号、关闭再打开同步、或者换一个新的浏览器配置文件，都会生成新的 cache_guid，需要重新审批一次。界面上应该把这一点写清楚，并显示设备的首次出现时间，方便管理员认出"这是我刚重置过的那台"。
- **是客户端自己报的**：服务器没法验证它的真假。它的安全性来自"不可猜"（随机生成的长字符串），本质上相当于一个口令。如果反向代理把完整 URL 写进了访问日志，日志泄露就等于口令泄露。

## 数据模型

新增 `devices` 表（新迁移文件，不改已有的迁移）：

| 列 | 类型 | 说明 |
|---|---|---|
| `id` | INTEGER PK | |
| `cache_guid` | TEXT | URL 里的 `client_id` |
| `email` | TEXT | 请求里解析出来的邮箱（Edge 为 `anonymous@localhost`） |
| `browser_kind` | TEXT | `chromium` / `edge` |
| `status` | TEXT | `approved` / `pending` / `rejected` |
| `user_agent` | TEXT | 方便管理员辨认设备 |
| `remote_addr` | TEXT | 同上；在反向代理后面时取 `X-Forwarded-For` || `first_seen` | INTEGER | 毫秒时间戳 |
| `last_seen` | INTEGER | 毫秒时间戳 |

唯一键：`(cache_guid, email, browser_kind)`。

界面上显示的「Chrome 141 · Windows」不单独存，渲染时从 `user_agent` 解析。

### 能知道设备的哪些信息

同步请求的 User-Agent 不是网页那种，而是 Chromium 专门拼的（`components/sync/base/sync_util.cc::FormatUserAgentForSync()`）：

```
Chrome WIN 141.0.7390.54 (<commit>-refs/branch-heads/<n>) channel(stable)
```

- 系统只有大类：`WIN` / `MAC` / `LINUX` / `CROS` / `ANDROID-PHONE` / `ANDROID-TABLET` / `IOS-PHONE` / `IOS-TABLET`。**没有系统版本**，分不出 Windows 10 和 11。
- 浏览器版本是完整的；渠道（stable / beta / dev）也有。
- 产品名由编译参数决定，Brave、Vivaldi 这类大概率显示成 Chrome 或 Chromium，**分不出具体是哪个浏览器**（待实测）。Edge 的格式待实测。

已批准的设备还能拿到更多：设备同步上来的 `DeviceInfoSpecifics` 里有设备名（`client_name`，例如「Alice 的 MacBook Pro」）、厂商、型号、系统类型。但它要等设备开始正常同步后才会提交，**待审核的设备拿不到**，只能靠 User-Agent 和来源 IP 辨认。

开关状态存进新的 `settings` 表（`key TEXT PK, value TEXT`），键名 `device_approval`。不用环境变量，因为要能在界面上随时切换，不用重启。

## 请求处理流程

在 `handler/sync.rs::handle_command` 里，解析完身份之后、调用 `find_or_create_user` **之前**加一步设备判定：

```
解析 ClientIdentity + client_id
        │
        ▼
client_id / 邮箱格式不对 ─────► 返回拒绝响应（不写库）
        │
        ▼
查 devices 表
        │
        ├─ 已批准 ────────────────► 更新 last_seen，照常处理
        │
        ├─ 待审核 / 黑名单 ───────► 返回拒绝响应（不写库）
        │
        └─ 没见过
                │
                ├─ 已达数量上限 ──► 返回拒绝响应（不写库）
                ├─ 开关关闭 ──────► 记为 approved，照常处理
                └─ 开关打开 ──────► 记为 pending，返回拒绝响应
```

几个关键点：

1. **判定必须放在 `find_or_create_user` 之前**。否则待审核设备虽然被拒绝了，用户行和初始化数据（Nigori、书签根目录）已经建好了，#13 关心的"陌生人占用服务器资源"问题并没有解决。
2. **开关关闭时也要记录设备**，并自动标成 approved。这样升级后再打开开关，已经在用的设备不会被误拦。
3. **请求里没有 `client_id`**（老版本或非标准客户端）：开关打开时直接拒绝，开关关闭时照常处理但不记录。
4. **待审核和黑名单设备直接拒绝，不写库**。待审核设备不受开关影响：关掉开关不会把它们放进来，仍要管理员批准、拉黑或清空。代价是界面上看不到它们最近一次重试的时间。
5. **设备数量上限**（#13 的后续讨论）：已批准 + 待审核达到上限（`settings.device_limit`，默认 20，管理页可改，范围 1–1000）后，没见过的设备直接拒绝、不记录。黑名单不占名额，拉黑垃圾设备就能腾出位置。没有上限时，开着审核开关也能被人用随机 client_id 刷出无数条待审核记录。
6. **格式校验**：`client_id` 最长 64 个字符、只能是 base64 字符（Chromium 的 cache GUID 是 16 字节随机数的 base64，24 个字符）；邮箱最长 254。不合格的请求直接拒绝，不写库。

## 拒绝时返回什么

在 `ClientToServerResponse.error_code` 里返回 `TRANSIENT_ERROR`，HTTP 状态码仍然是 200。

各选项在客户端的效果（依据 Chromium `components/sync/engine/syncer_proto_util.cc` 和 `components/sync/service/sync_service_impl.cc`）：

| error_code | 客户端行为 | 结论 |
|---|---|---|
| `TRANSIENT_ERROR` | 按指数退避重试 | ✅ 采用。批准之后，下一次重试就能自动恢复同步，用户什么都不用做 |
| `THROTTLED` | 按服务器给的间隔暂停 | 可以用，但语义是"限流"，不贴切 |
| `DISABLED_BY_ADMIN` | 停止同步，**直到浏览器重启** | ❌ 批准后还得重启浏览器 |
| `NOT_MY_BIRTHDAY` | 在客户端关闭同步，并清掉本地的加密口令 | ❌ 破坏性太大 |
| HTTP 401 / 403 | 被当成认证错误，会触发重新拿 token | ❌ 行为不可控 |

**待验证**：退避的上限是多久。如果最长要等好几分钟，界面上要提示"批准后最多 N 分钟内生效"。

## 管理界面

### 🔴 管理界面必须和同步接口分开

现在的 `GET /` 只是一个只读的用户列表，和 `/command` 在同一个端口上，同样没有任何认证。
如果直接在这个页面上加"开关"和"批准"按钮，那么任何能访问同步端口的人都能自己给自己批准，整个设计就失效了。

方案：新增一个独立的监听地址，专门跑管理界面。

- 新增环境变量 `SELFSYNC_ADMIN_ADDR`，默认 `127.0.0.1:8081`，只在本机可访问。
- `SELFSYNC_ADDR` 上只保留同步接口和 `/healthz`。
- 管理端口需要远程访问时，由用户自己在反向代理上加认证（OIDC、basic auth 都行，因为访问者是人，不是同步客户端）。#13 的提出者已经这么做了。

这是一个**行为变更**：升级后 `GET /` 不再出现在同步端口上，需要写进 CHANGELOG 和 README。

### 页面内容

可交互原型：https://claude.ai/artifact/FHkgyb2hjTQc1c4VRTCh97

单页，从上到下：

1. **顶栏**：`selfsync 管理`，右侧标明管理端口和「仅本机可访问」，提醒管理员这个页面不该暴露出去；最右边是「中文 / English」语言切换。
2. **审核开关卡片**：标题「新设备需要审核」，一句话说明开和关分别意味着什么，下方显示当前状态（「已开启 · 新设备批准后才能同步」/「已关闭 · 新设备自动批准」）。开关是 `role="switch"` 的按钮。
3. **设备数量上限卡片**：数字输入框 +「保存」，下方显示「当前 已批准+待审核 / 上限」，到上限时变色。
4. **操作反馈**：开关切换、批准、拉黑之后，在上限卡片下方显示一行结果（`aria-live`），例如「已批准：Chrome 141 · Windows（alice@example.com）」。批准后再加一行恢复方法：等浏览器自动重试；想马上恢复就重启浏览器，或在 `chrome://sync-internals` 点「Trigger GetUpdates」；别关掉再打开同步（设备 ID 会变）。
5. **待审核**：标题带数量，旁边一个「全部清空」按钮。每台设备一行：
   - 主信息：浏览器和系统（从 User-Agent 解析，如 `Chrome 141 · Windows`）、账号邮箱
   - 次要信息：来源 IP、首次出现、设备 ID（只显示首尾几位）。待审核设备的重试不写库，所以没有「最近一次尝试」
   - 操作：「加入黑名单」「批准」
   - 区块标题旁写明「批准后，浏览器下次重试时自动恢复同步，一般几分钟内，不用重启浏览器」
   - 达到数量上限时，顶部加一条提示：新设备会被直接拒绝，要加设备先调高上限或拉黑不用的设备
   - 没有待审核设备时显示空状态
6. **已批准设备**：表格，列为设备、账号、来源 IP、首次出现、最近同步；每行一个「加入黑名单」按钮，即改成 rejected。刚批准的设备「最近同步」先显示「等待浏览器重试」。
   - 「设备」列两行：上面是设备名（如「Alice 的 MacBook Pro」「Google Pixel 8」），下面是浏览器和系统。
   - 设备名取自这台设备同步上来的 `DeviceInfoSpecifics.client_name`：在 `sync_entities` 里找 DEVICE_INFO 类型、`cache_guid` 等于该设备的那一条。
   - 还没同步过 DeviceInfo 的设备（刚批准的），上面一行显示浏览器和系统，下面一行写「设备名在首次同步后显示」。
7. **黑名单**（`rejected`）：有数据时才显示。每行两个按钮：「改为批准」直接放行；「移出黑名单」删除这条记录，下次连上来按新设备处理。
8. **账号**：原有用户列表，保留账号、浏览器，新增「已批准设备」数量，`next_version` 改叫「数据版本」；去掉对管理员没意义的 Store Birthday 列。
9. **页脚说明**：设备 ID 什么情况下会变，以及变了之后同一台电脑会以新设备身份重新出现在待审核里。

视觉上沿用现有页面的朴素风格：白底卡片、无图标装饰，设备 ID、IP、版本号用等宽字体。状态靠文字和数量徽标区分，不只靠颜色。窄屏下按钮换行，表格在卡片内横向滚动。

### 原型里没覆盖、实现时要定的

- 拉黑已批准设备要不要二次确认。现在是直接拉黑，因为可以从「黑名单」里一键改回。
- 设备多了以后要不要分页或搜索。个人自用场景下预计不超过几十台，第一版不做。
- 页面是否自动刷新。待审核设备是浏览器在后台重试时才出现的，第一版可以只靠手动刷新，或者加一个每 10 秒的 `<meta http-equiv="refresh">`。

### 管理接口

| 方法 | 路径 | 作用 |
|---|---|---|
| GET | `/` | 管理页面 |
| POST | `/settings/device-approval` | 切换开关 |
| POST | `/settings/device-limit` | 修改设备数量上限 |
| POST | `/devices/{id}/approve` | 批准 |
| POST | `/devices/{id}/reject` | 加入黑名单 |
| POST | `/devices/{id}/delete` | 移出黑名单（删除记录） |
| POST | `/devices/clear-pending` | 清空待审核 |

表单提交用 POST，提交后重定向回 `/`（303）。

管理端口只监听本机，也挡不住你自己浏览器里打开的恶意网页向 `127.0.0.1:8081` 提交表单，所以加了两道检查：

- **跨站 POST**：浏览器带 `Sec-Fetch-Site` 时只接受 `same-origin` / `none`；不带时比较 `Origin` 和 `Host`。两个头都没有（curl、脚本）放行。
- **DNS rebinding**：管理端口绑在回环地址上时，`Host` 必须是 `localhost` / `127.0.0.1` / `[::1]`。绑在其他地址（Docker、反向代理后）时不检查，由反向代理负责。

### 前端实现方式

**不用前端框架，服务端直接渲染 HTML。** 页面就是几个列表加几个按钮，用不着 React / Vue，也不值得为它在 Dockerfile 里加一套 Node 构建。

- **模板**：用 [askama](https://crates.io/crates/askama)。模板是普通的 `.html` 文件，编译期生成 Rust 代码、自动转义，模板写错了编译不过，运行时没有额外开销。替代现在 `handler/users.rs` 里手拼字符串 + `html_escape` 的写法。
- **交互**：每个按钮都是一个 `<form method="post">`，提交后服务器重定向回 `/`（Post/Redirect/Get）。不写 JavaScript 也能用全部功能。
- **样式**：一段内联 `<style>`，用系统字体（`system-ui, "PingFang SC", "Microsoft YaHei", sans-serif`），不加载 Google Fonts 等外部资源，内网或离线环境也能正常显示。原型里的 IBM Plex 只是演示用。
- **打包**：模板和 CSS 都编译进二进制，仍然是单个可执行文件，部署方式不变。

### 中英文

不引入 i18n 库，手写一个文案结构体，中英文各一份常量：

```rust
pub struct Texts {
    pub approval_title: &'static str,
    pub approve: &'static str,
    pub reject: &'static str,
    // …
}

pub const ZH: Texts = Texts { approval_title: "新设备需要审核", approve: "批准", reject: "拒绝", /* … */ };
pub const EN: Texts = Texts { approval_title: "Require approval for new devices", approve: "Approve", reject: "Reject", /* … */ };
```

好处是**漏翻一条编译就报错**，不会在页面上露出一个 key；文案总共几十条，用不着 `rust-i18n` 或 Fluent 这种带运行时的方案。模板里写 `{{ t.approve }}`。

- **带变量的句子**（「2 分钟前」/「2 minutes ago」、英文单复数）：写成 `fn ago(&self, n) -> String` 这样的方法，中英文各实现一份。
- **选哪种语言**，按优先级：
  1. URL 参数 `?lang=zh` / `?lang=en`（点顶栏切换时带上，同时写入 cookie）
  2. cookie `selfsync_lang`
  3. 浏览器的 `Accept-Language`，以 `zh` 开头就用中文
  4. 默认英文
- `<html lang>` 跟着当前语言变，屏幕阅读器才能正确朗读。
- 设备名、邮箱、User-Agent 这些数据原样显示，不翻译。

## 不做的事

- **不替代私有网络部署**。设备审核只能拦住"没批准过的设备"，挡不住 cache_guid 泄露、也不加密传输。README 里仍然推荐内网或 Tailscale 这类方式。
- **不做多管理员、不做操作审计**。
- **不做按用户的限额**（例如每个邮箱最多几台设备）。数量上限是全局的。

## 待验证清单

- [ ] Edge、Brave 的请求 URL 里是否都带 `client_id`
- [ ] `TRANSIENT_ERROR` 下客户端的退避上限，以及批准后实际多久恢复
- [ ] 待审核期间 `chrome://sync-internals` 里显示什么，方便写进 FAQ
- [ ] `chrome://sync-internals` 的「Trigger GetUpdates」能否让被拒设备立刻重试（批准后提示里写了）
- [ ] 拉黑一台已批准设备后，客户端的表现（是否会反复重试、是否影响本地数据）
