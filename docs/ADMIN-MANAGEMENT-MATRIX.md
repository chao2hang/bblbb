# ADMIN-MANAGEMENT-MATRIX — 后台管理页形态与批量操作矩阵

> 状态：2026-09-12 全量盘点（27 个 `/admin` 路由）。结论：列表型实体已接入前端表格与批量交互；后端原子批量端点和全量服务端导出仍未完成，当前批量动作由 SvelteKit server action 逐条调用单条端点，导出仅针对当前已加载页面数据。
>
> 约定（全后台统一）：
> - **表格形态**：`.app-card > .app-table`（或等价行布局：列头 + 数据行 + 操作列）；
> - **约定 B（批量）**：行复选框（+ 表头全选）→ `BatchBar` 工具条（`N 项已选`）→ 批量参数 Dialog → 服务端循环调用单条端点（每条各自 If-Match 乐观锁 + reason 审计）；
> - **约定 D（行写操作）**：每行一个「⋮」`RowActionsMenu`，菜单项触发 Dialog；
> - 写操作一律 reason 必填 + 审计（`audit_logs`）；私有数据 `private, no-store`。

## 矩阵

| 页面 | 形态 | 批量操作 | 批量端点（服务端逐条调用） | 判定说明 |
|---|---|---|---|---|
| /admin/users | 表格 | ✅ 批量设置状态（封禁/解封等） | 逐条 `PATCH /admin/users/{id}`（If-Match） | 用户列表为典型批量场景 |
| /admin/posts | 表格 | ✅ 批量审核（通过/拒绝等） | 逐条 `POST /admin/posts/{id}/action` | 审核队列批量处理 |
| /admin/attachments | 表格 | ✅ 批量删除 | 逐条 `DELETE /admin/attachments/{id}` | 存储治理批量清理 |
| /admin/notifications | 表格 | ✅ 批量召回广播 | 逐条 `POST /admin/notifications/{id}/recall` | 广播召回 |
| /admin/boards | 表格 | ✅ 批量操作（状态等） | 逐条单条端点 | — |
| /admin/tags | 表格 | ✅ 批量操作 | 逐条单条端点 | — |
| /admin/plugins | 表格 | ✅ 批量启用/停用 | 逐条 `POST /admin/plugins/{id}/enable|disable` | — |
| /admin/marketplace | 表格 | ✅ 批量操作 | 逐条单条端点 | — |
| /admin/oauth | 表格 | ✅ 批量操作 | 逐条单条端点 | — |
| /admin/achievements | 表格 | ✅ 批量操作 + 手工授予 | 逐条单条端点 / `POST /achievements/{code}/grant` | — |
| /admin/ai | 表格 | ✅ 批量操作 | 逐条单条端点 | — |
| /admin/assignments | 表格 | ✅ 批量委派 | 逐条角色分配端点 | 角色委派成员表 |
| /admin/moderation/cases | 表格 | ✅ 批量关闭 / 批量驳回（DangerConfirm + 原因） | 逐条案件状态更新端点（与详情页 transition 同端点） | 举报队列 |
| /admin/shop | 行布局列表（功能等价表格） | ✅ 批量上架 / 批量下售 | 逐条 publish/disable | 商品列表 |
| **/admin/levels** | 表格（TL 规则表 + 配额档列表） | ✅ **批量设置附件配额**（2026-09 本轮新增；单档冲突不阻塞，207 按档汇总） | 逐条 `PATCH /admin/levels/{id}/attachment-quota`（各档独立 If-Match=policy_version） | 配额档位批量统一；TL 规则逐级语义不同（TL0 无条件/TL4 manual_only），**不做规则批量** |
| /admin/audit | 表格 | ❌ | — | 审计日志 append-only，只读（改历史 = 破坏审计链） |
| /admin/points | 表格 | ❌ | — | 积分流水账本只读（调整走单条 `POST /admin/points/adjust`，禁止改流水） |
| /admin/download-billing | 表格 | ❌ | — | 扣费流水只读（此前已决策移除无端点支撑的死批量 UI） |
| /admin/content | 表格（T-REVIEW-TABLE） | ❌（逐帖审核链接流） | 逐帖审核页 | 待审队列以行内「审核」进入单帖对比页 |
| /admin/video | 卡片 + 只读队列 | ❌ | — | 转码队列为只读 mock（无写端点，按纪律不造死批量 UI）；白名单为站点设置字段（非实体列表） |
| /admin/themes | 预览卡 | ❌ | — | 主题预览卡即主题本身的样式展示，表格化破坏预览语义 |
| /admin/roles | 主从形态 | ❌（成员委派在 /admin/assignments 已有批量） | — | 角色列表 → 权限矩阵详情页（对齐原型），非平铺列表 |
| /admin/activity | 配置卡 + 只读概览 | ❌ | — | 签到为全局配置（单例），非列表实体 |
| /admin/settings | 配置表单 | ❌ | — | 站点设置单例 |
| /admin/storage | 配置表单 + 配额档 | ❌（配额档批量在 /admin/levels） | — | 存储配置单例 |
| /admin/bi | 指标看板 | ❌ | — | 指标快照，非可操作实体 |
| /admin | 仪表盘 | ❌ | — | 汇总卡 |

## 本轮变更（2026-09-12）

- `/admin/levels` 附件配额补齐约定 B：行复选框 + 全选 + `BatchBar` +「批量设置配额」Dialog（`?/batchQuota`）。服务端逐档 PATCH，**每档各自 If-Match=打开 Dialog 时快照的 policy_version**；单档 409 冲突不阻塞其余档，返回按档汇总（成功 TLx、冲突 TLy）。
- TL 信任规则（名称/摘要/条件）不做批量：各级语义互异（TL0 无条件、TL4 仅手动授予），逐级编辑 + 逐级 reset 已在行「⋮」菜单内。

## 端点登记

当前批量均为前端循环调用既有单条端点，不代表 `M18-ADMIN-BATCH-01` 的后端原子批量端点已经完成；服务端批量路由仍待实现。当前 ExportButton 也是浏览器端对已加载行的导出，不代表 `M18-ADMIN-BATCH-02` 的全量流式 CSV 端点已经完成。单条端点登记见 `scripts/check-route-coverage.rb` 与 `docs/API.md`。
