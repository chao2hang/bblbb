-- BBLBB third-party Marketplace schema (M12-SCHEMA, MariaDB)
--
-- Platform-hosted bilateral in-site ledger (docs/MARKETPLACE-ACCOUNTING.md).
-- Table-for-table equivalent to migrations/sqlite/0056_marketplace.sql:
-- - marketplace_clients: Marketplace app registration (points at the OAuth
--   confidential client; secret hash only; webhook URL + redirect URI JSON;
--   versioned approval history; status includes emergency_disabled).
-- - client_scopes: per-app x per-scope approvals (pending/approved/disabled,
--   limits JSON, version, effective_at, approval/revoke audit).
-- - marketplace_merchant_accounts: merchant available/pending/frozen balance,
--   UNIQUE (client_id, currency_id); all balances non-negative.
-- - offers + offer_versions: server-registered offer snapshots (amount/currency/
--   stock/platform fee/recipient client all saved by BBLBB; versioned history).
-- - checkout_intents: short-TTL, one-shot, user-bound checkout snapshot;
--   UNIQUE (client_id, merchant_order_id) and UNIQUE (idempotency scope,key).
-- - purchases: committed purchase facts; UNIQUE intent; amount/fee/net snapshot;
--   point_operation_id + merchant_operation_id link the immutable ledger.
-- - refunds: append-only refund requests/processing; cumulative cap enforced
--   by service under lock.
-- - webhook_deliveries: post-commit delivery records (HMAC signing, backoff,
--   dead-letter).
-- - reconciliation_records: incremental reconciliation records + diff classes.

