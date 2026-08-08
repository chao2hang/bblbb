# BBLBB — 数据保留、导出、注销与隐私矩阵

> 基线：v0.4。下表是默认策略；法律保留、调查冻结和用户请求之间的优先级必须写入审计。删除优先匿名化，禁止用删除历史账务破坏恒等式。

| 数据 | 默认保留 | 用户导出 | 注销处理 | 例外 |
|---|---|---|---|---|
| 用户资料 | 账户存续 + 注销延迟 30 天 | 是 | 匿名化/删除 | 法律保留 |
| Session/Token | 过期后 30 天 | 否 | 立即撤销，哈希延迟清理 | 安全调查 |
| 帖子/评论 | 业务存续 | 是 | 作者匿名化，内容按策略处理 | 审核/法律 |
| 附件对象 | 被引用期间 | 文件清单+导出 | 清理未引用对象；不因 URL 过期删除 | 法律保留 |
| Download Authorization | 账务保留 7 年 | 本人摘要 | 匿名化用户可识别字段 | 账务/争议 |
| Point Operation/Transaction | 7 年 | 本人流水 | 不删除，匿名化主体 | 财务/审计 |
| Marketplace Purchase/Refund | 7 年 | 买方和商户可见投影 | 不删除，匿名化个人 | 争议/对账 |
| Webhook Delivery | 180 天 | 不含 Secret | 删除 payload 中可识别字段 | 对账调查 |
| AI Consent | 同意版本存续 + 7 年审计摘要 | 是 | 撤回；保留同意证据最小字段 | 法律保留 |
| AI Task/Suggestion | 180 天 | 用户自己的建议 | 删除原文（默认本来不存），保留 hash/结果摘要 | 审计/安全 |
| Video Embed 元数据 | 引用期间 + 90 天 | 是 | 移除引用后删除或匿名化 | 版权申诉 |
| IP/UA/安全日志 | 90 天 | 否 | 延迟清理/聚合 | 安全调查 |
| 管理审计日志 | 2 年 | 仅按权限导出 | 不允许普通注销删除 | 法律/安全 |
| 举报与案件（reports/moderation_cases/case_reports/case_assignments） | 结案后 1 年 | 否 | 匿名化 reporter/被举报主体，保留去重键证据 | 法律/安全调查 |
| 内部备注（moderation_notes） | 随案件删除 | 否（不导出） | 随案件删除 | 调查冻结 |
| 审核动作与修订（moderation_actions/action_revisions） | 只追加保留 7 年 | 否 | 不删除，匿名化 actor | 审计/法律 |
| 处罚与撤销（sanctions/sanction_reversals） | 失效后 2 年 / 撤销后 90 天 | 本人可见投影 | 不删除，匿名化主体 | 争议/合规 |
| 申诉与决定（appeals/appeal_decisions） | 决定后 2 年 | 本人可见投影 | 匿名化，保留冲突声明证据 | 审计/争议 |
| 通知（notifications） | 90 天；安全通知 180 天 | 是 | 删除正文与资源引用，保留去重键至窗口结束 | 安全调查 |
| 通知偏好（notification_preferences） | 账户存续 | 是 | 随账户删除 | 无 |

## 1. 优先级

1. 法律保留/调查冻结。
2. 不可变账务和安全审计的最小必要记录。
3. 用户删除与导出请求。
4. 普通业务清理和统计降采样。

## 2. 导出

导出采用异步 Job，格式为领域 JSONL + 文件清单/hash；临时下载包使用私有对象和短期 URL。导出不得包含 Session、Secret、Provider Token、其他用户信息、平台签名视频 URL或内部管理员备注。

## 3. 第三方数据

AI Provider、视频来源、S3、SMTP 的外部请求必须按来源策略记录最小审计。完整 URL、Cookie、授权 Header、Prompt、响应正文和签名 URL 不进入普通日志。用户在 AI 完整内容外发前必须看到 Provider、用途、留存、训练和区域信息。

## 4. M15 运维衔接

- 命令级隐私生命周期流程见 `ops/runbooks/privacy-lifecycle.md`
  （数据导出、注销匿名化、30 天删除、法律保留、恢复误删）。
- 备份保留策略：`ops/backup/daily.sh` 默认保留 14 天；备份/恢复不影响
  legal_hold（`users.legal_hold_at` 在备份恢复后保留，30 天 hard delete
  对 legal hold 用户跳过）。
- 备份内容（`ops/backup/manifest.sh`）不含 Secret 值；OIDC 主密钥与数据库
  备份分离存储（`ops/backup/oidc-keys.md`）。
