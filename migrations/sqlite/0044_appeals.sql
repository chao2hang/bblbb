-- BBLBB 申诉（M05-SCHEMA-04，SQLite）
--
-- appeals：针对处罚（sanction_id）的申诉——每处罚至多一条
-- （UNIQUE(sanction_id)，拒绝后不可重复申诉，只能走新处罚的申诉）；
-- 状态 submitted/reviewing/upheld/partially_upheld/rejected/withdrawn。
--
-- appeal_decisions：决定记录（可多次追加）——reviewer_id 为审查者；
-- conflict_of_interest 利益冲突声明字段：非空表示审查者存在利益冲突
-- （如审查者即处罚签发人），模型层校验审查者不得是申诉人本人且
-- 声明冲突时必须填写理由。仅追加，不覆盖历史决定。

CREATE TABLE appeals (
    id TEXT PRIMARY KEY NOT NULL,
    sanction_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    message TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'submitted'
        CHECK (status IN ('submitted', 'reviewing', 'upheld', 'partially_upheld', 'rejected', 'withdrawn')),
    reviewed_by TEXT,
    decided_at INTEGER,
    submitted_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE (sanction_id),
    FOREIGN KEY (sanction_id) REFERENCES sanctions (id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (reviewed_by) REFERENCES users (id) ON DELETE SET NULL
);

CREATE INDEX appeals_user_idx ON appeals (user_id, status);
CREATE INDEX appeals_status_idx ON appeals (status);

CREATE TABLE appeal_decisions (
    id TEXT PRIMARY KEY NOT NULL,
    appeal_id TEXT NOT NULL,
    reviewer_id TEXT NOT NULL,
    decision TEXT NOT NULL
        CHECK (decision IN ('upheld', 'partially_upheld', 'rejected')),
    decision_note TEXT,
    conflict_of_interest TEXT,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (appeal_id) REFERENCES appeals (id) ON DELETE CASCADE,
    FOREIGN KEY (reviewer_id) REFERENCES users (id) ON DELETE RESTRICT
);

CREATE INDEX appeal_decisions_appeal_idx ON appeal_decisions (appeal_id, created_at);
