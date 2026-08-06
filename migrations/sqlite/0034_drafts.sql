-- BBLBB 草稿数据模型（M04-SCHEMA-03，SQLite）
--
-- drafts：独立草稿资源（OpenAPI Draft，与 posts 分离）——owner、可选板块、
-- article/discussion 类型、标题、Markdown 原文、可见性等级/访问策略、定时
-- 发布时间、乐观并发版本、软删除。
--
-- 索引：owner 维度 cursor 列表（owner_id, deleted_at, updated_at）与定时
-- 发布 Job 扫描（scheduled_at）。
--
-- 约束：board 可空（草稿可在未选板块时创建）；board 删除置空而非级联删除
-- 草稿；owner 删除级联清理。

CREATE TABLE drafts (
    id TEXT PRIMARY KEY NOT NULL,
    owner_id TEXT NOT NULL,
    board_id TEXT,
    post_type TEXT NOT NULL DEFAULT 'discussion'
        CHECK (post_type IN ('article', 'discussion')),
    title TEXT NOT NULL,
    markdown TEXT NOT NULL,
    visibility_level INTEGER,
    access_policy TEXT,
    scheduled_at INTEGER,
    version INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER,
    FOREIGN KEY (owner_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (board_id) REFERENCES boards (id) ON DELETE SET NULL
);

CREATE INDEX drafts_owner_cursor_idx ON drafts (owner_id, deleted_at, updated_at);
CREATE INDEX drafts_scheduled_idx ON drafts (scheduled_at);
