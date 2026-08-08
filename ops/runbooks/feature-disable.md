# Runbook：可选能力停用 / 回滚 / 历史数据保护（AI、Video、OIDC、Marketplace、Download Billing）

> 执行人：值班 + 产品/运维审批人（见 `oncall.md`）。
> 原则：停用停止**新**任务/新授权/新交易；**不删除**历史数据、**不撤销**已提交
> 账务、**不改变**已发布内容可见性（docs/CONFIGURATION.md §1）。

## 1. 紧急停用（kill switch，P0 应急）

```sh
# 所有可选能力（AI/Video/OIDC/Marketplace/DownloadBilling）强制关闭，
# 优先于一切配置（M01-CONFIG-05）
# /etc/bblbb/backend.env 追加：
BBLBB__FEATURE_KILL_SWITCH=true
# 重启 backend + worker
systemctl restart bblbb-backend bblbb-worker
# 确认 /readyz 正常且 Feature Flag 管理视图显示全部关闭
```

## 2. 单项停用

| 能力 | 配置/管理入口 | 关闭效果 |
|---|---|---|
| AI | 管理后台 AI 配置（`flags.ai`） | 停止新建议任务；进行中任务自然结束；历史建议按 version 保留 |
| Video | 管理后台 Video 配置（`flags.video`） | 停止新 resolve/创建；历史视频引用重检降级为外链卡片 |
| OIDC | 管理后台 OIDC 配置（`flags.oidc`） | 停止新 interaction/授权；本地登录不受影响；旧 Token 按撤销策略 |
| Marketplace | 管理后台 Client/紧急停用（`flags.marketplace`） | 停止新 Intent/Checkout；历史交易可查询；Webhook 停止新投递 |
| Download Billing | 管理后台 Download Billing 配置 | 停止新扣费授权；历史授权不撤销；未授权下载按免费策略 |

管理后台配置修改均要求 reason + recent-auth + 审计（M13-ADMIN）；版本化
policy 回滚走 `config_revisions`/policy version（M01-CONFIG-08）。

## 3. 历史数据保护

- 关闭能力**不删除**：任务 payload、suggestion、授权、Intent、订单、装扮、
  内容引用、账本流水全部保留；
- 重新开启时不产生追溯副作用（无重复扣款/重复授权——幂等键与账本不可变
  流水保证）；
- OIDC/Marketplace 关闭期间 JWKS/Client 配置保留，恢复后新旧密钥兼容。

## 4. 回滚与验证

```sh
# 撤销 kill switch（保留审计）
# 逐项重新开启走 M17-FLAGS 专项门槛流程（审批人/范围/观察指标/回滚命令）
systemctl restart bblbb-backend bblbb-worker
curl -fsS http://127.0.0.1:8080/readyz
ops/smoke/smoke.sh
```
