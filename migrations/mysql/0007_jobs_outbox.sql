-- BBLBB jobs 表 + outbox 扩展（M01-JOBS-01）

ALTER TABLE outbox_events
    ADD COLUMN payload_version INT NOT NULL DEFAULT 1,
    ADD COLUMN idempotency_key VARCHAR(128) CHARACTER SET ascii COLLATE ascii_bin NULL;

CREATE UNIQUE INDEX outbox_events_idempotency_key_uq
    ON outbox_events (idempotency_key);

CREATE TABLE jobs (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    queue VARCHAR(64) NOT NULL DEFAULT 'default',
    kind VARCHAR(64) NOT NULL,
    payload MEDIUMTEXT NOT NULL,
    payload_version INT NOT NULL DEFAULT 1,
    status VARCHAR(32) NOT NULL DEFAULT 'queued',
    attempts INT NOT NULL DEFAULT 0,
    max_attempts INT NOT NULL DEFAULT 5,
    available_at BIGINT NOT NULL,
    locked_by CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    locked_until BIGINT NULL,
    deduplication_key VARCHAR(128) CHARACTER SET ascii COLLATE ascii_bin NULL,
    last_error TEXT NULL,
    completed_at BIGINT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT jobs_status_ck CHECK (status IN ('queued', 'running', 'retry_wait', 'succeeded', 'cancelled', 'dead')),
    UNIQUE KEY jobs_deduplication_key_uq (deduplication_key),
    KEY jobs_status_available_at_idx (status, available_at),
    KEY jobs_queue_status_idx (queue, status)
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_bin;
