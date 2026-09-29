-- Cover the public board post_count query's board/status/deleted predicates.
CREATE INDEX posts_board_published_count_idx
    ON posts (board_id, status, deleted_at);
