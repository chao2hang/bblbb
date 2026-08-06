# BBLBB — Markdown 渲染管线与策略变更管理

> M04-MARKDOWN：安全 Markdown 渲染管线的实现契约与**策略变更（升级/缓存/回滚）**操作手册。
> 事实来源：本文件 ↔ `backend/src/content/markdown/*` ↔ `frontend/src/lib/components/SafeHtml.svelte` ↔ `scripts/check-html-sinks.rb`。

## 1. 管线与版本模型

```
Markdown 原文
  └─ render_to_html    (pulldown-cmark 0.10，仅表格扩展；原始 HTML 事件剥离；
                        标题锚点/代码块/引用/表格/嵌套确定性上限，M04-MARKDOWN-02/04)
  └─ sanitize_html     (ammonia 4.1.4 allowlist：标签/属性/协议/rel/target/
                        iframe Provider，M04-MARKDOWN-03)
  └─ render_public_excerpt (纯文本摘要，只取公开正文，M04-MARKDOWN-06)
     └─ render_content → {body_html, restricted_html, excerpt, renderer_version}
```

**策略版本**：`POLICY_VERSION = RENDERER_VERSION + "+" + SANITIZER_VERSION`
（`backend/src/content/markdown/policy.rs`）。

- `RENDERER_VERSION` 当前 `markdown-v1`：渲染行为变更（扩展开关、锚点规则、
  各类上限、HTML 转义策略）时递增；
- `SANITIZER_VERSION` 当前 `ammonia-v1`：清洗 allowlist 变更（标签/属性/协议/
  iframe Provider/URL 规则）时递增；
- 任一递增 → `POLICY_VERSION` 变化 → 存量行 `renderer_version` 判定为 stale。

**落库**：`post_contents.renderer_version` 与 `post_revisions.renderer_version`
存 POLICY_VERSION。修订快照的 markdown 原文不可变；渲染产物（html/excerpt）
是**派生数据**，随策略升级按需再生。

## 2. 策略升级流程（变更发布）

1. **改代码**：修改 `render.rs`/`sanitize.rs` 行为，并在 `policy.rs` 递增对应
   版本常量（保持两个版本各自独立递增）。
2. **全量门禁**：`make check`（含 `check-html-sinks` 前端 sink 扫描）与
   `make test` 全绿；XSS corpus（`cargo test --test markdown_xss`）与
   一致性（`cargo test --test markdown_consistency`）必须通过。
3. **部署新版本**：应用包含新策略版本的二进制。此步不产生数据库迁移
   （策略版本是代码常量，禁止修改已执行迁移文件——三库迁移等价测试
   `migration_equivalence` 约束）。
4. **重渲染旧数据**：执行 `enqueue_rerender_jobs(pool, limit)`（或等价运维
   命令）——为所有 `renderer_version <> 当前` 的 `post_contents`/`post_revisions`
   行各入队一个 `markdown.rerender` Job（kind 见 docs/JOBS.md §7，payload
   `{target: content|revision, id}`，dedup key `markdown:rerender:{target}:{id}`
   幂等合并）。
5. **worker 处理**：Job 用当前策略重渲染并覆盖渲染产物（markdown 快照与
   元数据不变）；行缺失或已最新 → 幂等成功；无效 payload → 永久死信。
6. **验证收尾**：再次运行 `enqueue_rerender_jobs` 应返回 0（无 stale 行）；
   抽查若干帖子 body_html 含新策略产物；观察 Job 队列 drained。

## 3. 缓存失效

- **数据库投影**：`body_html`/`excerpt` 为持久化投影，无二级缓存——列表/详情
  每次直读 `post_contents`，重渲染即对下一次读生效。
- **API 响应缓存**：若为 Post detail/list 配置 HTTP 缓存，缓存键必须包含
  `renderer_version`（或 Post 的 `version` 乐观并发字段）；策略升级后必须
  使旧版本缓存失效。默认实现不设正文 HTTP 长缓存（正文随编辑与重渲染变化）。
- **浏览器缓存**：正文页面 `Cache-Control` 不缓存或短期（建议 `no-cache` +
  ETag）；前端 SafeHtml 直接注入服务端渲染输出，无客户端正文缓存。
- **搜索索引**：`search_documents` 的纯文本与 excerpt 由 `search.index` Job
  独立维护（源码 → clean text，见 docs/SEARCH.md），**不**依赖 `body_html`，
  渲染策略变更无需级联重建搜索索引。
- **前端 SSR**：无 JS 一致性由 SSR + SafeHtml 保证（
  `frontend/src/lib/testing/ssr/post-content-nojs.test.ts`）；静态资源
  （JS/CSS）不含正文，不参与失效。

## 4. 回滚方案

**前提**：markdown 原文永不丢失——`post_contents.body_markdown` 与
`post_revisions.body_markdown` 在重渲染中保持不变，回滚只需重新渲染。

**回滚步骤**（可不停机、分批）：

1. 代码回滚：`git revert` 渲染/清洗策略变更（含版本常量回到旧值）。
2. 重新部署旧版本二进制。
3. 此时行内 `renderer_version` = 新值 ≠ 旧 `POLICY_VERSION` → 全部判定为
   stale（与升级对称）。
4. 运行 `enqueue_rerender_jobs` → 全部按旧策略重渲染，产物恢复升级前结果。

**风险说明**：重渲染是覆盖写（`UPDATE post_contents/post_revisions`），
不产生历史快照；若需审计"某个时间点渲染成什么样"，以 `post_revisions`
的 markdown 原文 + 当时策略版本为事实，可离线复现。升级/回滚期间短暂展示
的是上一次成功渲染的结果，操作完成即收敛。

## 5. 验证矩阵

| 层 | 验证 | 命令 |
|---|---|---|
| 后端单元 | 渲染/清洗/摘要/重渲染 | `cargo test --lib content::markdown` |
| 后端集成 | XSS corpus（28 例） | `cargo test --test markdown_xss` |
| 后端集成 | 升级重渲染 Job 生命周期 | `cargo test --test markdown_rerender` |
| 后端集成 | 渲染一致性（纯文本/代码/链接/图片/长文/无 JS） | `cargo test --test markdown_consistency` |
| 前端 | SafeHtml 唯一 `{@html}` sink | `ruby scripts/check-html-sinks.rb` |
| 前端 | 无 JS SSR 一致性 | `npx vitest run src/lib/testing/ssr/post-content-nojs.test.ts` |
| 全量 | 门禁 | `make check && make test` |

## 6. 参考实现

- `backend/src/content/markdown/policy.rs` — 版本常量与上限常量
- `backend/src/content/markdown/render.rs` — CommonMark 渲染（确定性上限）
- `backend/src/content/markdown/sanitize.rs` — ammonia allowlist
- `backend/src/content/markdown/excerpt.rs` — 公开安全摘要
- `backend/src/content/markdown/rerender.rs` — `markdown.rerender` Job（入队/处理）
- `frontend/src/lib/components/SafeHtml.svelte` — 唯一 `{@html}` sink
- `scripts/check-html-sinks.rb` — 前端 HTML sink 静态检查
