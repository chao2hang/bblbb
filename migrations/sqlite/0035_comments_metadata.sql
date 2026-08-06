-- BBLBB 评论元数据扩展（M04-SCHEMA-04，SQLite）
--
-- comments 表增加：引用回复（quoted_comment_id，删除置空保留占位语义）、
-- 乐观并发版本、软删除时间。
--
-- 兼容说明：content/content_format 为 0003 骨架遗留列（M04-COMMENTS 替换
-- 骨架时收口到与 post_contents 一致的正文表）；floor 为主题内楼层号（既有
-- 列，SCHEMA.md 语义一致；唯一约束随 M04-SCHEMA-07 落地）；status 的 CHECK
-- 保持 0003 值域（published/hidden/deleted；pending 审核态随 M04-POSTS 迁移
-- 扩展）。

ALTER TABLE comments ADD COLUMN quoted_comment_id TEXT REFERENCES comments (id) ON DELETE SET NULL;
ALTER TABLE comments ADD COLUMN version INTEGER NOT NULL DEFAULT 1;
ALTER TABLE comments ADD COLUMN deleted_at INTEGER;

CREATE INDEX comments_quoted_idx ON comments (quoted_comment_id);