CREATE TABLE marketplace_clients (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    client_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    oauth_client_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    owner_user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    name VARCHAR(120) NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    terms_url VARCHAR(1024) NOT NULL,
    privacy_url VARCHAR(1024) NOT NULL,
    webhook_url VARCHAR(1024) NULL,
    webhook_secret_hash VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    webhook_secret_version INT NOT NULL DEFAULT 0,
    redirect_uris_json TEXT NOT NULL,
    fee_bps INT NOT NULL DEFAULT 0,
    version INT NOT NULL DEFAULT 1,
    approval_history_json TEXT NOT NULL,
    created_by CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at BIGINT NOT NULL,
    updated_by CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT marketplace_clients_client_id_uq UNIQUE (client_id),
    CONSTRAINT marketplace_clients_oauth_uq UNIQUE (oauth_client_id),
    CONSTRAINT marketplace_clients_status_ck CHECK (status IN ('pending', 'active', 'disabled', 'emergency_disabled')),
    CONSTRAINT marketplace_clients_owner_fk FOREIGN KEY (owner_user_id) REFERENCES users (id) ON DELETE RESTRICT,
    CONSTRAINT marketplace_clients_oauth_fk FOREIGN KEY (oauth_client_id) REFERENCES oauth_clients (id) ON DELETE RESTRICT,
    CONSTRAINT marketplace_clients_fee_ck CHECK (fee_bps >= 0 AND fee_bps <= 10000),
    CONSTRAINT marketplace_clients_webhook_https_ck CHECK (webhook_url IS NULL OR webhook_url LIKE 'https://%'),
    CONSTRAINT marketplace_clients_terms_https_ck CHECK (terms_url LIKE 'https://%'),
    CONSTRAINT marketplace_clients_privacy_https_ck CHECK (privacy_url LIKE 'https://%')
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX marketplace_clients_owner_idx ON marketplace_clients (owner_user_id, status);

CREATE TABLE client_scopes (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    client_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    scope VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    limits_json TEXT NOT NULL,
    version INT NOT NULL DEFAULT 1,
    effective_at BIGINT NOT NULL DEFAULT 0,
    approved_by CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    approved_at BIGINT NULL,
    revoke_reason TEXT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT client_scopes_client_scope_uq UNIQUE (client_id, scope),
    CONSTRAINT client_scopes_status_ck CHECK (status IN ('pending', 'approved', 'disabled')),
    CONSTRAINT client_scopes_client_fk FOREIGN KEY (client_id) REFERENCES marketplace_clients (id) ON DELETE CASCADE,
    CONSTRAINT client_scopes_approved_by_fk FOREIGN KEY (approved_by) REFERENCES users (id) ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX client_scopes_client_status_idx ON client_scopes (client_id, status);

CREATE TABLE marketplace_merchant_accounts (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    client_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    owner_user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    currency_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    available_balance BIGINT NOT NULL DEFAULT 0,
    pending_balance BIGINT NOT NULL DEFAULT 0,
    frozen_balance BIGINT NOT NULL DEFAULT 0,
    status VARCHAR(32) NOT NULL DEFAULT 'active',
    version INT NOT NULL DEFAULT 1,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT merchant_accounts_client_currency_uq UNIQUE (client_id, currency_id),
    CONSTRAINT merchant_accounts_status_ck CHECK (status IN ('active', 'frozen')),
    CONSTRAINT merchant_accounts_balance_ck CHECK (available_balance >= 0 AND pending_balance >= 0 AND frozen_balance >= 0),
    CONSTRAINT merchant_accounts_client_fk FOREIGN KEY (client_id) REFERENCES marketplace_clients (id) ON DELETE RESTRICT,
    CONSTRAINT merchant_accounts_owner_fk FOREIGN KEY (owner_user_id) REFERENCES users (id) ON DELETE RESTRICT,
    CONSTRAINT merchant_accounts_currency_fk FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX merchant_accounts_owner_idx ON marketplace_merchant_accounts (owner_user_id);

CREATE TABLE offers (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    client_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    external_offer_id VARCHAR(128) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    title VARCHAR(120) NOT NULL,
    description_safe TEXT NULL,
    currency_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    amount BIGINT NOT NULL,
    quantity_min INT NOT NULL DEFAULT 1,
    quantity_max INT NOT NULL DEFAULT 1,
    stock_policy VARCHAR(32) NOT NULL DEFAULT 'unlimited',
    stock_remaining INT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'draft',
    fee_bps INT NOT NULL DEFAULT 0,
    version INT NOT NULL DEFAULT 1,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT offers_client_external_uq UNIQUE (client_id, external_offer_id),
    CONSTRAINT offers_status_ck CHECK (status IN ('draft', 'active', 'paused', 'disabled')),
    CONSTRAINT offers_stock_policy_ck CHECK (stock_policy IN ('unlimited', 'finite')),
    CONSTRAINT offers_client_fk FOREIGN KEY (client_id) REFERENCES marketplace_clients (id) ON DELETE RESTRICT,
    CONSTRAINT offers_currency_fk FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT,
    CONSTRAINT offers_amount_ck CHECK (amount >= 0),
    CONSTRAINT offers_quantity_ck CHECK (quantity_min >= 1 AND quantity_max >= 1 AND quantity_max >= quantity_min),
    CONSTRAINT offers_stock_ck CHECK (stock_remaining IS NULL OR stock_remaining >= 0),
    CONSTRAINT offers_fee_ck CHECK (fee_bps >= 0 AND fee_bps <= 10000)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX offers_client_status_idx ON offers (client_id, status, updated_at);

CREATE TABLE offer_versions (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    offer_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    version INT NOT NULL,
    title VARCHAR(120) NOT NULL,
    description_safe TEXT NULL,
    currency_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    amount BIGINT NOT NULL,
    quantity_min INT NOT NULL DEFAULT 1,
    quantity_max INT NOT NULL DEFAULT 1,
    stock_policy VARCHAR(32) NOT NULL,
    stock_remaining INT NULL,
    status VARCHAR(32) NOT NULL,
    fee_bps INT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT offer_versions_offer_version_uq UNIQUE (offer_id, version),
    CONSTRAINT offer_versions_offer_fk FOREIGN KEY (offer_id) REFERENCES offers (id) ON DELETE RESTRICT,
    CONSTRAINT offer_versions_amount_ck CHECK (amount >= 0),
    CONSTRAINT offer_versions_quantity_ck CHECK (quantity_min >= 1 AND quantity_max >= 1 AND quantity_max >= quantity_min),
    CONSTRAINT offer_versions_stock_ck CHECK (stock_remaining IS NULL OR stock_remaining >= 0),
    CONSTRAINT offer_versions_fee_ck CHECK (fee_bps >= 0 AND fee_bps <= 10000)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX offer_versions_offer_created_idx ON offer_versions (offer_id, created_at);

CREATE TABLE checkout_intents (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    client_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    offer_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    offer_version INT NOT NULL,
    quantity INT NOT NULL,
    amount BIGINT NOT NULL,
    fee_refundable TINYINT NOT NULL DEFAULT 1,
    currency_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    merchant_order_id VARCHAR(128) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    request_hash VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    expires_at BIGINT NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    consumed_at BIGINT NULL,
    version INT NOT NULL DEFAULT 1,
    idempotency_scope VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    idempotency_key VARCHAR(200) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT intents_client_order_uq UNIQUE (client_id, merchant_order_id),
    CONSTRAINT intents_idem_uq UNIQUE (idempotency_scope, idempotency_key),
    CONSTRAINT intents_status_ck CHECK (status IN ('pending', 'consumed', 'denied', 'expired')),
    CONSTRAINT intents_client_fk FOREIGN KEY (client_id) REFERENCES marketplace_clients (id) ON DELETE RESTRICT,
    CONSTRAINT intents_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE RESTRICT,
    CONSTRAINT intents_offer_fk FOREIGN KEY (offer_id) REFERENCES offers (id) ON DELETE RESTRICT,
    CONSTRAINT intents_currency_fk FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT,
    CONSTRAINT intents_amount_ck CHECK (amount >= 0),
    CONSTRAINT intents_quantity_ck CHECK (quantity >= 1)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX intents_user_status_idx ON checkout_intents (user_id, status, expires_at);
CREATE INDEX intents_client_created_idx ON checkout_intents (client_id, created_at);
CREATE INDEX intents_expiry_idx ON checkout_intents (expires_at);

CREATE TABLE purchases (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    intent_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    client_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    offer_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    offer_version INT NOT NULL,
    quantity INT NOT NULL,
    amount BIGINT NOT NULL,
    fee_amount BIGINT NOT NULL DEFAULT 0,
    merchant_net BIGINT NOT NULL,
    currency_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'succeeded',
    refunded_amount BIGINT NOT NULL DEFAULT 0,
    point_operation_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    merchant_operation_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    fee_operation_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    merchant_order_id VARCHAR(128) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT purchases_intent_uq UNIQUE (intent_id),
    CONSTRAINT purchases_op_uq UNIQUE (point_operation_id),
    CONSTRAINT purchases_merchant_op_uq UNIQUE (merchant_operation_id),
    CONSTRAINT purchases_client_order_uq UNIQUE (client_id, merchant_order_id),
    CONSTRAINT purchases_status_ck CHECK (status IN ('succeeded', 'refunded', 'partially_refunded')),
    CONSTRAINT purchases_intent_fk FOREIGN KEY (intent_id) REFERENCES checkout_intents (id) ON DELETE RESTRICT,
    CONSTRAINT purchases_client_fk FOREIGN KEY (client_id) REFERENCES marketplace_clients (id) ON DELETE RESTRICT,
    CONSTRAINT purchases_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE RESTRICT,
    CONSTRAINT purchases_offer_fk FOREIGN KEY (offer_id) REFERENCES offers (id) ON DELETE RESTRICT,
    CONSTRAINT purchases_currency_fk FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT,
    CONSTRAINT purchases_amount_ck CHECK (amount >= 0 AND fee_amount >= 0 AND merchant_net >= 0),
    CONSTRAINT purchases_refund_cap_ck CHECK (refunded_amount >= 0 AND refunded_amount <= amount),
    CONSTRAINT purchases_quantity_ck CHECK (quantity >= 1)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX purchases_user_created_idx ON purchases (user_id, created_at);
CREATE INDEX purchases_client_created_idx ON purchases (client_id, created_at);

CREATE TABLE refunds (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    purchase_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    client_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    amount BIGINT NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'requested',
    reason_code VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    reason TEXT NULL,
    merchant_refund_id VARCHAR(128) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    reversal_operation_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    refunded_by CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    refunded_by_type VARCHAR(32) NOT NULL DEFAULT 'client',
    idempotency_scope VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    idempotency_key VARCHAR(200) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at BIGINT NOT NULL,
    processed_at BIGINT NULL,
    PRIMARY KEY (id),
    CONSTRAINT refunds_idem_uq UNIQUE (idempotency_scope, idempotency_key),
    CONSTRAINT refunds_client_refund_uq UNIQUE (client_id, merchant_refund_id),
    CONSTRAINT refunds_status_ck CHECK (status IN ('requested', 'processed')),
    CONSTRAINT refunds_by_type_ck CHECK (refunded_by_type IN ('client', 'admin')),
    CONSTRAINT refunds_purchase_fk FOREIGN KEY (purchase_id) REFERENCES purchases (id) ON DELETE RESTRICT,
    CONSTRAINT refunds_client_fk FOREIGN KEY (client_id) REFERENCES marketplace_clients (id) ON DELETE RESTRICT,
    CONSTRAINT refunds_amount_ck CHECK (amount > 0)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX refunds_purchase_idx ON refunds (purchase_id, created_at);
CREATE INDEX refunds_client_created_idx ON refunds (client_id, created_at);

CREATE TABLE webhook_deliveries (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    event_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    client_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    event_type VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    payload TEXT NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    attempts INT NOT NULL DEFAULT 0,
    max_attempts INT NOT NULL DEFAULT 5,
    next_retry_at BIGINT NOT NULL,
    last_status_code INT NULL,
    last_error TEXT NULL,
    delivered_at BIGINT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT webhook_deliveries_client_event_uq UNIQUE (client_id, event_id),
    CONSTRAINT webhook_deliveries_status_ck CHECK (status IN ('pending', 'sent', 'failed', 'dead_letter')),
    CONSTRAINT webhook_deliveries_client_fk FOREIGN KEY (client_id) REFERENCES marketplace_clients (id) ON DELETE RESTRICT,
    CONSTRAINT webhook_deliveries_attempts_ck CHECK (attempts >= 0 AND max_attempts >= 1)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX webhook_deliveries_pending_idx ON webhook_deliveries (status, next_retry_at);

CREATE TABLE reconciliation_records (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    client_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    after_cursor BIGINT NOT NULL,
    purchases_count INT NOT NULL,
    amount_sum BIGINT NOT NULL,
    fee_sum BIGINT NOT NULL,
    ledger_delta_sum BIGINT NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'consistent',
    diffs_json TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT reconciliation_status_ck CHECK (status IN ('consistent', 'diff_found')),
    CONSTRAINT reconciliation_client_fk FOREIGN KEY (client_id) REFERENCES marketplace_clients (id) ON DELETE RESTRICT,
    CONSTRAINT reconciliation_counts_ck CHECK (purchases_count >= 0)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX reconciliation_client_created_idx ON reconciliation_records (client_id, created_at);
