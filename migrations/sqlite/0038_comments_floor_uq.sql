-- BBLBB 主题内楼层唯一约束（M04-SCHEMA-07，SQLite）
--
-- comments (post_id, floor) 唯一：同一主题内楼层号唯一（M04-COMMENTS-03
-- 事务内原子分配，唯一约束兜底并发）。
--
-- 其余 SCHEMA-07 唯一约束此前已落地：
-- - 板块内 slug：posts_board_slug_uq（0032）；
-- - revision 唯一：post_revisions UNIQUE(post_id, version)（0033）；
-- - 客户端请求 ID：idempotency_records UNIQUE(scope, key)（0010，
--   client_request_id 以 scope="post.create"/"draft.create" 等写入）。

CREATE UNIQUE INDEX comments_post_floor_uq ON comments (post_id, floor);
