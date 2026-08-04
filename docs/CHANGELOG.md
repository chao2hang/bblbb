# BBLBB — 文档变更记录

## v0.5 — 2026-08-04

### 需求问答确认

- 产品定位确认为公开兴趣社区，并冻结匿名访问、邮箱验证、冷静期、强制 2FA、无私信和审核公开策略。
- 冻结内容可见性、B币非现金属性、内部商城、共享附件容量、AI 逐次同意、第三方应用额度市场和数据保留规则。
- 已确认扩展能力全部纳入 v1.0 目标，以内部里程碑、Feature Flag 和专项发布门槛控制启用，不再标记为 v1.1/v1.2。
- 新增 `PRODUCT-DECISIONS.md` 和 `CRAWLER-POLICY.md`，明确产品所有者决策以及搜索索引、AI 爬虫和批量访问策略。
- 产品所有者于 2026-08-04 确认冻结 v0.5，作为生产工程开发依据；生产工程实现尚未开始。

## v0.4 — 2026-08-04

### 新增能力

- Marketplace Offer、Checkout Intent、托管确认、双边站内账本、商户待结算余额、退款和 Webhook。
- 下载抵扣的策略优先级、免费授权、原子扣费和 S3 URL 重签语义。
- AI Gateway、Provider、独立同意、Task、Suggestion、预算和故障降级。
- 核心 Video Service 与 Direct/HLS/Xigua Provider Adapter、手动 URL、CSP 和 SSRF/HLS 防护。
- 内部积分商城、装扮槽位、签到任务、社区 Reaction、库存和活跃反刷规则。

### 基线补档

- 新增文档状态与功能发布矩阵。
- 新增统一术语、状态机、错误码和 Endpoint 权限矩阵。
- 新增资源 DTO 契约、Marketplace 账务决策。
- 新增配置、领域事件、数据保留和隐私矩阵。
- 统一基础文档版本为 v0.4。
- 统一 Post 的 `closed_at` 语义、Sanction 枚举、Video 管理 API 和 AI/Video 异步 Task 语义。

### 兼容说明

v0.4 仍处于首次正式开发前，没有已发布的稳定 v1 API 客户端，因此上述统一不构成生产破坏性变更。从 v1.0 发布后，删除字段、改变枚举语义、改变账务或权限规则必须按 `API.md` 的废弃周期处理。
