# BBLBB 微信小程序

BBLBB 社区论坛的微信小程序端（原生小程序，无构建步骤、无第三方依赖），
对接仓库 `backend/`（Rust/axum）的 `/api/v1` REST API。

- 设计语言与 `prototype/` 高保真原型一致（暖纸底 `#F5F3EE` / 墨色 / 珊瑚红 `#E45735`）；
- 全部 API 字段已对真实后端逐条验证（2026-09-04，debug 构建 + SQLite）；
- 认证走后端既有的 **会话 Cookie + CSRF** 机制（`M02-SESSION-07/08/09`），
  不引入 Bearer / 微信 code2session 新端点。

## 功能覆盖

| 模块 | 页面 | 对接端点 |
|---|---|---|
| 认证 | 登录（含 MFA 两步）/ 注册 / 找回密码 | `/auth/login`、`/auth/login/mfa`、`/auth/register`、`/auth/password-reset[+confirm]`、`/auth/csrf` |
| 会话 | 设备列表 / 逐设备撤销 / 全部登出 | `/auth/session`、`/auth/sessions[/{id}]` |
| 社区 | 首页（板块导航 + 最新/热门）/ 板块详情 / 搜索 | `/boards`、`/boards/{slug}`、`/boards/{slug}/posts`、`/posts`、`/search` |
| 帖子 | 详情（正文渲染/锁定/付费解锁）/ 发帖 / 收藏 / 点赞 | `/posts/{id}`、`POST /posts`、`/posts/{id}/favorite`、`/posts/{id}/reactions`、`/posts/{id}/unlock` |
| 评论 | 楼层列表 / 发表 / 回复 / 编辑 / 删除 | `/posts/{id}/comments`、`/comments/{id}` |
| 用户 | 公开主页 / 关注 / 编辑资料 | `/users/{username}`、`/users/{username}/follow`、`PATCH /me`（If-Match 乐观并发） |
| 活跃 | 每日签到 / 等级经验 / 积分明细 | `/activity/summary`、`/activity/visit`、`/me/point-transactions` |
| 成就 | 成就目录 + 已点亮 | `/achievements`、`/me/achievements` |
| 商城 | 商品 / 购买 / 装扮权益装备 | `/shop/products`、`/shop/orders`、`/me/entitlements[/{id}/equip\|unequip]`、`/me/presentation` |
| 其他 | 我的收藏 | `/me/favorites` |

## 目录结构

```
miniprogram/
├── project.config.json     # 开发者工具项目配置（appid 占位 touristappid）
├── app.json                # 页面注册 / tabbar / 窗口样式
├── app.js                  # 入口：会话恢复、登录守卫、统一错误提示
├── app.wxss                # 全局设计 token 与组件样式
├── config.js               # API_BASE / 手动 Cookie 开关 / 超时
├── sitemap.json
├── utils/
│   ├── request.js          # ★ 核心：wx.request 封装（CSRF/会话/错误归一化）
│   ├── cookie-jar.js       # 兜底 cookie 管理（config.MANUAL_COOKIES=true 时启用）
│   ├── api.js              # 领域 API 层（每个后端端点一个函数）
│   ├── store.js            # 本地登录态缓存 + 订阅
│   ├── html.js             # 服务端 HTML 净化（rich-text 纵深防御）
│   ├── format.js           # 时间/数字格式化
│   └── uuid.js             # 幂等键 client_request_id
├── components/
│   ├── post-card/          # 帖子列表卡片
│   ├── empty-state/        # 空/加载/错误三态
│   └── load-more/          # 上拉加载指示
├── pages/
│   ├── index/              # 首页（tab）
│   ├── board/  post/  create/  search/          # 社区
│   ├── login/  register/  forgot/               # 认证
│   ├── profile/  profile-edit/  sessions/  favorites/   # 个人
│   └── user/  shop/  achievements/  transactions/       # 社交/经济
└── dev/
    ├── start-backend.sh    # 小程序联调后端（独立端口/独立 SQLite）
    └── start-all.sh        # 一键：构建 + 后端 + worker
```

## 快速开始（本地联调）

