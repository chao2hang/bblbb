-- BBLBB 审核案件与举报（M05-SCHEMA-01/06，SQLite）
--
-- reports：举报——多态目标（target_type + target_id，无单一外键），
-- reason_code 封闭枚举；状态 open/triaged/investigating/resolved/rejected/
-- reopened/withdrawn（withdrawn 仅举报端，案件不含）。
--
-- 去重（M05-SCHEMA-06）：report_dedup_key 把 (reporter_id, target_type,
-- target_id, reason_code) 归一化为单列（参考 0040 grant_target_key 手法，
-- 避免跨库 NULL 唯一性语义差异）；去重窗口锚定——dedup_until 为当前
-- 锚定窗口终点（模型层 REPORT_DEDUP_WINDOW_MS 计算），同一窗口内相同键
-- 至多一条，UNIQUE(report_dedup_key, dedup_until) 三库统一强制；
-- 窗口内重复举报由模型层按 dedup_until > now 拒绝。
--
-- moderation_cases：案件——举报聚合为单一处理线程；状态机见
-- STATE-MACHINES.md §3；priority low/normal/high/urgent。
-- case_reports：举报↔案件多对多（举报可合并进同一案件）。
-- case_assignments：案件指派历史（只追加；释放记 released_at）。
-- moderation_notes：内部备注（不随对外 API 暴露正文）。

CREATE TABLE reports (
    id TEXT PRIMARY KEY NOT NULL,
    reporter_id TEXT NOT NULL,
    target_type TEXT NOT NULL
        CHECK (target_type IN ('post', 'comment', 'user', 'board')),
    target_id TEXT NOT NULL,
    reason_code TEXT NOT NULL
        CHECK (reason_code IN ('spam', 'harassment', 'illegal', 'nsfw', 'misinformation', 'impersonation', 'other')),
    details TEXT,
    status TEXT NOT NULL DEFAULT 'open'
        CHECK (status IN ('open', 'triaged', 'investigating', 'resolved', 'rejected', 'reopened', 'withdrawn')),
    report_dedup_key TEXT NOT NULL,
    dedup_until INTEGER NOT NULL,
    assigned_to TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE (report_dedup_key, dedup_until),
    FOREIGN KEY (reporter_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (assigned_to) REFERENCES users (id) ON DELETE SET NULL
);

CREATE INDEX reports_reporter_idx ON reports (reporter_id);
CREATE INDEX reports_target_idx ON reports (target_type, target_id);
CREATE INDEX reports_status_idx ON reports (status, dedup_until);
CREATE INDEX reports_dedup_key_idx ON reports (report_dedup_key);

CREATE TABLE moderation_cases (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'open'
        CHECK (status IN ('open', 'triaged', 'investigating', 'resolved', 'rejected', 'reopened')),
    priority TEXT NOT NULL DEFAULT 'normal'
        CHECK (priority IN ('low', 'normal', 'high', 'urgent')),
    assigned_to TEXT,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    resolved_at INTEGER,
    resolution TEXT,
    FOREIGN KEY (assigned_to) REFERENCES users (id) ON DELETE SET NULL,
    FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE RESTRICT
);

CREATE INDEX moderation_cases_status_idx ON moderation_cases (status, priority);
CREATE INDEX moderation_cases_assignee_idx ON moderation_cases (assigned_to);
CREATE INDEX moderation_cases_created_idx ON moderation_cases (created_at);

CREATE TABLE case_reports (
    case_id TEXT NOT NULL,
    report_id TEXT NOT NULL,
    added_by TEXT NOT NULL,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (case_id, report_id),
    FOREIGN KEY (case_id) REFERENCES moderation_cases (id) ON DELETE CASCADE,
    FOREIGN KEY (report_id) REFERENCES reports (id) ON DELETE CASCADE,
    FOREIGN KEY (added_by) REFERENCES users (id) ON DELETE RESTRICT
);

CREATE INDEX case_reports_report_idx ON case_reports (report_id);

CREATE TABLE case_assignments (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    assignee_id TEXT NOT NULL,
    assigned_by TEXT NOT NULL,
    assigned_at INTEGER NOT NULL,
    released_at INTEGER,
    note TEXT,
    FOREIGN KEY (case_id) REFERENCES moderation_cases (id) ON DELETE CASCADE,
    FOREIGN KEY (assignee_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (assigned_by) REFERENCES users (id) ON DELETE RESTRICT
);

CREATE INDEX case_assignments_case_idx ON case_assignments (case_id, assigned_at);
CREATE INDEX case_assignments_assignee_idx ON case_assignments (assignee_id);

CREATE TABLE moderation_notes (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    author_id TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER,
    FOREIGN KEY (case_id) REFERENCES moderation_cases (id) ON DELETE CASCADE,
    FOREIGN KEY (author_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX moderation_notes_case_idx ON moderation_notes (case_id, created_at);
