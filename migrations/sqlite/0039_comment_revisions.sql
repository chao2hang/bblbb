-- BBLBB 评论修订快照（M04-COMMENTS-05，SQLite）
--
-- comment_revisions：不可变修订快照（编辑历史）——正文与清洗 HTML 快照、
-- renderer version、change_reason、对应 comment.version、创建时间。
--
-- 约束：修订按 (comment_id, version) 唯一（每版恰好一条）；随 comment
-- 级联删除；编辑正文写入时同时渲染/清洗（M04-COMMENTS-05）。

CREATE TABLE comment_revisions (
    id TEXT PRIMARY KEY NOT NULL,
    comment_id TEXT NOT NULL,
    editor_id TEXT NOT NULL,
    body_markdown TEXT NOT NULL,
    body_html TEXT NOT NULL,
    renderer_version TEXT NOT NULL,
    change_reason TEXT,
    version INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (comment_id, version),
    FOREIGN KEY (comment_id) REFERENCES comments (id) ON DELETE CASCADE,
    FOREIGN KEY (editor_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX comment_revisions_comment_idx ON comment_revisions (comment_id, version);
CREATE INDEX comment_revisions_editor_idx ON comment_revisions (editor_id);
