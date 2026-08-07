-- BBLBB levels + activity + reactions (M07-LEVELS/M07-SHOP, MySQL)
--
-- level_schemes/levels: versioned experience thresholds; (scheme_id, threshold)
--   and (scheme_id, sort_order) unique. Currency drives exp source account.
-- user_levels: rebuildable (user_id, scheme_id) composite-PK cache, computed
--   from the exp balance — never the source of truth for rewards.
-- level_events: append-only promotion/demotion journal (reason + created_at).
-- activity_rules: check_in/task/reaction/post/comment/leaderboard reward rules
--   with daily limit, cooldown and conditions_json.
-- activity_claims: deduplicated per (rule_id, user_id, deduplication_key),
--   activity_day in user timezone; point_operation_id unique.
-- user_reactions: (user_id, target_type, target_id, reaction) unique; reactions
--   never alter visibility/moderation/ordering or cash value.

CREATE TABLE level_schemes (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    name VARCHAR(64) NOT NULL,
    currency_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    is_active TINYINT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT level_schemes_currency_fk FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX level_schemes_active_idx ON level_schemes (is_active, name);

CREATE TABLE levels (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    scheme_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    name VARCHAR(64) NOT NULL,
    threshold BIGINT NOT NULL,
    sort_order INT NOT NULL,
    icon VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    color VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NULL,
    benefits_json TEXT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT levels_scheme_threshold_uq UNIQUE (scheme_id, threshold),
    CONSTRAINT levels_scheme_sort_uq UNIQUE (scheme_id, sort_order),
    CONSTRAINT levels_scheme_fk FOREIGN KEY (scheme_id) REFERENCES level_schemes (id) ON DELETE CASCADE,
    CONSTRAINT levels_threshold_ck CHECK (threshold >= 0)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX levels_sort_idx ON levels (scheme_id, sort_order);

CREATE TABLE user_levels (
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    scheme_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    level_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    computed_from_balance BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (user_id, scheme_id),
    CONSTRAINT user_levels_level_fk FOREIGN KEY (level_id) REFERENCES levels (id) ON DELETE RESTRICT,
    CONSTRAINT user_levels_scheme_fk FOREIGN KEY (scheme_id) REFERENCES level_schemes (id) ON DELETE CASCADE,
    CONSTRAINT user_levels_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX user_levels_level_idx ON user_levels (scheme_id, level_id);

CREATE TABLE level_events (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    scheme_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    from_level_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    to_level_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    reason VARCHAR(64) NOT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT level_events_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT level_events_scheme_fk FOREIGN KEY (scheme_id) REFERENCES level_schemes (id) ON DELETE CASCADE,
    CONSTRAINT level_events_from_fk FOREIGN KEY (from_level_id) REFERENCES levels (id) ON DELETE RESTRICT,
    CONSTRAINT level_events_to_fk FOREIGN KEY (to_level_id) REFERENCES levels (id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX level_events_user_created_idx ON level_events (user_id, created_at);
CREATE INDEX level_events_scheme_idx ON level_events (scheme_id, to_level_id);

CREATE TABLE activity_rules (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    kind VARCHAR(16) NOT NULL,
    currency_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    amount BIGINT NOT NULL,
    daily_limit INT NULL,
    cooldown_seconds BIGINT NULL,
    conditions_json TEXT NULL,
    version INT NOT NULL DEFAULT 1,
    is_enabled TINYINT NOT NULL DEFAULT 1,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT activity_rules_kind_ck CHECK (kind IN ('check_in', 'task', 'reaction', 'post', 'comment', 'leaderboard')),
    CONSTRAINT activity_rules_currency_fk FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT,
    CONSTRAINT activity_rules_amount_ck CHECK (amount >= 0)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX activity_rules_kind_enabled_idx ON activity_rules (kind, is_enabled, version);

CREATE TABLE activity_claims (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    rule_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    activity_day VARCHAR(10) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    deduplication_key VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    point_operation_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'granted',
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT activity_claims_rule_user_key_uq UNIQUE (rule_id, user_id, deduplication_key),
    CONSTRAINT activity_claims_op_uq UNIQUE (point_operation_id),
    CONSTRAINT activity_claims_rule_fk FOREIGN KEY (rule_id) REFERENCES activity_rules (id) ON DELETE CASCADE,
    CONSTRAINT activity_claims_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT activity_claims_status_ck CHECK (status IN ('granted', 'revoked'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX activity_claims_user_day_idx ON activity_claims (user_id, activity_day, status);
CREATE INDEX activity_claims_rule_day_idx ON activity_claims (rule_id, activity_day);

CREATE TABLE user_reactions (
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    target_type VARCHAR(32) NOT NULL,
    target_id VARCHAR(64) NOT NULL,
    reaction VARCHAR(32) NOT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (user_id, target_type, target_id, reaction),
    CONSTRAINT user_reactions_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX user_reactions_target_idx ON user_reactions (target_type, target_id);
