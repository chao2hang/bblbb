# BBLBB — 搜索索引存储契约（M03-SEARCH-STORE）

> 状态：Frozen candidate · 负责领域：后端/搜索 · 版本：v1.0-rc.2
> 事实来源优先级：`REQUIREMENTS.md` → `openapi/openapi.yaml` → `SCHEMA.md`/迁移 → 本文档 → `CRAWLER-POLICY.md`

## 1. 目标与范围

本文档定义搜索索引的**存储层契约**（M03-SEARCH-STORE）：跨 SQLite FTS5、
MySQL 8 FULLTEXT、MariaDB 10.11 FULLTEXT 一致的文档模型、revision 语义与安全
文本约束。公开 API 形状（`GET /api/v1/search` → `SearchPage`/`SearchResult`）
见 OpenAPI；索引策略（作者退出、管理员全站/板块策略、统一排除规则）见
M08-INDEX 与 `CRAWLER-POLICY.md`。

需求锚点（`REQUIREMENTS.md`）：

- 搜索属于 v1.0；SQLite 使用 FTS5，MySQL/MariaDB 使用全文索引，并在结果层
  重新执行内容可见性过滤。
- 隐藏正文绝不会通过搜索、excerpt、highlight、相关内容或错误泄漏。

## 2. 搜索文档最小模型

内部索引一行为一个可搜索实体（`search_documents`，迁移 0030 起落地）。
Rust 模型：`backend/src/search/mod.rs::SearchDocument`。

| 字段 | 类型 | 公开投影 | 说明 |
|---|---|---|---|
| `id` | TEXT UUID | `id` | 源实体 id（posts/users/boards/tags.id） |
| `entity_type` | TEXT | `type` | `post`/`user`/`board`/`tag`（OpenAPI `SearchResult.type`） |
| `title` | TEXT | `title` | post.title / user.display_name / board.name / tag.name；≤ 240 字符 |
| `body` | TEXT | — | 内部索引正文（清洗后）；≤ 100_000 字符；绝不对外投影 |
| `excerpt` | TEXT | `excerpt` | 公开投影安全摘要；≤ 200 字符；绝不包含受限正文 |
| `slug` | TEXT | `url` | post.slug / user.username_normalized / board.slug / tag.slug；≤ 120 字符 |
| `author_id` | TEXT UUID | — | post 作者公开 id（仅 post；结果层拼接作者卡用） |
| `tags_json` | TEXT | — | post 标签名列表（仅 post） |
| `source_revision` | INTEGER | — | 源内容版本 = 源实体 `updated_at`（毫秒，见 §4） |
| `policy_revision` | INTEGER | — | 可见性/策略状态版本（见 §5） |
| `indexed_at` | INTEGER | — | 入索引时间（毫秒） |

构造即清洗/校验（`SearchDocument::new`）：标题 trim 后非空且 ≤ 240；正文清洗
后非空且 ≤ 100_000；slug 为 `[a-z0-9_-]+` 且 ≤ 120（下划线兼容
`username_normalized`；board/tag slug 由其各自校验模块进一步约束为
`[a-z0-9-]+`）；摘要按字符边界截断到 200 并以 `…` 结尾。

## 3. 公开投影字段

`SearchResult`（OpenAPI）只暴露 `id`、`type`、`title`、`url`、`excerpt`：

- `url`：由 `slug` 按类型组装（`/posts/{slug}`、`/users/{username}`、
  `/boards/{slug}`、`/tags/{slug}`——对应端点在 M4/M3 落地）。
- `excerpt`：从**已清洗索引正文**按字符边界截断（`excerpt_from_clean`），
  绝不包含受限正文、`restricted_html` 或隐藏内容。
- `body`、`author_id`、`tags_json`、revisions、`indexed_at` 属于内部字段，
  不出现在任何公开投影/DOM/日志/异常/遥测（`SECURITY.md` §6 精神）。

## 4. source revision

- 定义：`source_revision = 源实体 updated_at`（毫秒）。
- 语义：内容每次变更必须 bump `updated_at`（与 boards/tags/users If-Match
  乐观并发版本同源，`SCHEMA.md` §6；M03-BOARDS-05/07、M03-PROFILE-04 已落地）。
- 用途：索引新鲜度水位。freshen/rebuild 时若 `index.source_revision <
  source.updated_at`，说明内容已漂移，需要重建该文档。

## 5. policy revision

- 定义：`policy_revision = max(全部策略相关行 updated_at)`。对 post 文档，
  策略输入 = post.status、post.visibility、board.is_active、board.visibility、
  作者账号状态/deleted_at、作者索引退出标记（`search_index_opt_out`，
  M08-INDEX-03 落地）。
- 不变式：任何可能改变可见性/策略的变更**必须** bump 对应行 `updated_at`
  （设计契约；M5 制裁、M7 权益、M8 策略变更遵循）。post 自身内容变更 bump
  `post.updated_at`，同时进入 source 与 policy 的 max。
- 单调性：`policy_revision` 非递减——随最新输入行变更单调不减；任何输入行
  `updated_at` 更大都会使 max 严格递增。
- 用途：可见性/策略漂移水位 + 陈旧写防覆盖。
- **旧 revision 不覆盖新**：索引写入为条件 upsert，仅当
  `stored.policy_revision <= candidate.policy_revision` 才应用
  （`INSERT ... ON CONFLICT(id) DO UPDATE SET ... WHERE search_documents.policy_revision <= :candidate`）
  ——持旧策略快照的写回者（max 更小）被拒绝；相等（内容/策略未漂移）允许幂等
  重写，强制重建（M03-SEARCH-STORE-06）始终生效。绝不回退已更新的索引。

