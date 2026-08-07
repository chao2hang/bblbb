-- BBLBB 搜索退出与索引策略（M08-INDEX-03，MySQL）
--
-- posts 增加作者逐帖退出标记（search_index_opt_out / ai_summary_opt_out）；
-- search_site_index_policy（单行，scope_key='site'）与 board_index_policies
-- 承载管理员全站/板块索引策略（deny 优先于作者 allow，CRAWLER-POLICY.md §1）。
-- 行更新必须 bump updated_at（策略 revision 单调性来源，docs/SEARCH.md §5）。

ALTER TABLE posts ADD COLUMN search_index_opt_out TINYINT(1) NOT NULL DEFAULT 0;
ALTER TABLE posts ADD COLUMN ai_summary_opt_out TINYINT(1) NOT NULL DEFAULT 0;

CREATE INDEX posts_index_optout_idx ON posts (search_index_opt_out, ai_summary_opt_out);

CREATE TABLE search_site_index_policy (
    scope_key VARCHAR(8) PRIMARY KEY NOT NULL,
    search_index VARCHAR(8) NOT NULL DEFAULT 'allow',
    ai_summary VARCHAR(8) NOT NULL DEFAULT 'allow',
    version BIGINT NOT NULL DEFAULT 1,
    updated_by VARCHAR(36) NOT NULL,
    updated_at BIGINT NOT NULL,
    CONSTRAINT search_site_scope_ck CHECK (scope_key = 'site'),
    CONSTRAINT search_site_index_ck CHECK (search_index IN ('allow', 'deny')),
    CONSTRAINT search_site_ai_ck CHECK (ai_summary IN ('allow', 'deny'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE board_index_policies (
    board_id VARCHAR(36) PRIMARY KEY NOT NULL,
    search_index VARCHAR(8) NOT NULL DEFAULT 'allow',
    ai_summary VARCHAR(8) NOT NULL DEFAULT 'allow',
    version BIGINT NOT NULL DEFAULT 1,
    updated_by VARCHAR(36) NOT NULL,
    updated_at BIGINT NOT NULL,
    CONSTRAINT board_index_policies_index_ck CHECK (search_index IN ('allow', 'deny')),
    CONSTRAINT board_index_policies_ai_ck CHECK (ai_summary IN ('allow', 'deny')),
    CONSTRAINT board_index_policies_board_fk FOREIGN KEY (board_id) REFERENCES boards (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
