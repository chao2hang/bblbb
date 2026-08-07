-- BBLBB 审核动作与修订历史（M05-SCHEMA-02，SQLite）
--
-- moderation_actions：案件/内容上的一次性审核动作——action 封闭枚举，
-- target 多态（target_type + target_id）；只追加不覆盖（行不可变，
-- 修订一律写入 moderation_action_revisions）。
--
-- moderation_action_revisions：不可变修订快照（只追加）——每次修订
-- (action_id, revision) 唯一且 revision 严格递增（模型层校验）；
-- snapshot_json 为修订时动作行的完整快照（含 correction 语义），
-- change_reason 记录变更原因。

CREATE TABLE moderation_actions (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT,
    actor_id TEXT NOT NULL,
    action TEXT NOT NULL
        CHECK (action IN ('escalate', 'assign', 'resolve', 'reject', 'reopen',
                          'hide_content', 'restore_content', 'delete_content',
                          'issue_sanction', 'revoke_sanction',
                          'merge_cases', 'remove_report')),
    target_type TEXT
        CHECK (target_type IN ('post', 'comment', 'user', 'report', 'case', 'sanction')),
    target_id TEXT,
    reason TEXT,
    metadata_json TEXT,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (case_id) REFERENCES moderation_cases (id) ON DELETE CASCADE,
    FOREIGN KEY (actor_id) REFERENCES users (id) ON DELETE RESTRICT
);

CREATE INDEX moderation_actions_case_idx ON moderation_actions (case_id, created_at);
CREATE INDEX moderation_actions_actor_idx ON moderation_actions (actor_id, created_at);
CREATE INDEX moderation_actions_target_idx ON moderation_actions (target_type, target_id);

CREATE TABLE moderation_action_revisions (
    id TEXT PRIMARY KEY NOT NULL,
    action_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    snapshot_json TEXT NOT NULL,
    change_reason TEXT,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (action_id, revision),
    FOREIGN KEY (action_id) REFERENCES moderation_actions (id) ON DELETE CASCADE,
    FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE RESTRICT
);

CREATE INDEX moderation_action_revisions_action_idx ON moderation_action_revisions (action_id, revision);
