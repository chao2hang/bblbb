-- BBLBB third-party Marketplace schema (M12-SCHEMA, SQLite)
--
-- 平台托管双边站内账本（docs/MARKETPLACE-ACCOUNTING.md）：
-- - marketplace_clients：Marketplace 应用登记（指向 oauth_clients 的
--   Confidential Client；secret 只存 hash；webhook URL 与 redirect URI JSON；
--   approval history 版本化；status 含 emergency_disabled）。
-- - client_scopes：逐应用 × 逐 scope 审批（status pending/approved/disabled、
--   限额 JSON、version、effective_at、审批/撤销审计）。
-- - marketplace_merchant_accounts：商户 available/pending/frozen 余额，
--   (client_id, currency_id) 唯一；所有余额非负。
-- - offers + offer_versions：服务端登记的报价快照（金额/货币/库存/平台费/
--   收款 Client 全部由 BBLBB 保存；版本化不可变历史）。
-- - checkout_intents：短 TTL、一次性、用户绑定的结账快照；
--   (client_id, merchant_order_id) 与幂等 (scope,key) 唯一。
-- - purchases：成功购买事实记录；intent 唯一；金额/费用/商户净额快照；
--   point_operation_id + merchant_operation_id 链接不可变账本。
-- - refunds：只追加的退款请求/处理记录；累计退款由服务端锁内校验。
-- - webhook_deliveries：Outbox 提交后投递记录（HMAC 签名、退避、dead-letter）。
-- - reconciliation_records：增量对账记录与差异分类。

CREATE TABLE marketplace_clients (
    id TEXT PRIMARY KEY NOT NULL,
    client_id TEXT NOT NULL,
    oauth_client_id TEXT NOT NULL,
    owner_user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    terms_url TEXT NOT NULL,
    privacy_url TEXT NOT NULL,
    webhook_url TEXT NULL,
    webhook_secret_hash TEXT NULL,
    webhook_secret_version INTEGER NOT NULL DEFAULT 0,
    redirect_uris_json TEXT NOT NULL,
    fee_bps INTEGER NOT NULL DEFAULT 0,
    version INTEGER NOT NULL DEFAULT 1,
    approval_history_json TEXT NOT NULL,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_by TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    CONSTRAINT marketplace_clients_client_id_uq UNIQUE (client_id),
    CONSTRAINT marketplace_clients_oauth_uq UNIQUE (oauth_client_id),
    CONSTRAINT marketplace_clients_status_ck CHECK (status IN ('pending', 'active', 'disabled', 'emergency_disabled')),
    CONSTRAINT marketplace_clients_owner_fk FOREIGN KEY (owner_user_id) REFERENCES users (id) ON DELETE RESTRICT,
    CONSTRAINT marketplace_clients_oauth_fk FOREIGN KEY (oauth_client_id) REFERENCES oauth_clients (id) ON DELETE RESTRICT,
    CONSTRAINT marketplace_clients_fee_ck CHECK (fee_bps >= 0 AND fee_bps <= 10000),
    CONSTRAINT marketplace_clients_webhook_https_ck CHECK (webhook_url IS NULL OR webhook_url LIKE 'https://%'),
    CONSTRAINT marketplace_clients_terms_https_ck CHECK (terms_url LIKE 'https://%'),
    CONSTRAINT marketplace_clients_privacy_https_ck CHECK (privacy_url LIKE 'https://%')
);

CREATE INDEX marketplace_clients_owner_idx ON marketplace_clients (owner_user_id, status);

CREATE TABLE client_scopes (
    id TEXT PRIMARY KEY NOT NULL,
    client_id TEXT NOT NULL,
    scope TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    limits_json TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    effective_at INTEGER NOT NULL DEFAULT 0,
    approved_by TEXT NULL,
    approved_at INTEGER NULL,
    revoke_reason TEXT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CONSTRAINT client_scopes_client_scope_uq UNIQUE (client_id, scope),
    CONSTRAINT client_scopes_status_ck CHECK (status IN ('pending', 'approved', 'disabled')),
    CONSTRAINT client_scopes_client_fk FOREIGN KEY (client_id) REFERENCES marketplace_clients (id) ON DELETE CASCADE,
    CONSTRAINT client_scopes_approved_by_fk FOREIGN KEY (approved_by) REFERENCES users (id) ON DELETE SET NULL
);

CREATE INDEX client_scopes_client_status_idx ON client_scopes (client_id, status);

