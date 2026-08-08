# 上一版本生成 client 的兼容性 Fixture（M16-HARNESS-07）

## 冻结契约

`openapi.yaml` 是 **M15（commit `468883e`）** 的 `openapi/openapi.yaml` 精确副本，
代表"上一版本生成的 client"所依赖的 API 表面。生成方式：

```sh
git show 468883e:openapi/openapi.yaml > compat/frozen-client/openapi.yaml
```

## 校验

```sh
ruby scripts/check-client-compat.rb
```

逐操作保证：

1. **操作表面**：frozen 中每个 `METHOD path` 在当前契约仍存在。
2. **请求参数**：旧客户端发送的每个 query/path/header 参数当前仍被接受；
   旧客户端省略的参数不会被新变成必填。
3. **请求体**：旧客户端发送的每个属性当前仍存在；当前不会新增必填属性。
4. **响应体**：旧客户端读取的每个字段当前仍存在（允许新增字段）；
   frozen 的 enum 值不会从当前响应中移除。

## 兼容规则

- **新增字段/新增操作是合法的**（不破坏旧客户端）。
- **移除字段/参数、把可选变必填、移除 enum 值、改变类型** 都是破坏性变更，
  必须升级 API 版本（见 `docs/API-COMPATIBILITY.md`），不能直接改契约。

## 版本升级流程

每次 v1.x 契约冻结后，把当时的 `openapi/openapi.yaml` 复制到
`compat/frozen-client-v<上一版本>/`，并让 `check-client-compat.rb` 对每个
frozen 版本逐一校验。v1.0.0 发布时冻结的第一个版本即本目录。
