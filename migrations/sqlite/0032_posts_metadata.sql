-- BBLBB 帖子元数据扩展（M04-SCHEMA-01，SQLite）
--
-- posts 表增加：article/discussion 类型、slug（板块内唯一）、乐观并发版本、
-- 定时/发布/置顶/精选/锁帖时间、SEO 字段、删除时间与 last_reply 缓存键。
--
-- 兼容说明：
-- - content/content_format/visibility/pinned/last_reply_by 为 0003 骨架遗留
--   列（M04-POSTS 替换骨架路由时随 post_contents/access_policy 收口）；
-- - status 的 CHECK 保持 0003 值域；'locked' 为遗留值（新代码用 closed_at），
--   pending_review/rejected 状态值随 M04-POSTS 迁移扩展（SQLite 改 CHECK 需
--   重建表，待骨架列收口时一并落地）。

ALTER TABLE posts ADD COLUMN post_type TEXT NOT NULL DEFAULT 'discussion'
    CHECK (post_type IN ('article', 'discussion'));
ALTER TABLE posts ADD COLUMN slug TEXT;
ALTER TABLE posts ADD COLUMN excerpt TEXT;
ALTER TABLE posts ADD COLUMN version INTEGER NOT NULL DEFAULT 1;
ALTER TABLE posts ADD COLUMN scheduled_at INTEGER;
ALTER TABLE posts ADD COLUMN published_at INTEGER;
ALTER TABLE posts ADD COLUMN pinned_at INTEGER;
ALTER TABLE posts ADD COLUMN featured_at INTEGER;
ALTER TABLE posts ADD COLUMN closed_at INTEGER;
ALTER TABLE posts ADD COLUMN canonical_url TEXT;
ALTER TABLE posts ADD COLUMN seo_title TEXT;
ALTER TABLE posts ADD COLUMN seo_description TEXT;
ALTER TABLE posts ADD COLUMN last_reply_id TEXT;
ALTER TABLE posts ADD COLUMN deleted_at INTEGER;

-- 板块内 slug 唯一（文章必须有 slug；草稿 slug 可空，多 NULL 不冲突）
CREATE UNIQUE INDEX posts_board_slug_uq ON posts (board_id, slug);
CREATE INDEX posts_board_status_idx ON posts (board_id, status, pinned_at, last_reply_at);
CREATE INDEX posts_author_status_idx ON posts (author_id, status, created_at);
CREATE INDEX posts_type_status_published_idx ON posts (post_type, status, published_at);
CREATE INDEX posts_scheduled_idx ON posts (scheduled_at);
