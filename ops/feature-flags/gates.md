# BBLBB — 可选能力启用 Gate 记录（M17-FLAGS-01..08）

> 审批人：platform/product-operations；日期：2026-08-08。
> 原则：核心论坛 + 邮箱验证 + 审核 + 积分基础 + 本地附件以默认配置上线
> （FLAGS-01，无 Flag 门控）；可选能力（AI / Video / Download Billing / OIDC /
> Marketplace）v1.0 **默认关闭**（`backend/src/config/flags.rs`，`feature_for_path`
> 映射前缀，关闭时 409 feature_disabled）。

## 逐项启用记录模板（每次启用填写）

```text
能力：<name>
审批人：platform/product-operations（+ 领域 owner）
范围：<Provider/Client/Scope 粒度>
commit：<启用 commit>
时间：<UTC>
阈值/观察指标：<p95、错误率、429、busy、队列、账务差异>
回滚命令：<设置 flag 关闭 / kill-switch>
观察窗口：<24h/7d>
审计：<feature-flags 变更记录 + audit_logs>
```

## 五项 P2 的启用计划（默认关闭；对应实现/门槛已绿）

| P2 | FeatureName | 启用计划（gate 条件） | 状态 |
|---|---|---|---|
| M17-FLAGS-02 Download Billing | `DownloadBilling` | 策略/免费授权/URL 失败/Range/三库竞争门槛（M06-DOWNLOAD 套件绿）后按站点开关开启；观察重复扣费与 URL 失败率 | 默认关闭，计划就绪 |
| M17-FLAGS-03 AI | `Ai` | 逐次同意/脱敏/Provider/任务故障/迟到输出/预算门槛（M09 + M16-SECURITY-08 绿）后开启；观察外发同意率与 Provider 错误 | 默认关闭，计划就绪 |
| M17-FLAGS-04 Video | `Video` | Direct/HLS/Xigua 各自 SSRF/HLS/CSP/版权门槛（M10 套件绿）后按 Provider 开启；观察 MIME 欺骗/下架率 | 默认关闭，计划就绪 |
| M17-FLAGS-05 OIDC | `Oidc` | conformance/两个独立 RP/Refresh reuse/key rotation/恢复门槛（M11 实现绿；conformance/RP/恢复演练外部阻塞）后开启；观察 refresh 重用与签名密钥轮换 | 默认关闭，计划就绪（3 项外部门槛阻塞中） |
| M17-FLAGS-06 Marketplace | `Marketplace` | 账务/并发/退款/Webhook/对账/冻结门槛（M12 套件绿）后按 Client/Scope 开启；观察恒等式与对账差异 | 默认关闭，计划就绪 |

## 紧急关闭演练（FLAGS-08）

- kill-switch（`feature_kill_switch=true`）优先于 Flag 启用（tests/http.rs
  `kill_switch_blocks_even_enabled_features` 绿）。
- 各 FeatureName 关闭路径：`flags.set(<name>, false, ...)`；路由层
  `feature_gate` 返回 409；历史授权/订单/装扮/内容/账本保持可查询
  （M12 marketplace emergency disable + M01 kill-switch 套件绿）。

## 回滚命令

```sh
# 关闭某项能力（生产）
bblbb flags disable --name <Ai|Video|DownloadBilling|Oidc|Marketplace> --reason "<issue>"
# 或全局紧急关闭
BBLBB__FEATURE_KILL_SWITCH=true systemctl restart bblbb-backend
```
