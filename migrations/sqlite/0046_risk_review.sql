-- BBLBB 风险审核状态与策略（M05-RISK-01/03/08/09，SQLite）
--
-- posts.review_status：'none' 正常发布流程；'pending_review' 高风险内容
-- 待人工审核（发布时原子写入：status='draft' + review_status='pending_review'，
-- 从而天然不进公开投影——公开查询一律按 status='published'/'hidden' 过滤）。
--
-- risk_policies：版本化风险策略（M05-RISK-01/08）。每次管理员更新以
-- (id, version+1) 追加一行，UNIQUE(id, version) 保证并发版本控制
-- （同时提交同版本只有一方成功）；reason 必填并写审计（M05-RISK-08）。
-- thresholds_json 只含阈值与规则参数，不含任何内部数据。
--
-- risk_evaluations：风险评估指标（M05-RISK-09）——只记 verdict/reason
-- category/延迟/策略版本，**绝不记录正文**；reviewed_at 用于队列时长
-- （reviewed_at - created_at），false_positive 为误判反馈。

ALTER TABLE posts ADD COLUMN review_status TEXT NOT NULL DEFAULT 'none'
    CHECK (review_status IN ('none', 'pending_review'));

CREATE INDEX posts_review_status_idx ON posts (review_status);

CREATE TABLE risk_policies (
    id TEXT PRIMARY KEY NOT NULL,
    version INTEGER NOT NULL CHECK (version > 0),
    thresholds_json TEXT NOT NULL,
    reason TEXT NOT NULL,
    updated_by TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE (id, version)
);

CREATE TABLE risk_evaluations (
    id TEXT PRIMARY KEY NOT NULL,
    post_id TEXT NOT NULL,
    author_id TEXT NOT NULL,
    verdict TEXT NOT NULL
        CHECK (verdict IN ('allow', 'pending_review')),
    reason_category TEXT,
    policy_version INTEGER NOT NULL,
    latency_ms INTEGER NOT NULL,
    reviewed_at INTEGER,
    false_positive INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE,
    FOREIGN KEY (author_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX risk_evaluations_post_idx ON risk_evaluations (post_id);
CREATE INDEX risk_evaluations_created_idx ON risk_evaluations (created_at);
