# BBLBB — 测试策略与验收矩阵

> 版本：v0.3
> 本文定义编码后必须持续执行的测试；“支持 SQLite/MySQL/MariaDB、安全 OIDC、隐藏内容”必须由自动化和恢复演练证明。

## 1. 测试层级

| 层 | 目标 |
|---|---|
| 单元测试 | Domain policy、状态机、解析和纯函数 |
| 仓储契约测试 | 同一 repository 行为在三数据库一致 |
| 集成测试 | HTTP、事务、Session、任务和存储适配器 |
| 端到端 | 浏览器真实流程和无 JS 退化 |
| 安全测试 | 身份、权限、CSRF、泄漏、上传和 OIDC |
| 性能测试 | 小机器预算、SQLite 写竞争、SSR 和 worker |
| 运维演练 | 迁移、备份、恢复、回滚和密钥轮换 |

## 2. CI 矩阵

每个 PR：

```text
Rust: fmt, clippy -D warnings, test, cargo audit/deny
Frontend: format, lint, check, vitest, build
Database:
  SQLite 3.40+
  MySQL 8.0+
  MariaDB 10.11+
Integration: migrations up + repository contract + API smoke
Docs: markdown links, referenced files, terminology checks
```

定期/发布前：Playwright、axe、安全扫描、性能测试、OIDC conformance 和恢复演练。

## 3. 数据库迁移测试

- 空库应用全部迁移。
- 上一发布版本数据库升级到当前版本。
- migration checksum 修改会失败。
- 外键、唯一约束、枚举/应用校验在三数据库效果一致。
- MySQL 与 MariaDB 分开执行，不以一个通过代表另一个。
- SQLite 每连接启用 foreign keys/WAL/busy timeout。
- 迁移失败不会留下被错误标记为成功的版本。

## 4. 仓储契约

同一测试套件运行于每个引擎：

- 用户规范化唯一性。
- 角色与板块 assignment。
- 列表/详情可见性一致。
- 主题内楼层唯一和并发分配。
- 帖子版本冲突。
- 软删除/恢复。
- Cursor 分页不重不漏。
- Outbox 与业务事务原子性。
- 授权码一次消费。
- Refresh Token family 重用检测。

## 5. 权限测试

组合角色：

- 匿名。
- pending/active/restricted/banned 用户。
- member。
- 当前板块 moderator。
- 其他板块 moderator。
- 全局 moderator。
- administrator。

对象条件：

- 自己/他人内容。
- public/members/restricted/hidden 板块。
- draft/pending/published/hidden/deleted 内容。
- 锁定帖子。
- 有效/过期 assignment。
- mute/board mute/ban。
- 已/未解锁限制正文。

必须直接调用 API 验证拒绝，不能只看 UI 按钮。

## 6. 隐藏内容防泄漏

测试未授权用户无法从以下渠道取得 `restricted_html` 原始标记字符串：

- 帖子/回复详情 API。
- 列表、excerpt 和相关内容推荐。
- SSR HTML 与 hydration payload。
- RSS/Atom。
- sitemap、OpenGraph、JSON-LD。
- 搜索索引和高亮。
- 通知和邮件。
- 日志、tracing、错误响应和审计 metadata。
- 公共缓存/304。
- 附件下载。

付费解锁测试并发重复请求只扣一次并只创建一个 grant。

## 7. 积分测试

- 奖励、消费、冻结、解冻、转账、管理员调整和补偿。
- 不允许负余额时拒绝透支。
- 账本 `balance_after` 与账户一致。
- 历史流水不可修改。
- 相同幂等键相同请求返回原结果。
- 相同 key 不同请求返回冲突。
- SQLite 并发写、MySQL/MariaDB 行锁竞争。
- 模拟事务各步骤失败，保证无“余额变但流水没写”。
- 经验等级缓存可从账户重建。

可用属性测试验证所有账户：

```text
初始余额 + Σ(delta_balance) = 当前余额
初始冻结 + Σ(delta_frozen) = 当前冻结余额
```

## 8. Session 与 CSRF

- 登录成功/失败、账号锁定和统一错误。
- Session fixation：登录后 Token 变化。
- idle/absolute 过期。
- 登出、改密码、封禁、设备撤销。
- Cookie 属性：`__Host-`、Secure、HttpOnly、Path、SameSite。
- 无/错/其他 Session 的 CSRF token 被拒绝。
- 跨 Origin POST 被拒绝。
- GET 无副作用。
- 登录 CSRF 和 form action 代理的 Set-Cookie 传播。

