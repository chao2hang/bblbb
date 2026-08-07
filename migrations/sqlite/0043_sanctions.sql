-- BBLBB 处罚（M05-SCHEMA-03，SQLite）
--
-- sanctions：warning/rate_limit/mute/board_mute/ban。board_mute 必须带
-- board_id；其他 kind 拒绝携带板块范围（CHECK 强制，三库一致）。
-- ends_at 可空 = 永久（warning/ban 常为永久）；非空时须晚于 starts_at。
-- status 由模型层按时间推移在 scheduled/active/expired 间推进；
-- revoked 必须带 revoked_at 与 revoked_by（CHECK 强制）。
--
-- sanction_reversals：撤销记录只追加不可变——每条处罚至多一条
-- （UNIQUE(sanction_id)）；撤销证据链（谁、何时、为何）不可覆盖。
-- sanctions.revoked_at/revoked_by/revoke_reason 为查询便利的当前态镜像。

CREATE TABLE sanctions (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    board_id TEXT,
    kind TEXT NOT NULL
        CHECK (kind IN ('warning', 'rate_limit', 'mute', 'board_mute', 'ban')),
    status TEXT NOT NULL DEFAULT 'scheduled'
        CHECK (status IN ('scheduled', 'active', 'expired', 'revoked')),
    reason TEXT,
    starts_at INTEGER NOT NULL,
    ends_at INTEGER,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    revoked_at INTEGER,
    revoked_by TEXT,
    revoke_reason TEXT,
    CHECK ((kind = 'board_mute' AND board_id IS NOT NULL)
        OR (kind != 'board_mute' AND board_id IS NULL)),
    CHECK (ends_at IS NULL OR ends_at > starts_at),
    CHECK (status != 'revoked' OR (revoked_at IS NOT NULL AND revoked_by IS NOT NULL)),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (board_id) REFERENCES boards (id) ON DELETE CASCADE,
    FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE RESTRICT,
    FOREIGN KEY (revoked_by) REFERENCES users (id) ON DELETE SET NULL
);

CREATE INDEX sanctions_user_status_idx ON sanctions (user_id, status);
CREATE INDEX sanctions_board_idx ON sanctions (board_id);
CREATE INDEX sanctions_ends_at_idx ON sanctions (ends_at);

CREATE TABLE sanction_reversals (
    id TEXT PRIMARY KEY NOT NULL,
    sanction_id TEXT NOT NULL,
    reversed_by TEXT NOT NULL,
    reason TEXT NOT NULL,
    reversed_at INTEGER NOT NULL,
    UNIQUE (sanction_id),
    FOREIGN KEY (sanction_id) REFERENCES sanctions (id) ON DELETE CASCADE,
    FOREIGN KEY (reversed_by) REFERENCES users (id) ON DELETE RESTRICT
);
