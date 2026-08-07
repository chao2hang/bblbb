-- BBLBB ledger core (M07-LEDGER-01/02, MySQL)
--
-- currencies: point currencies (exp/coin), integer-only, kind
--   experience/spendable/reputation, allow_negative off by default.
-- point_accounts: (user_id, currency_id) composite PK; balance/frozen_balance
--   never negative (CHECK), version for optimistic concurrency.
-- point_operations: immutable operation log; (idempotency_scope,
--   idempotency_key) UNIQUE for replay, request_hash detects same-key
--   different-payload conflicts; kind award/consume/shop_purchase/transfer/
--   freeze/unfreeze/adjust/reversal; reverses_operation_id links reversals.
-- point_transactions: append-only ledger rows; delta_balance/delta_frozen with
--   balance_after/frozen_after snapshots (initial + sum(delta) = balance).
-- point_balance_snapshots: point-in-time balance snapshots for accounting.

CREATE TABLE currencies (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    code VARCHAR(16) NOT NULL,
    name VARCHAR(64) NOT NULL,
    kind VARCHAR(16) NOT NULL,
    allow_negative TINYINT NOT NULL DEFAULT 0,
    is_enabled TINYINT NOT NULL DEFAULT 1,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY currencies_code_uq (code),
    CONSTRAINT currencies_kind_ck CHECK (kind IN ('experience', 'spendable', 'reputation'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

INSERT INTO currencies (id, code, name, kind, allow_negative, is_enabled, created_at, updated_at)
VALUES
    ('01911fd5-0047-0000-0000-000000000001', 'exp', '经验', 'experience', 0, 1, 1722816000, 1722816000),
    ('01911fd5-0047-0000-0000-000000000002', 'coin', '金币', 'spendable', 0, 1, 1722816000, 1722816000);

CREATE TABLE point_accounts (
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    currency_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    balance BIGINT NOT NULL DEFAULT 0,
    frozen_balance BIGINT NOT NULL DEFAULT 0,
    version BIGINT NOT NULL DEFAULT 0,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (user_id, currency_id),
    CONSTRAINT point_accounts_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT point_accounts_currency_fk FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT,
    CONSTRAINT point_accounts_balance_ck CHECK (balance >= 0),
    CONSTRAINT point_accounts_frozen_ck CHECK (frozen_balance >= 0)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX point_accounts_currency_idx ON point_accounts (currency_id);

CREATE TABLE point_operations (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    idempotency_scope VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    idempotency_key VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    request_hash VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    kind VARCHAR(16) NOT NULL,
    actor_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    source_type VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NULL,
    source_id VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    reverses_operation_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    memo VARCHAR(255) NOT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY point_operations_idempotency_uq (idempotency_scope, idempotency_key),
    CONSTRAINT point_operations_kind_ck CHECK (kind IN ('award', 'consume', 'shop_purchase', 'transfer', 'freeze', 'unfreeze', 'adjust', 'reversal')),
    CONSTRAINT point_operations_actor_fk FOREIGN KEY (actor_id) REFERENCES users (id) ON DELETE SET NULL,
    CONSTRAINT point_operations_reverses_fk FOREIGN KEY (reverses_operation_id) REFERENCES point_operations (id) ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX point_operations_source_idx ON point_operations (source_type, source_id);
CREATE INDEX point_operations_reverses_idx ON point_operations (reverses_operation_id);

CREATE TABLE point_transactions (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    operation_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    currency_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    delta_balance BIGINT NOT NULL,
    delta_frozen BIGINT NOT NULL,
    balance_after BIGINT NOT NULL,
    frozen_after BIGINT NOT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT point_transactions_operation_fk FOREIGN KEY (operation_id) REFERENCES point_operations (id) ON DELETE RESTRICT,
    CONSTRAINT point_transactions_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT point_transactions_currency_fk FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX point_transactions_operation_idx ON point_transactions (operation_id);
CREATE INDEX point_transactions_user_idx ON point_transactions (user_id, currency_id, created_at);

CREATE TABLE point_balance_snapshots (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    currency_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    balance BIGINT NOT NULL,
    frozen_balance BIGINT NOT NULL,
    snapshot_at BIGINT NOT NULL,
    reason VARCHAR(64) NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT point_balance_snapshots_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT point_balance_snapshots_currency_fk FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT,
    CONSTRAINT point_balance_snapshots_balance_ck CHECK (balance >= 0),
    CONSTRAINT point_balance_snapshots_frozen_ck CHECK (frozen_balance >= 0)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX point_balance_snapshots_lookup_idx ON point_balance_snapshots (user_id, currency_id, snapshot_at);
