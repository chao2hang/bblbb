-- BBLBB 幂等记录表（M01-AUDIT-03）
-- scope/`key` 唯一；request_hash 为请求摘要（SHA-256 hex）；
-- status 记录执行进度；response_reference 指向已存储的响应/结果；
-- expires_at 控制保留窗口。
-- 注意：`key` 为 MySQL/MariaDB 保留字，DDL 与查询中必须反引号转义
-- （SQLite 同样接受反引号标识符，三库语法一致）。

CREATE TABLE idempotency_records (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL PRIMARY KEY,
    scope VARCHAR(50) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    `key` VARCHAR(200) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    request_hash CHAR(64) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    status VARCHAR(20) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL DEFAULT 'in_progress',
    response_reference VARCHAR(100) NULL,
    expires_at BIGINT NOT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    UNIQUE KEY idempotency_scope_key_uq (scope, `key`),
    CONSTRAINT chk_idempotency_status
        CHECK (status IN ('in_progress', 'completed', 'failed'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX idempotency_expiry_idx ON idempotency_records (expires_at);
