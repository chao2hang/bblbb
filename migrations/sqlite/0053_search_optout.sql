-- BBLBB 搜索退出与索引策略（M08-INDEX-03，SQLite）
--
-- posts 增加作者逐帖退出标记：
-- - search_index_opt_out：作者退出该帖的搜索引擎公开索引；
-- - ai_summary_opt_out：作者退出该帖的 AI 摘要生成。
-- 均为逐帖作者偏好；管理员全站/板块策略优先（deny 覆盖作者 allow，
-- CRAWLER-POLICY.md §1）。
--
-- search_site_index_policy：全站索引策略（单行，scope_key 固定 'site'）；
-- board_index_policies：按板块索引策略（board_id 主键，板块删除级联清理）。
-- 策略值封闭枚举：'allow'（默认，遵循作者选择）/ 'deny'（强制退出索引）。
-- 行更新必须 bump updated_at（策略 revision 单调性来源，docs/SEARCH.md §5）。

ALTER TABLE posts ADD COLUMN search_index_opt_out INTEGER NOT NULL DEFAULT 0;
ALTER TABLE posts ADD COLUMN ai_summary_opt_out INTEGER NOT NULL DEFAULT 0;

CREATE INDEX posts_index_optout_idx ON posts (search_index_opt_out, ai_summary_opt_out);

CREATE TABLE search_site_index_policy (
    scope_key TEXT PRIMARY KEY NOT NULL DEFAULT 'site' CHECK (scope_key = 'site'),
    search_index TEXT NOT NULL DEFAULT 'allow' CHECK (search_index IN ('allow', 'deny')),
    ai_summary TEXT NOT NULL DEFAULT 'allow' CHECK (ai_summary IN ('allow', 'deny')),
    version INTEGER NOT NULL DEFAULT 1,
    updated_by TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE board_index_policies (
    board_id TEXT PRIMARY KEY NOT NULL,
    search_index TEXT NOT NULL DEFAULT 'allow' CHECK (search_index IN ('allow', 'deny')),
    ai_summary TEXT NOT NULL DEFAULT 'allow' CHECK (ai_summary IN ('allow', 'deny')),
    version INTEGER NOT NULL DEFAULT 1,
    updated_by TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (board_id) REFERENCES boards (id) ON DELETE CASCADE
);
