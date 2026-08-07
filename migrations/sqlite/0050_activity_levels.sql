-- BBLBB levels + activity + reactions (M07-LEVELS/M07-SHOP, SQLite)
--
-- 与 mysql/mariadb 同版本同结构：level_schemes/levels（经验阈值，(scheme_id,
-- threshold) 与 (scheme_id, sort_order) 唯一）、user_levels（可重建缓存，
-- 真实来源是经验账户）、level_events（只追加升降级日志）、activity_rules
-- （签到/任务/反应/发帖/评论/排行榜奖励规则）、activity_claims（(rule_id,
-- user_id, deduplication_key) 唯一防重复奖励）与 user_reactions（复合唯一防重复）。

CREATE TABLE level_schemes (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    currency_id TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CONSTRAINT level_schemes_currency_fk FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT
);

CREATE INDEX level_schemes_active_idx ON level_schemes (is_active, name);

CREATE TABLE levels (
    id TEXT PRIMARY KEY NOT NULL,
    scheme_id TEXT NOT NULL,
    name TEXT NOT NULL,
    threshold INTEGER NOT NULL,
    sort_order INTEGER NOT NULL,
    icon TEXT NULL,
    color TEXT NULL,
    benefits_json TEXT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CONSTRAINT levels_scheme_threshold_uq UNIQUE (scheme_id, threshold),
    CONSTRAINT levels_scheme_sort_uq UNIQUE (scheme_id, sort_order),
    CONSTRAINT levels_scheme_fk FOREIGN KEY (scheme_id) REFERENCES level_schemes (id) ON DELETE CASCADE,
    CONSTRAINT levels_threshold_ck CHECK (threshold >= 0)
);

CREATE INDEX levels_sort_idx ON levels (scheme_id, sort_order);

CREATE TABLE user_levels (
    user_id TEXT NOT NULL,
    scheme_id TEXT NOT NULL,
    level_id TEXT NOT NULL,
    computed_from_balance INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, scheme_id),
    CONSTRAINT user_levels_level_fk FOREIGN KEY (level_id) REFERENCES levels (id) ON DELETE RESTRICT,
    CONSTRAINT user_levels_scheme_fk FOREIGN KEY (scheme_id) REFERENCES level_schemes (id) ON DELETE CASCADE,
    CONSTRAINT user_levels_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX user_levels_level_idx ON user_levels (scheme_id, level_id);

CREATE TABLE level_events (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    scheme_id TEXT NOT NULL,
    from_level_id TEXT NULL,
    to_level_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    CONSTRAINT level_events_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT level_events_scheme_fk FOREIGN KEY (scheme_id) REFERENCES level_schemes (id) ON DELETE CASCADE,
    CONSTRAINT level_events_from_fk FOREIGN KEY (from_level_id) REFERENCES levels (id) ON DELETE RESTRICT,
    CONSTRAINT level_events_to_fk FOREIGN KEY (to_level_id) REFERENCES levels (id) ON DELETE RESTRICT
);

CREATE INDEX level_events_user_created_idx ON level_events (user_id, created_at);
CREATE INDEX level_events_scheme_idx ON level_events (scheme_id, to_level_id);

CREATE TABLE activity_rules (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL,
    currency_id TEXT NOT NULL,
    amount INTEGER NOT NULL,
    daily_limit INTEGER NULL,
    cooldown_seconds INTEGER NULL,
    conditions_json TEXT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    is_enabled INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CONSTRAINT activity_rules_kind_ck CHECK (kind IN ('check_in', 'task', 'reaction', 'post', 'comment', 'leaderboard')),
    CONSTRAINT activity_rules_currency_fk FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT,
    CONSTRAINT activity_rules_amount_ck CHECK (amount >= 0)
);

CREATE INDEX activity_rules_kind_enabled_idx ON activity_rules (kind, is_enabled, version);

CREATE TABLE activity_claims (
    id TEXT PRIMARY KEY NOT NULL,
    rule_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    activity_day TEXT NOT NULL,
    deduplication_key TEXT NOT NULL,
    point_operation_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'granted',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CONSTRAINT activity_claims_rule_user_key_uq UNIQUE (rule_id, user_id, deduplication_key),
    CONSTRAINT activity_claims_op_uq UNIQUE (point_operation_id),
    CONSTRAINT activity_claims_rule_fk FOREIGN KEY (rule_id) REFERENCES activity_rules (id) ON DELETE CASCADE,
    CONSTRAINT activity_claims_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT activity_claims_status_ck CHECK (status IN ('granted', 'revoked'))
);

CREATE INDEX activity_claims_user_day_idx ON activity_claims (user_id, activity_day, status);
CREATE INDEX activity_claims_rule_day_idx ON activity_claims (rule_id, activity_day);

CREATE TABLE user_reactions (
    user_id TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    reaction TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, target_type, target_id, reaction),
    CONSTRAINT user_reactions_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX user_reactions_target_idx ON user_reactions (target_type, target_id);