## 9. OIDC

自动化：

- Discovery 与 JWKS。
- Public/Confidential Client。
- redirect 精确匹配和危险变体。
- PKCE 缺失、plain、错误 verifier。
- state Client 集成流程与 nonce claim。
- code 过期、重放、绑定错误 redirect/Client。
- ID Token `iss/sub/aud/exp/iat/auth_time/nonce/kid`。
- scope 和 consent。
- opaque Access Token 的 userinfo。
- Refresh Rotation、旧 token reuse 撤销 family。
- Client 禁用、用户封禁和 consent 撤销。
- key rotation 期间新旧 Token 校验。
- logout 与 post logout redirect。

发布 v1.1 前运行适用的 OpenID Foundation conformance profile，并保存报告。

## 10. 审核测试

- 举报去重和状态机。
- 板块版主范围。
- 自己案件的利益冲突阻断。
- 内容 hide/restore revision。
- mute、board mute、rate limit、ban 的实时生效与到期。
- 封禁撤销 Session/Refresh Token。
- 申诉接受创建撤销记录而不删历史。
- 举报者与内部备注不泄漏。

## 11. 文件与存储

- 本地/S3 adapter contract。
- 路径穿越、绝对路径、符号链接。
- MIME 欺骗、SVG、polyglot、图片炸弹和超尺寸。
- 中断上传、重复 complete 和 pending 清理。
- 私有/受限附件权限、Range、缓存。
- 图片元数据移除和缩略图。
- 孤儿 mark-and-sweep 不误删在用文件。
- 数据库与附件恢复后一致性。

## 12. 前端与可访问性

Playwright 流程：

- 匿名浏览文章/论坛。
- 注册、验证、登录、退出和 Session 管理。
- 发文章、发讨论、草稿、冲突提示、回复。
- 举报、审核、处罚和申诉。
- 积分/等级/解锁。
- 主题切换和默认 fallback。
- 管理后台高风险确认。

可访问性：

- axe 无严重/关键错误。
- 全流程键盘操作。
- 焦点管理、表单错误关联、对比度和减少动画。
- 无 JavaScript 时公开浏览和关键表单仍合理退化。

## 13. API 契约

- OpenAPI 生成与提交文件一致。
- problem+json 格式和稳定 code。
- 401/403/404 策略。
- Cursor 分页、未知参数、最大 limit。
- ETag/If-Match 版本冲突。
- 429 与 Retry-After。
- 隐私 DTO 与管理员 DTO 分离。
- 向后兼容测试使用上一版本生成客户端。

## 14. 任务与故障注入

- Outbox 与业务提交/回滚。
- Worker 崩溃、lease 到期和重复执行。
- SMTP/S3 超时、永久错误和 dead-letter。
- 幂等 handler。
- 优雅停机不领取新任务。
- SQLite busy 时退避而非高频自旋。
- 邮件任务不会把 token 写日志。

## 15. 性能预算

在明确环境记录：CPU、RAM、数据库、数据量、并发、命令和 commit。

SQLite 512MB 参考场景：

- 10 万用户、100 万帖子/回复级的合成数据（可分阶段建立）。
- 首页/文章/板块 SSR。
- 登录、发帖、回复。
- 积分并发。
- worker 处理邮件/缩略图时 HTTP 延迟。

验收使用 SLO，而非无依据 QPS，例如：

- 公开文章 p95 服务端响应目标。
- 登录和发帖 p95。
- 峰值 RSS 不超过部署预算并留系统余量。
- 无持续 SQLite busy 错误。

数值在第一次基准测试后写入版本化性能基线。

## 16. 备份恢复与升级演练

每次发布候选至少：

- 从上一版本数据执行迁移。
- 验证兼容回滚路径或明确不可回滚。
- 恢复最近 SQLite 备份。
- 定期恢复 MySQL 和 MariaDB 备份。
- 恢复附件和 OIDC key 后验证旧 ID Token/JWKS。
- 校验账户与账本、附件 hash、授权 grant、迁移版本。

“备份命令成功”不等于恢复测试成功。

## 17. 发布门槛

v1.0：

- 三数据库迁移/契约绿。
- 核心权限、审核、Session、CSRF、内容泄漏测试绿。
- 默认主题 Playwright + axe 绿。
- SQLite 恢复演练成功。

v1.1 OIDC：

- conformance 适用 profile 通过。
- key rotation 和 Refresh reuse 测试通过。
- 与至少两个独立 RP 集成。
- OIDC 密钥恢复演练通过。
