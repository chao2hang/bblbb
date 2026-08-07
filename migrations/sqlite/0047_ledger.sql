-- BBLBB ledger core (M07-LEDGER-01/02, SQLite)
--
-- 与 mysql/mariadb 同版本同结构：currencies（exp/coin，整数最小单位）、
-- point_accounts（复合主键，余额非负 CHECK，version 乐观并发）、
-- point_operations（只追加，幂等键唯一 + request_hash 冲突检测）、
-- point_transactions（不可变流水，含余额快照）与 point_balance_snapshots。

CREATE TABLE currencies (
    id TEXT PRIMARY KEY NOT NULL,
    code TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL
        CHECK (kind IN ('experience', 'spendable', 'reputation')),
    allow_negative INTEGER NOT NULL DEFAULT 0,
    is_enabled INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

INSERT INTO currencies (id, code, name, kind, allow_negative, is_enabled, created_at, updated_at)
VALUES
    ('01911fd5-0047-0000-0000-000000000001', 'exp', '经验', 'experience', 0, 1, 1722816000, 1722816000),
    ('01911fd5-0047-0000-0000-000000000002', 'coin', '金币', 'spendable', 0, 1, 1722816000, 1722816000);

CREATE TABLE point_accounts (
    user_id TEXT NOT NULL,
    currency_id TEXT NOT NULL,
    balance INTEGER NOT NULL DEFAULT 0,
    frozen_balance INTEGER NOT NULL DEFAULT 0,
    version INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, currency_id),
    CHECK (balance >= 0),
    CHECK (frozen_balance >= 0),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT
);

CREATE INDEX point_accounts_currency_idx ON point_accounts (currency_id);

CREATE TABLE point_operations (
    id TEXT PRIMARY KEY NOT NULL,
    idempotency_scope TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    kind TEXT NOT NULL
        CHECK (kind IN ('award', 'consume', 'shop_purchase', 'transfer', 'freeze', 'unfreeze', 'adjust', 'reversal')),
    actor_id TEXT,
    source_type TEXT,
    source_id TEXT,
    reverses_operation_id TEXT,
    memo TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (idempotency_scope, idempotency_key),
    FOREIGN KEY (actor_id) REFERENCES users (id) ON DELETE SET NULL,
    FOREIGN KEY (reverses_operation_id) REFERENCES point_operations (id) ON DELETE SET NULL
);

CREATE INDEX point_operations_source_idx ON point_operations (source_type, source_id);
CREATE INDEX point_operations_reverses_idx ON point_operations (reverses_operation_id);

CREATE TABLE point_transactions (
    id TEXT PRIMARY KEY NOT NULL,
    operation_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    currency_id TEXT NOT NULL,
    delta_balance INTEGER NOT NULL,
    delta_frozen INTEGER NOT NULL,
    balance_after INTEGER NOT NULL,
    frozen_after INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (operation_id) REFERENCES point_operations (id) ON DELETE RESTRICT,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT
);

CREATE INDEX point_transactions_operation_idx ON point_transactions (operation_id);
CREATE INDEX point_transactions_user_idx ON point_transactions (user_id, currency_id, created_at);

CREATE TABLE point_balance_snapshots (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    currency_id TEXT NOT NULL,
    balance INTEGER NOT NULL,
    frozen_balance INTEGER NOT NULL,
    snapshot_at INTEGER NOT NULL,
    reason TEXT NOT NULL,
    CHECK (balance >= 0),
    CHECK (frozen_balance >= 0),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT
);

CREATE INDEX point_balance_snapshots_lookup_idx ON point_balance_snapshots (user_id, currency_id, snapshot_at);
