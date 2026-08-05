-- BBLBB 幂等记录表（M01-AUDIT-03）
-- scope/key 唯一；request_hash 为请求摘要（SHA-256 hex）；
-- status 记录执行进度；response_reference 指向已存储的响应/结果；
-- expires_at 控制保留窗口。

CREATE TABLE idempotency_records (
    id TEXT PRIMARY KEY NOT NULL,
    scope TEXT NOT NULL,
    key TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'in_progress'
        CHECK (status IN ('in_progress', 'completed', 'failed')),
    response_reference TEXT,
    expires_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE (scope, key)
);

CREATE INDEX idempotency_expiry_idx ON idempotency_records (expires_at);
