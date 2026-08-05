-- BBLBB jobs 表 + outbox 扩展（M01-JOBS-01）
-- 1) outbox_events 增加 payload_version 与 idempotency_key（消费者去重幂等约束）
-- 2) 新建 jobs 表：状态、attempt、run_at(available_at)、lease(locked_by/until)、
--    payload version、幂等(deduplication_key 唯一)

ALTER TABLE outbox_events ADD COLUMN payload_version INTEGER NOT NULL DEFAULT 1;
ALTER TABLE outbox_events ADD COLUMN idempotency_key TEXT;

CREATE UNIQUE INDEX outbox_events_idempotency_key_uq
    ON outbox_events (idempotency_key);

CREATE TABLE jobs (
    id TEXT PRIMARY KEY NOT NULL,
    queue TEXT NOT NULL DEFAULT 'default',
    kind TEXT NOT NULL,
    payload TEXT NOT NULL,
    payload_version INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued', 'running', 'retry_wait', 'succeeded', 'cancelled', 'dead')),
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 5,
    available_at INTEGER NOT NULL,
    locked_by TEXT,
    locked_until INTEGER,
    deduplication_key TEXT,
    last_error TEXT,
    completed_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX jobs_deduplication_key_uq ON jobs (deduplication_key);
CREATE INDEX jobs_status_available_at_idx ON jobs (status, available_at);
CREATE INDEX jobs_queue_status_idx ON jobs (queue, status);
