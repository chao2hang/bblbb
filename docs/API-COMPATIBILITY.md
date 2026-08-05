# BBLBB — v1 兼容策略与弃用流程

> 基线：v0.4（2026-08-04 冻结）。适用范围：`openapi/openapi.yaml` v1.0.0 及其派生产物（coverage manifest、生成 TS 类型、错误码/权限/状态机注册表）。本文对应 M00-CONTRACT-11。

## 1. 版本承诺

- 业务 API 前缀 `/api/v1` 在 v1 生命周期内保持**向后兼容**。
- OIDC 标准端点（`/.well-known/*`、`/oauth/*`）、`/healthz`、`/readyz`、`/api/v1/openapi.json` 不进入业务版本承诺，但变更仍需评审并记录。
- `code`、`message_key`、`x-permission`、状态枚举、`operationId` 是客户端与测试依赖的稳定机器语义，禁止未经批准的语义复用或漂移。

## 2. v1 允许的变更（只增不破）

以下变更**可以**直接进入 v1，无需冻结批准：

1. 新增 operation（新 path+method、唯一 `operationId`）。
2. 新增可选请求字段、可选响应字段、可选响应头。
3. 新增 enum 值（不得复用、重命名或删除既有值）。
4. 新增问题码（必须同步 `docs/ERROR-CODES.md`、前端映射与测试向量）。
5. 新增 permission 值（必须同步 `docs/PERMISSION-MATRIX.md` 附录注册表）。
6. 扩大字段长度上限、放宽校验（不改变既有合法输入的结果）。
7. 澄清 description / example / `message_key` 文案（不改语义）。
8. 修复不影响语义的拼写错误。

任何允许的变更都必须：更新 `openapi/openapi.yaml`、重跑
`ruby scripts/sync-operation-coverage.rb`、重跑全部 check 脚本、重新生成
v1 TS 类型并保持 `--check` 可复现。

## 3. v1 禁止的变更（破坏性，必须进入 v2 或取得冻结批准）

1. 删除、重命名 operation（`operationId` 或 method/path）。
2. 删除、重命名、改变已有字段/枚举值/错误码/权限的语义。
3. 把可选字段变必填；把已有响应字段改为省略、改名或改变类型。
4. 改变既有输入在成功时的状态码、响应结构或幂等/并发语义。
5. 收紧权限、安全要求或 `x-csrf`/security 组合而不提供兼容过渡。
6. 改变列表分页形状、游标语义或排序默认行为。
7. 对已发布字段引入前后不一致的 `nullable`/默认值变化。

## 4. 弃用流程（v1 内退役一个 operation 或字段）

1. 在 OpenAPI 上标记 `deprecated: true`，在 summary 注明替代 operation 与计划移除版本。
2. 在 coverage manifest 中记录 `deprecated` 状态与移除版本；保留 `contract_status: frozen`。
3. 客户端与文档同步标记；生成 TS 类型保留字段/操作（生成器对 deprecated 成员输出注释）。
4. 弃用保留期不少于 2 个发布周期；到期后只能通过 v2 移除，不得在 v1 内就地删除。
5. 移除前必须在 `docs/CHANGELOG.md` 记录，并在 API 文档中给出迁移指引。

## 5. 冻结变更批准

- v1 契约的任何破坏性变更需要：变更提案（改动点、影响面、迁移计划）+
  主代理/契约 owner 批准，批准记录写入 CHANGELOG 与 coverage evidence。
- 错误码、权限、状态机的增删必须与本文件 §2/§3 一致，并同步
  `ERROR-CODES.md`、`PERMISSION-MATRIX.md`、`STATE-MACHINES.md` 三份注册表。
- 未经批准的语义差异会让 CI 中的 `check-openapi.rb`、
  `check-error-codes.rb`、`check-permission-matrix.rb`、`check-route-coverage.rb`
  失败，并输出具体 operationId 与修复入口（M00-CONTRACT-12）。

## 6. 版本化产物

- **上一正式版本 client**：`frontend/src/lib/api/generated/v1/` 为冻结目录
  （生成脚本输出固定指向 v1，README 声明只读；`--check` 做可复现 diff）。
- 破坏性变更进入 v2 时生成新的 `frontend/src/lib/api/generated/v2/`，
  v1 目录保留供向后兼容编译与响应 Fixture 测试（M00-CONTRACT-10）。
- `todo/openapi-operation-coverage.json` 与 `todo/OPENAPI-COVERAGE.md` 由
  `scripts/sync-operation-coverage.rb` 生成，属于冻结契约投影，字段不可手工修改。

## 7. 常见问题

- **为什么新增字段是安全的？** 客户端忽略未知字段，服务端容忍未知输入字段，
  因此可选新增不破坏已发布客户端。
- **为什么不能复用错误码？** 客户端按 `code` 决定动作；复用会让旧客户端执行错误动作。
- **如何判定"改变语义"？** 任何导致已发布客户端行为可观察变化的改动都算。
