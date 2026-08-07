-- BBLBB internal shop + entitlements (M07-SHOP-SCHEMA, SQLite)
--
-- 与 mysql/mariadb 同版本同结构：shop_products（版本化目录，kind/status/slug 枚举、
-- 价格快照、库存、等级门槛、销售窗口、有效期与退款策略）、shop_orders（服务端价格
-- 快照，(user_id, idempotency_key) 唯一防重复扣款，point_operation_id 链接不可变
-- 账本）、user_entitlements（owned/equipped/expired/revoked/consumed 状态机与剩余
-- 数量）和 user_presentations（衣柜投影，引用本人有效权益）。

CREATE TABLE shop_products (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft',
    slug TEXT NOT NULL,
    title TEXT NOT NULL,
    description_safe TEXT NULL,
    icon_token TEXT NULL,
    presentation_tokens_json TEXT NULL,
    slot TEXT NOT NULL,
    currency_id TEXT NOT NULL,
    unit_price INTEGER NOT NULL,
    quantity_limit INTEGER NOT NULL DEFAULT 1,
    stock_remaining INTEGER NULL,
    required_level INTEGER NOT NULL DEFAULT 1,
    validity_seconds INTEGER NULL,
    sale_start_at INTEGER NULL,
    sale_end_at INTEGER NULL,
    refund_policy TEXT NOT NULL DEFAULT 'non_refundable',
    version INTEGER NOT NULL DEFAULT 1,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CONSTRAINT shop_products_kind_ck CHECK (kind IN ('cosmetic_nickname', 'cosmetic_avatar', 'cosmetic_avatar_attachment', 'cosmetic_badge', 'profile_effect', 'post_effect', 'reaction_pack', 'title_prefix', 'utility')),
    CONSTRAINT shop_products_status_ck CHECK (status IN ('draft', 'pending_review', 'published', 'disabled', 'retired')),
    CONSTRAINT shop_products_refund_ck CHECK (refund_policy IN ('non_refundable', 'compensation_only', 'full_refund')),
    CONSTRAINT shop_products_slug_uq UNIQUE (slug),
    CONSTRAINT shop_products_currency_fk FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT,
    CONSTRAINT shop_products_price_ck CHECK (unit_price >= 0),
    CONSTRAINT shop_products_stock_ck CHECK (stock_remaining IS NULL OR stock_remaining >= 0),
    CONSTRAINT shop_products_created_by_fk FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE RESTRICT
);

CREATE INDEX shop_products_status_slot_idx ON shop_products (status, slot, sale_start_at);

CREATE TABLE shop_orders (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    product_id TEXT NOT NULL,
    product_version INTEGER NOT NULL,
    quantity INTEGER NOT NULL,
    currency_id TEXT NOT NULL,
    unit_price INTEGER NOT NULL,
    total_amount INTEGER NOT NULL,
    point_operation_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'succeeded',
    idempotency_key TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CONSTRAINT shop_orders_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE RESTRICT,
    CONSTRAINT shop_orders_product_fk FOREIGN KEY (product_id) REFERENCES shop_products (id) ON DELETE RESTRICT,
    CONSTRAINT shop_orders_currency_fk FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT,
    CONSTRAINT shop_orders_status_ck CHECK (status IN ('succeeded', 'refunded', 'partially_refunded')),
    CONSTRAINT shop_orders_user_idem_uq UNIQUE (user_id, idempotency_key),
    CONSTRAINT shop_orders_op_uq UNIQUE (point_operation_id),
    CONSTRAINT shop_orders_qty_ck CHECK (quantity > 0),
    CONSTRAINT shop_orders_amount_ck CHECK (unit_price >= 0 AND total_amount >= 0)
);

CREATE INDEX shop_orders_user_created_idx ON shop_orders (user_id, created_at);
CREATE INDEX shop_orders_product_idx ON shop_orders (product_id, product_version);

CREATE TABLE user_entitlements (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    product_id TEXT NOT NULL,
    order_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'owned',
    quantity INTEGER NOT NULL DEFAULT 1,
    remaining_quantity INTEGER NOT NULL DEFAULT 1,
    valid_from INTEGER NOT NULL,
    expires_at INTEGER NULL,
    equipped_at INTEGER NULL,
    revoked_at INTEGER NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CONSTRAINT user_entitlements_status_ck CHECK (status IN ('owned', 'equipped', 'expired', 'revoked', 'consumed')),
    CONSTRAINT user_entitlements_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT user_entitlements_product_fk FOREIGN KEY (product_id) REFERENCES shop_products (id) ON DELETE RESTRICT,
    CONSTRAINT user_entitlements_order_fk FOREIGN KEY (order_id) REFERENCES shop_orders (id) ON DELETE RESTRICT,
    CONSTRAINT user_entitlements_qty_ck CHECK (quantity >= 1 AND remaining_quantity >= 0 AND remaining_quantity <= quantity)
);

CREATE INDEX user_entitlements_user_status_idx ON user_entitlements (user_id, status, valid_from);
CREATE INDEX user_entitlements_product_idx ON user_entitlements (product_id);

CREATE TABLE user_presentations (
    user_id TEXT PRIMARY KEY NOT NULL,
    nickname_decoration_id TEXT NULL,
    nickname_color_id TEXT NULL,
    avatar_frame_id TEXT NULL,
    avatar_attachment_id TEXT NULL,
    profile_effect_id TEXT NULL,
    title_prefix_id TEXT NULL,
    profile_badge_ids_json TEXT NULL,
    post_effect_id TEXT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    updated_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    CONSTRAINT user_presentations_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);
