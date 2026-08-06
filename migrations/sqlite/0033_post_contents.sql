-- BBLBB 帖子正文与修订数据模型（M04-SCHEMA-02，SQLite）
--
-- 1) post_contents：帖子当前正文（1:1 with posts）——Markdown 原文、后端
--    清洗后的公开 HTML、renderer version（M04-MARKDOWN-05 升级触发重渲染）、
--    安全摘要（公开可见，禁止从隐藏正文截断，M04-MARKDOWN-06）；
-- 2) post_revisions：不可变修订快照（编辑历史，M04-POSTS-08）——正文与受限
--    正文快照、renderer version、change_reason、对应 post.version、创建时间。
--
-- 约束：post_contents 与 posts 1:1（post_id 主键 + 级联删除）；修订按
-- (post_id, version) 唯一（每版恰好一条）；受限正文列可空。

CREATE TABLE post_contents (
    post_id TEXT PRIMARY KEY NOT NULL,
    body_markdown TEXT NOT NULL,
    body_html TEXT NOT NULL,
    restricted_markdown TEXT,
    restricted_html TEXT,
    renderer_version TEXT NOT NULL,
    excerpt TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE
);

CREATE TABLE post_revisions (
    id TEXT PRIMARY KEY NOT NULL,
    post_id TEXT NOT NULL,
    editor_id TEXT NOT NULL,
    body_markdown TEXT NOT NULL,
    body_html TEXT NOT NULL,
    restricted_markdown TEXT,
    restricted_html TEXT,
    renderer_version TEXT NOT NULL,
    change_reason TEXT,
    version INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (post_id, version),
    FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE,
    FOREIGN KEY (editor_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX post_revisions_post_idx ON post_revisions (post_id, version);
CREATE INDEX post_revisions_editor_idx ON post_revisions (editor_id);