## 6. 索引写入安全文本约束（M03-SEARCH-STORE-05 强制）

- 索引写入只接受经过可见性裁决的安全文本：draft/hidden/locked/deleted 正文、
  非公开可见性内容、受限板块内容、作者退出标记命中内容一律不写入。
  裁决入口：`backend/src/search/gate.rs::decide_*_indexability`
  （post 按 status/visibility/板块启用与可见性/作者账号；user 按 status/删除；
  board 按启用/可见性；tag 按启用）——被排除的实体从索引移除，绝不写入受限内容。
- 索引正文只存清洗后的纯文本（`clean_index_text`：控制字符/连续空白折叠为单个
  空格、首尾 trim、长度上限）；写路径先经 `vet_index_text` 拒绝
  `restricted_html`/`restricted_markdown` 特征串与 HTML 标记——绝不存
  `restricted_html` 或渲染 HTML（第二道防线，第一道是写路径只接收公开投影字段）。
- 结果层（M08-INDEX-07）返回前重新执行实时可见性/处罚/退出判断——索引只是
  候选集，不是授权裁决。

## 7. 三数据库全文检索策略（M03-SEARCH-STORE-02/03/04）

| 数据库 | 索引机制 | 分词 | 已知限制 |
|---|---|---|---|
| SQLite 3.40+ | FTS5 external content 表 | unicode61 | 无内置中文分词；按 token 匹配 |
| MySQL 8 | InnoDB FULLTEXT | 默认空格/标点分词 | `innodb_ft_min_token_size` 等参数 |
| MariaDB 10.11 | FULLTEXT | 类似 MySQL | 与 MySQL 存在已知差异（M03-SEARCH-STORE-04 记录） |

三库基础查询契约（文档存在/查询命中/删除/重建/旧 revision 不覆盖新）必须一致
（M03-SEARCH-STORE-07 同一 Fixture 验证）。

### 7.1 触发器/Job 更新策略（M03-SEARCH-STORE-02/06）

- `search_documents` 是唯一索引写入面，由索引 Job（M03-SEARCH-STORE-06）
  维护（创建/更新/隐藏/删除/恢复/退出索引均为幂等 Job）。
- SQLite：`search_fts` 为 FTS5 external content 表（`content='search_documents'`、
  `content_rowid='rowid'`、`tokenize='unicode61'`），0030 迁移内置三个同步
  触发器（`search_fts_ai/ad/au`）——Job 不直接写 FTS 表，触发器自动把
  title/body 同步进 FTS5。
- MySQL/MariaDB：FULLTEXT 索引（0031/0032）由 InnoDB 原生随行更新，
  无需触发器。

### 7.2 重建命令（M03-SEARCH-STORE-02/03/04）

- SQLite：`INSERT INTO search_fts(search_fts) VALUES('rebuild')`——从
  `search_documents` 全量重建 FTS5 external content 表，幂等。
- MySQL/MariaDB：`OPTIMIZE TABLE search_documents`——重建表与 FULLTEXT 索引。
- 统一入口：`backend/src/search/fts.rs::rebuild_fts(pool)`（Either 按引擎分发）。

### 7.3 MySQL 8 FULLTEXT 分词限制（M03-SEARCH-STORE-03）

- 0031 迁移：`ALTER TABLE search_documents ADD FULLTEXT INDEX
  search_documents_fts_idx (title, body)`（InnoDB 原生随行更新）。
- `innodb_ft_min_token_size`（默认 3）：短于 3 字符的 token 不索引、不可命中。
- `innodb_ft_max_token_size`（默认 84）：长于 84 字符的 token 不索引。
- 未启用 ngram parser：CJK 文本按空白/标点分词，长中文串成为最长 84 字符的
  单 token（已知限制；结果层以实时可见性过滤兜底，M08-INDEX-07）。
- 以上为服务端全局变量，非逐表配置；部署文档（`OPERATIONS.md`）记录调整方式。

### 7.4 MariaDB 10.11 与 MySQL 8 的已知差异（M03-SEARCH-STORE-04）

- 0031 迁移对 mysql/mariadb 使用同一 DDL（迁移等价测试要求可执行 SQL 字节
  一致；mariadb 文件以注释登记本差异，见 `migrations/mariadb/0031_search_fts.sql`）。
- **ngram parser**：MySQL 8 提供 `WITH PARSER ngram` 用于 CJK 分词；MariaDB
  10.11 无同等 ngram parser——两者默认都按空白/标点分词，长中文串成为
  ≤84 字符单 token。
- **分词上限**：两者默认一致（`innodb_ft_min_token_size`=3 /
  `innodb_ft_max_token_size`=84，服务端变量）。
- **重建**：`OPTIMIZE TABLE search_documents` 两者可用；MariaDB 另支持
  `ALTER TABLE search_documents FORCE`。
- **停用词**：两发行版内置 InnoDB 停用词表可能不同；行为只在部署验证中固定
  （M03-SEARCH-STORE-07 三库 Fixture / M16 专项）。
- 基础查询契约（命中/删除/重建）以同一 Fixture 验证，与 MySQL 保持一致。

## 8. 相关文档

- `openapi/openapi.yaml`：`SearchResult`/`SearchPage`/`searchPublicContent`
- `docs/REQUIREMENTS.md`：搜索需求锚点与隐藏正文红线
- `docs/CRAWLER-POLICY.md`：索引投影与 AI 爬虫边界（M08 扩展）
- `backend/src/search/mod.rs`：文档模型与清洗/截断/revision 实现