CREATE TABLE marketplace_merchant_accounts (
    id TEXT PRIMARY KEY NOT NULL,
    client_id TEXT NOT NULL,
    owner_user_id TEXT NOT NULL,
    currency_id TEXT NOT NULL,
    available_balance INTEGER NOT NULL DEFAULT 0,
    pending_balance INTEGER NOT NULL DEFAULT 0,
    frozen_balance INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'active',
    version INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CONSTRAINT merchant_accounts_client_currency_uq UNIQUE (client_id, currency_id),
    CONSTRAINT merchant_accounts_status_ck CHECK (status IN ('active', 'frozen')),
    CONSTRAINT merchant_accounts_balance_ck CHECK (available_balance >= 0 AND pending_balance >= 0 AND frozen_balance >= 0),
    CONSTRAINT merchant_accounts_client_fk FOREIGN KEY (client_id) REFERENCES marketplace_clients (id) ON DELETE RESTRICT,
    CONSTRAINT merchant_accounts_owner_fk FOREIGN KEY (owner_user_id) REFERENCES users (id) ON DELETE RESTRICT,
    CONSTRAINT merchant_accounts_currency_fk FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT
);

CREATE INDEX merchant_accounts_owner_idx ON marketplace_merchant_accounts (owner_user_id);

CREATE TABLE offers (
    id TEXT PRIMARY KEY NOT NULL,
    client_id TEXT NOT NULL,
    external_offer_id TEXT NOT NULL,
    title TEXT NOT NULL,
    description_safe TEXT NULL,
    currency_id TEXT NOT NULL,
    amount INTEGER NOT NULL,
    quantity_min INTEGER NOT NULL DEFAULT 1,
    quantity_max INTEGER NOT NULL DEFAULT 1,
    stock_policy TEXT NOT NULL DEFAULT 'unlimited',
    stock_remaining INTEGER NULL,
    status TEXT NOT NULL DEFAULT 'draft',
    fee_bps INTEGER NOT NULL DEFAULT 0,
    version INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CONSTRAINT offers_client_external_uq UNIQUE (client_id, external_offer_id),
    CONSTRAINT offers_status_ck CHECK (status IN ('draft', 'active', 'paused', 'disabled')),
    CONSTRAINT offers_stock_policy_ck CHECK (stock_policy IN ('unlimited', 'finite')),
    CONSTRAINT offers_client_fk FOREIGN KEY (client_id) REFERENCES marketplace_clients (id) ON DELETE RESTRICT,
    CONSTRAINT offers_currency_fk FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT,
    CONSTRAINT offers_amount_ck CHECK (amount >= 0),
    CONSTRAINT offers_quantity_ck CHECK (quantity_min >= 1 AND quantity_max >= 1 AND quantity_max >= quantity_min),
    CONSTRAINT offers_stock_ck CHECK (stock_remaining IS NULL OR stock_remaining >= 0),
    CONSTRAINT offers_fee_ck CHECK (fee_bps >= 0 AND fee_bps <= 10000)
);

CREATE INDEX offers_client_status_idx ON offers (client_id, status, updated_at);

CREATE TABLE offer_versions (
    id TEXT PRIMARY KEY NOT NULL,
    offer_id TEXT NOT NULL,
    version INTEGER NOT NULL,
    title TEXT NOT NULL,
    description_safe TEXT NULL,
    currency_id TEXT NOT NULL,
    amount INTEGER NOT NULL,
    quantity_min INTEGER NOT NULL DEFAULT 1,
    quantity_max INTEGER NOT NULL DEFAULT 1,
    stock_policy TEXT NOT NULL,
    stock_remaining INTEGER NULL,
    status TEXT NOT NULL,
    fee_bps INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    CONSTRAINT offer_versions_offer_version_uq UNIQUE (offer_id, version),
    CONSTRAINT offer_versions_offer_fk FOREIGN KEY (offer_id) REFERENCES offers (id) ON DELETE RESTRICT,
    CONSTRAINT offer_versions_amount_ck CHECK (amount >= 0),
    CONSTRAINT offer_versions_quantity_ck CHECK (quantity_min >= 1 AND quantity_max >= 1 AND quantity_max >= quantity_min),
    CONSTRAINT offer_versions_stock_ck CHECK (stock_remaining IS NULL OR stock_remaining >= 0),
    CONSTRAINT offer_versions_fee_ck CHECK (fee_bps >= 0 AND fee_bps <= 10000)
);

CREATE INDEX offer_versions_offer_created_idx ON offer_versions (offer_id, created_at);

CREATE TABLE checkout_intents (
    id TEXT PRIMARY KEY NOT NULL,
    client_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    offer_id TEXT NOT NULL,
    offer_version INTEGER NOT NULL,
    quantity INTEGER NOT NULL,
    amount INTEGER NOT NULL,
    fee_refundable INTEGER NOT NULL DEFAULT 1,
    currency_id TEXT NOT NULL,
    merchant_order_id TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    consumed_at INTEGER NULL,
    version INTEGER NOT NULL DEFAULT 1,
    idempotency_scope TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    CONSTRAINT intents_client_order_uq UNIQUE (client_id, merchant_order_id),
    CONSTRAINT intents_idem_uq UNIQUE (idempotency_scope, idempotency_key),
    CONSTRAINT intents_status_ck CHECK (status IN ('pending', 'consumed', 'denied', 'expired')),
    CONSTRAINT intents_client_fk FOREIGN KEY (client_id) REFERENCES marketplace_clients (id) ON DELETE RESTRICT,
    CONSTRAINT intents_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE RESTRICT,
    CONSTRAINT intents_offer_fk FOREIGN KEY (offer_id) REFERENCES offers (id) ON DELETE RESTRICT,
    CONSTRAINT intents_currency_fk FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT,
    CONSTRAINT intents_amount_ck CHECK (amount >= 0),
    CONSTRAINT intents_quantity_ck CHECK (quantity >= 1)
);