前置：Rust 工具链、`sqlite3` CLI（构建/迁移）、[微信开发者工具](https://developers.weixin.qq.com/miniprogram/dev/devtools/download.html)。

```bash
# 1. 一键启动联调环境（后端 127.0.0.1:18181 + job worker，独立 SQLite）
bash miniprogram/dev/start-all.sh
# 只起后端：bash miniprogram/dev/start-backend.sh [port]

# 2. 微信开发者工具
#    导入 miniprogram/ 目录 → 详情 → 本地设置 →
#    勾选「不校验合法域名、web-view（业务域名）、TLS 版本以及 HTTPS 证书」

# 3. 对齐 API 地址
#    miniprogram/config.js → API_BASE = 'http://127.0.0.1:18181'
```

### 联调账号说明

后端默认**无生产 SMTP 客户端**（`email.deliver` Job 会进入重试直至死信），
因此本地环境收不到验证邮件。发帖/评论/签到等「内容写入」权限要求
`邮箱已验证 + 账号 active + 新号 24h 冷静期已过`（`M02-IDENTITY-09`、`AUTHZ-06`）。

本地联调激活一个刚注册的账号（一次性 SQL）：

```bash
DB=/tmp/bblbb-miniprogram.sqlite   # start-backend.sh 使用的库
sqlite3 "$DB" "
  UPDATE users SET
    status = 'active',
    email_verified = 1,
    email_verified_at = (strftime('%s','now') - 25*3600)*1000
  WHERE username_normalized = '你的用户名';
"
```

（把 `email_verified_at` 设到 25 小时前即可跳过 24 小时冷静期。）

## 后端对接要点（为什么这样写）

### 1. 会话 Cookie

- 后端会话 cookie 为 `__Host-bblbb_session`
  （HttpOnly / Secure / SameSite=Lax / Path=/，`M02-SESSION-02`）。
- 微信 `wx.request` 由运行时 cookie 引擎**自动**携带 `Set-Cookie` / `Cookie`，
  小程序 JS 无需（也读不到）HttpOnly cookie——这是默认路径。
- 个别基础库/真机若未自动携带，置 `config.MANUAL_COOKIES = true`：
  `utils/cookie-jar.js` 自行解析 `Set-Cookie` 并在后续请求显式发送 `Cookie` 头。

### 2. CSRF（写请求必须处理）

后端对所有 `POST/PUT/PATCH/DELETE` 分两档校验 `X-CSRF-Token`
（`backend/src/middleware/csrf.rs`）：

| 场景 | token 来源 |
|---|---|
| 未持会话的预认证写路径（login / register / verify-email / resend / password-reset 及其 confirm） | 先 `GET /api/v1/auth/csrf` → 服务端签发 `__Host-bblbb_csrf` cookie + 派生 token（`M02-SESSION-08` 防 login CSRF） |
| 已登录写请求 | 会话绑定的 synchronizer token，同样经 `GET /api/v1/auth/csrf` 获取（同一会话稳定） |

`utils/request.js` 对两条缓存分别管理，收到 `403 csrf_failed` 自动重取并重试一次。

### 3. 请求来源（Referer）

微信 `wx.request` 会自动携带
`Referer: https://servicewechat.com/{appid}/{page}/{version}`。
后端 `M02-SESSION-09` 的来源校验会把该 origin 与 `BBLBB__ALLOWED_ORIGINS`
或请求 `Host` 比对——因此**服务端必须放行**：

```
BBLBB__ALLOWED_ORIGINS=https://servicewechat.com[,其他来源]
```

未放行时写请求返回 `400 origin_not_allowed`（客户端会给出该提示）。
`dev/start-backend.sh` 已默认包含。

### 4. 其他约定

- 时间戳均为 **Unix 毫秒**；列表统一 `{items, page:{next_cursor, has_more}}`
  游标分页（`/me/favorites`、`/me/point-transactions`、`/conversations` 用
  `{items, next_cursor}`，`/auth/sessions` 为纯数组）；
- 写操作幂等键：`client_request_id`（帖子 16–200 字符，uuid 自动生成；
  商城用 `idempotency_key`）；
- 乐观并发：`PATCH /me`、`PATCH /posts/{id}`、`PATCH /comments/{id}` 需要
  `If-Match: <version>` 整数；
- 错误统一为 RFC 7807 Problem JSON（`code` 为稳定错误码），
  `utils/request.js` 映射为中文提示并处理 429 `Retry-After`；
- 正文为服务端渲染 HTML（`body_html`），经 `utils/html.js` 净化后交
  `<rich-text>` 渲染（样式在 `app.wxss` 的 `.rich-body` 作用域内）。

## 部署对接（生产）

1. **小程序 AppID**：`project.config.json` 的 `appid` 替换为真实 AppID。
2. **request 合法域名**：微信公众平台 → 开发管理 → 开发设置 → 服务器域名，
   将后端 HTTPS 域名加入 **request 合法域名**（`https://forum.example.com`）。
   生产必须 HTTPS（`__Host-` cookie 要求 Secure 连接）。
3. **`config.js`**：`API_BASE` 指向同一 HTTPS 域名。
4. **服务端配置**（与 Web 端同一后端实例即可）：
   `BBLBB__ALLOWED_ORIGINS` 追加 `https://servicewechat.com`
   （若 `BBLBB__ALLOWED_HOSTS` 为严格模式，保持包含站点域名即可，
   小程序请求的 `Host` 头就是该域名）。
5. **Worker**：生产按 `deploy/systemd/bblbb-worker.service` 运行 job worker
   （搜索索引、成就、签到任务等）。

## 已知限制 / 后端侧说明

- **微信原生登录（code2session）未接入**：后端当前无微信 OAuth 端点，
  小程序沿用「邮箱/用户名 + 密码」认证（含 MFA 两步）。如需 `wx.login()`
  静默登录，需要后端新增端点（openid 表 + 配置 appid/secret），
  属后端契约变更，未在本仓库范围内实施；`utils/auth` 分层已为此预留位置。
- **搜索**：`search.index` 依赖 job worker；且后端搜索索引要求帖子 slug 为
  ASCII（`SlugInvalid`），纯中文标题的帖子当前无法入索引——
  属后端问题（`backend/src/content/posts/service.rs generate_slug` 保留
  Unicode 字符，与 `backend/src/search/mod.rs` 的 slug 校验冲突），
  小程序侧按空结果优雅降级。
- **帖子编辑版本**：`GET /posts/{id}` 当前实现未返回 OpenAPI 声明的 `version`
  字段，客户端以「本地版本跟踪」（新建=1、每次成功编辑+1）提供 `If-Match`，
  冲突时提示刷新。
- **点赞状态**：`GET /posts/{id}` 不含「当前用户是否已赞」，
  客户端本地跟踪 toggle 状态；重新进入页面默认未赞。
- **内容风控（M05-RISK）**：发布时后端做风险评估，命中规则
  （敏感词 / 链接数 / 新用户发帖数 / 频率 / 7 天窗口内其他作者的重复正文指纹）
  → 帖子进入 `pending_review`（状态 draft，仅作者本人可见，子资源 404），
  审核通过后公开。发帖页已提示；小程序无需额外处理（作者本人可正常查看
  审核中帖子，他人访问按 404 提示「帖子不存在或已删除」）。
- **视频嵌入 / AI 辅助 / 下载计费 / OIDC / 外部市场**：对应能力受后端
  Feature Flag 控制（默认关闭，`409 feature_disabled`），小程序未做页面，
  后端开启后可在 `utils/api.js` 直接扩展。

## 验证状态

- 所有 JS 通过 `node --check` 语法校验；所有 JSON 可解析；
- 全部请求流程（CSRF 预认证 → 注册 → 登录 → 发帖 → 评论 → 点赞/收藏 →
  签到 → 搜索 → 商城）已在本地后端（127.0.0.1:18181，SQLite）用
  模拟小程序请求头（含 `Referer: https://servicewechat.com/...`）
  逐条 curl 验证通过，见 `dev/` 启动脚本与本文「联调账号说明」。
