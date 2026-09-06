-- BBLBB 经济与个人域扩展（GAP-FIX 管理域 Part B，MariaDB）
--
-- Table-for-table equivalent to migrations/sqlite/0062_economy_ext.sql:
-- 1) level_rules: level rule table backing GET/PATCH /api/v1/admin/levels.
--    0050's `levels` is a per-scheme threshold table (UUID key, no admin rule
--    columns); integer-level-addressed admin rules get their own table.
--    users.level (0019) is the user's current level for user_count. Seeds
--    5 levels; `version` backs the If-Match optimistic lock on PATCH.
-- 2) posts.summary: author-written summary (<=300 chars, article type).
--    Distinct from post_contents.excerpt (auto-generated excerpt) and the
--    legacy posts.excerpt SEO column; posts.price_coin came in 0061.
-- 3) drafts snapshot columns: price_coin/summary/tags_json persisted with
--    the draft and synced to posts on publish.
--
-- oauth_grants is NOT created: 0055_oidc's oauth_consents
-- (user_id, client_id, scope, granted_at, revoked_at) is already the
-- user authorization record table; GET/DELETE /api/v1/me/oauth-grants
-- reads/revokes it directly (client_name from oauth_clients.name,
-- last_used_at aggregated from oauth_tokens).

CREATE TABLE level_rules (
    level INT NOT NULL,
    name VARCHAR(64) NOT NULL,
    min_exp BIGINT NOT NULL,
    daily_post_limit BIGINT NOT NULL,
    daily_comment_limit BIGINT NOT NULL,
    attachment_quota BIGINT NOT NULL,
    is_enabled TINYINT(1) NOT NULL DEFAULT 1,
    version BIGINT NOT NULL DEFAULT 1,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (level),
    CONSTRAINT level_rules_min_exp_ck CHECK (min_exp >= 0),
    CONSTRAINT level_rules_limits_ck CHECK (daily_post_limit >= 0 AND daily_comment_limit >= 0 AND attachment_quota >= 0)
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_general_ci;

INSERT INTO level_rules (level, name, min_exp, daily_post_limit, daily_comment_limit, attachment_quota, is_enabled, version, updated_at)
VALUES
    (1, 'Lv1 新人', 0, 5, 20, 52428800, 1, 1, 1722816000),
    (2, 'Lv2 学徒', 500, 10, 50, 104857600, 1, 1, 1722816000),
    (3, 'Lv3 常客', 2000, 20, 100, 209715200, 1, 1, 1722816000),
    (4, 'Lv4 资深', 8000, 40, 200, 524288000, 1, 1, 1722816000),
    (5, 'Lv5 元老', 30000, 80, 400, 1073741824, 1, 1, 1722816000);

CREATE INDEX level_rules_enabled_idx ON level_rules (is_enabled, level);

-- Author-written summary (article type; post_contents.excerpt is the
-- auto-generated excerpt -- different semantics).
ALTER TABLE posts ADD COLUMN summary TEXT NULL;

-- Draft snapshot: paid pricing / summary / tags.
ALTER TABLE drafts ADD COLUMN price_coin BIGINT NULL;
ALTER TABLE drafts ADD COLUMN summary TEXT NULL;
ALTER TABLE drafts ADD COLUMN tags_json TEXT NULL;
