-- BBLBB comments floor uniqueness (M04-SCHEMA-07, MariaDB)
--
-- comments (post_id, floor) unique: floor numbers are unique within a topic
-- (atomic allocation in a transaction, M04-COMMENTS-03; the unique constraint
-- backs concurrency).
--
-- Other M04-SCHEMA-07 unique constraints landed earlier:
-- - board-scoped slug: posts_board_slug_uq (0032);
-- - revision uniqueness: post_revisions UNIQUE(post_id, version) (0033);
-- - client request id: idempotency_records UNIQUE(scope, key) (0010,
--   client_request_id stored with scope="post.create"/"draft.create" etc.).

CREATE UNIQUE INDEX comments_post_floor_uq ON comments (post_id, floor);
