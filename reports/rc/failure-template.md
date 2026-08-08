# BBLBB — 失败报告模板（M16-RELEASE-TEST-03）

> 每条失败必须链接 operation/task、最小复现命令、日志 artifact 与负责人。
> 无此信息不关闭失败项。

## 模板

```markdown
# FAIL-<编号>：<一句话标题>

- **来源**：<PR/nightly/RC 层；workflow job 名>
- **任务/Operation**：<M16-XXX-YY / OpenAPI operationId / runbook 条目>
- **负责人**：<platform/team>（从 PERMISSION-MATRIX / oncall.md 认领）
- **状态**：open / fixing / verified-closed / accepted-risk（附理由）

## 复现命令（最小）

```sh
<完整可粘贴命令，含环境变量与前置条件>
```

## 日志 artifact

- CI：<workflow run URL / artifact 名>
- 本地：<相对路径或文件>（`.log` 入库或存 `/tmp` 并写 URL）

## 证据与影响

- 复现输出（错误摘要，不含 Secret）
- 影响面：<数据/权限/可用性/性能；是否 P0/P1>
- 回归测试：<新增或更新测试名>

## 关闭条件

- [ ] 复现命令在干净环境通过
- [ ] 门禁绿（check-roadmap / cargo / npm / 契约脚本）
- [ ] 负责人与复查日期签名
```

## 本里程碑失败项登记（已处理）

| ID | 来源 | 结果 | 处置 |
|---|---|---|---|
| FAIL-2026-08-08-01 | 发布冒烟附件 complete 403（macOS） | 环境因素：`/tmp` 为符号链接，触发本地存储适配器符号链接防护；改用非符号链接存储目录后 `SMOKE: PASS=14 FAIL=0` | 环境说明写入 release-test.md；非产品缺陷 |
| FAIL-2026-08-08-02 | release bundle 构建缺 `bblbb-migrate` 二进制 | 产物命名不一致：`src/bin/migrate.rs` 默认产物为 `migrate`，deploy 层统一引用 `bblbb-migrate` | 修复：Cargo.toml 显式声明 `[[bin]] name="bblbb-migrate"`，两个 drill 脚本同步改名；`PASS=26 FAIL=0` |
| FAIL-2026-08-08-03 | `check-openapi.rb` 基线 184≠193 | 契约增长未回填基线 | 修复：基线冻结为 193（与 check-roadmap 一致） |
| FAIL-2026-08-08-04 | `check-error-codes.rb` 13 个已登记码未进 OpenAPI 枚举 + 注册表与实现漂移 | M12/M13 起累计的注册表/实现漂移 | 修复：openapi Problem.code 同步为 106 码；backend 转换函数输出稳定码；check-code-fixtures.rb 四方一致 |
| FAIL-2026-08-08-05 | 性能：`GET /api/v1/posts`（无过滤）1M 行 p95=1207ms 超 500ms 慢查询阈值 | 缺复合索引（Schema 变更项） | 登记 thresholds.md + p0-p1.md；非 P0/P1；优化项 |