CREATE INDEX intents_user_status_idx ON checkout_intents (user_id, status, expires_at);
CREATE INDEX intents_client_created_idx ON checkout_intents (client_id, created_at);
CREATE INDEX intents_expiry_idx ON checkout_intents (expires_at);

CREATE TABLE purchases (
    id TEXT PRIMARY KEY NOT NULL,
    intent_id TEXT NOT NULL,
    client_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    offer_id TEXT NOT NULL,
    offer_version INTEGER NOT NULL,
    quantity INTEGER NOT NULL,
    amount INTEGER NOT NULL,
    fee_amount INTEGER NOT NULL DEFAULT 0,
    merchant_net INTEGER NOT NULL,
    currency_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'succeeded',
    refunded_amount INTEGER NOT NULL DEFAULT 0,
    point_operation_id TEXT NOT NULL,
    merchant_operation_id TEXT NOT NULL,
    fee_operation_id TEXT NULL,
    merchant_order_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
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
);

CREATE INDEX purchases_user_created_idx ON purchases (user_id, created_at);
CREATE INDEX purchases_client_created_idx ON purchases (client_id, created_at);

CREATE TABLE refunds (
    id TEXT PRIMARY KEY NOT NULL,
    purchase_id TEXT NOT NULL,
    client_id TEXT NOT NULL,
    amount INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'requested',
    reason_code TEXT NOT NULL,
    reason TEXT NULL,
    merchant_refund_id TEXT NOT NULL,
    reversal_operation_id TEXT NULL,
    refunded_by TEXT NOT NULL,
    refunded_by_type TEXT NOT NULL DEFAULT 'client',
    idempotency_scope TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    processed_at INTEGER NULL,
    CONSTRAINT refunds_idem_uq UNIQUE (idempotency_scope, idempotency_key),
    CONSTRAINT refunds_client_refund_uq UNIQUE (client_id, merchant_refund_id),
    CONSTRAINT refunds_status_ck CHECK (status IN ('requested', 'processed')),
    CONSTRAINT refunds_by_type_ck CHECK (refunded_by_type IN ('client', 'admin')),
    CONSTRAINT refunds_purchase_fk FOREIGN KEY (purchase_id) REFERENCES purchases (id) ON DELETE RESTRICT,
    CONSTRAINT refunds_client_fk FOREIGN KEY (client_id) REFERENCES marketplace_clients (id) ON DELETE RESTRICT,
    CONSTRAINT refunds_amount_ck CHECK (amount > 0)
);

CREATE INDEX refunds_purchase_idx ON refunds (purchase_id, created_at);
CREATE INDEX refunds_client_created_idx ON refunds (client_id, created_at);

CREATE TABLE webhook_deliveries (
    id TEXT PRIMARY KEY NOT NULL,
    event_id TEXT NOT NULL,
    client_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 5,
    next_retry_at INTEGER NOT NULL,
    last_status_code INTEGER NULL,
    last_error TEXT NULL,
    delivered_at INTEGER NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CONSTRAINT webhook_deliveries_client_event_uq UNIQUE (client_id, event_id),
    CONSTRAINT webhook_deliveries_status_ck CHECK (status IN ('pending', 'sent', 'failed', 'dead_letter')),
    CONSTRAINT webhook_deliveries_client_fk FOREIGN KEY (client_id) REFERENCES marketplace_clients (id) ON DELETE RESTRICT,
    CONSTRAINT webhook_deliveries_attempts_ck CHECK (attempts >= 0 AND max_attempts >= 1)
);

CREATE INDEX webhook_deliveries_pending_idx ON webhook_deliveries (status, next_retry_at);

CREATE TABLE reconciliation_records (
    id TEXT PRIMARY KEY NOT NULL,
    client_id TEXT NOT NULL,
    after_cursor INTEGER NOT NULL,
    purchases_count INTEGER NOT NULL,
    amount_sum INTEGER NOT NULL,
    fee_sum INTEGER NOT NULL,
    ledger_delta_sum INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'consistent',
    diffs_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    CONSTRAINT reconciliation_status_ck CHECK (status IN ('consistent', 'diff_found')),
    CONSTRAINT reconciliation_client_fk FOREIGN KEY (client_id) REFERENCES marketplace_clients (id) ON DELETE RESTRICT,
    CONSTRAINT reconciliation_counts_ck CHECK (purchases_count >= 0)
);

CREATE INDEX reconciliation_client_created_idx ON reconciliation_records (client_id, created_at);
