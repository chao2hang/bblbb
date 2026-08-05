# Frozen v1 API client types

> 冻结基线：`openapi/openapi.yaml` v1.0.0（2026-08-04 冻结）。

## 内容

- `types.ts` — 由 `components.schemas` 生成的 TypeScript interface/type。
- `enums.ts` — 由 OpenAPI `enum` 声明的命名联合类型。
- `index.ts` — 统一导出入口。

## 规则（M00-CONTRACT-07 / -10 / -11）

- **禁止手工修改**：这些文件只能由
  `ruby scripts/generate-ts-types.rb` 重新生成；`--check` 模式在 CI 校验
  无漂移（可复现 diff 检查）。
- **v1 为冻结版本**：本目录永远对应上一正式发布契约，不做就地变更。
  破坏性契约变更进入 v2 时必须生成新的
  `frontend/src/lib/api/generated/v2/`，保留 v1 目录供向后兼容编译与
  Fixture 测试。
- 生成文件当前不被 hand-written client（`frontend/src/lib/api/client.ts`）
  引用；M00-FRONTEND-03 接入后方成为 API DTO 唯一类型来源。
