-- BBLBB 帖子关联数据模型（M04-SCHEMA-05，SQLite）
--
-- 1) posts.cover_attachment_id：封面附件引用（只存附件 UUID，attachments 表
--    M6 落地后补 FK；禁止存远程/签名 URL，与 users.cover_attachment_id 一致）；
-- 2) post_tags.created_at：标签关联创建时间（标签列表排序）；
-- 3) post_attachments：帖子附件引用（图库/封面），position 排序，
--    attachments 表 M6 落地后补 FK。
--
-- 引用回复关联（quoted_comment_id）已随 M04-SCHEMA-04 落地。

ALTER TABLE posts ADD COLUMN cover_attachment_id TEXT;
ALTER TABLE post_tags ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0;

CREATE TABLE post_attachments (
    id TEXT PRIMARY KEY NOT NULL,
    post_id TEXT NOT NULL,
    attachment_id TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'gallery'
        CHECK (kind IN ('cover', 'gallery')),
    position INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE
);

CREATE INDEX post_attachments_post_idx ON post_attachments (post_id, position);
