-- BBLBB internal shop + entitlements (M07-SHOP-SCHEMA, MySQL)
--
-- shop_products: versioned catalog with kind (cosmetic_nickname/.../utility),
--   status (draft/pending_review/published/disabled/retired), price snapshot
--   currency + unit_price, inventory (quantity_limit/stock_remaining), level gate
--   (required_level), sale window, validity and refund policy.
-- shop_orders: server-side price/currency snapshot + (user_id, idempotency_key)
--   UNIQUE for no double-charge replay; point_operation_id links the immutable
--   ledger operation.
-- user_entitlements: owned/equipped/expired/revoked/consumed state machine,
--   quantity + remaining_quantity for reaction packs, valid window.
-- user_presentations: per-user wardrobe projection; all ids reference the user's
--   own valid entitlements (service-enforced, server-side safe tokens only).

CREATE TABLE shop_products (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    kind VARCHAR(32) NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'draft',
    slug VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    title VARCHAR(120) NOT NULL,
    description_safe TEXT NULL,
    icon_token VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    presentation_tokens_json TEXT NULL,
    slot VARCHAR(32) NOT NULL,
    currency_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    unit_price BIGINT NOT NULL,
    quantity_limit INT NOT NULL DEFAULT 1,
    stock_remaining INT NULL,
    required_level INT NOT NULL DEFAULT 1,
    validity_seconds BIGINT NULL,
    sale_start_at BIGINT NULL,
    sale_end_at BIGINT NULL,
    refund_policy VARCHAR(32) NOT NULL DEFAULT 'non_refundable',
    version INT NOT NULL DEFAULT 1,
    created_by CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT shop_products_kind_ck CHECK (kind IN ('cosmetic_nickname', 'cosmetic_avatar', 'cosmetic_avatar_attachment', 'cosmetic_badge', 'profile_effect', 'post_effect', 'reaction_pack', 'title_prefix', 'utility')),
    CONSTRAINT shop_products_status_ck CHECK (status IN ('draft', 'pending_review', 'published', 'disabled', 'retired')),
    CONSTRAINT shop_products_refund_ck CHECK (refund_policy IN ('non_refundable', 'compensation_only', 'full_refund')),
    CONSTRAINT shop_products_slug_uq UNIQUE (slug),
    CONSTRAINT shop_products_currency_fk FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT,
    CONSTRAINT shop_products_price_ck CHECK (unit_price >= 0),
    CONSTRAINT shop_products_stock_ck CHECK (stock_remaining IS NULL OR stock_remaining >= 0),
    CONSTRAINT shop_products_created_by_fk FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX shop_products_status_slot_idx ON shop_products (status, slot, sale_start_at);

CREATE TABLE shop_orders (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    product_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    product_version INT NOT NULL,
    quantity INT NOT NULL,
    currency_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    unit_price BIGINT NOT NULL,
    total_amount BIGINT NOT NULL,
    point_operation_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'succeeded',
    idempotency_key VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    request_hash VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT shop_orders_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE RESTRICT,
    CONSTRAINT shop_orders_product_fk FOREIGN KEY (product_id) REFERENCES shop_products (id) ON DELETE RESTRICT,
    CONSTRAINT shop_orders_currency_fk FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT,
    CONSTRAINT shop_orders_status_ck CHECK (status IN ('succeeded', 'refunded', 'partially_refunded')),
    CONSTRAINT shop_orders_user_idem_uq UNIQUE (user_id, idempotency_key),
    CONSTRAINT shop_orders_op_uq UNIQUE (point_operation_id),
    CONSTRAINT shop_orders_qty_ck CHECK (quantity > 0),
    CONSTRAINT shop_orders_amount_ck CHECK (unit_price >= 0 AND total_amount >= 0)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX shop_orders_user_created_idx ON shop_orders (user_id, created_at);
CREATE INDEX shop_orders_product_idx ON shop_orders (product_id, product_version);

CREATE TABLE user_entitlements (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    product_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    order_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'owned',
    quantity INT NOT NULL DEFAULT 1,
    remaining_quantity INT NOT NULL DEFAULT 1,
    valid_from BIGINT NOT NULL,
    expires_at BIGINT NULL,
    equipped_at BIGINT NULL,
    revoked_at BIGINT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT user_entitlements_status_ck CHECK (status IN ('owned', 'equipped', 'expired', 'revoked', 'consumed')),
    CONSTRAINT user_entitlements_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT user_entitlements_product_fk FOREIGN KEY (product_id) REFERENCES shop_products (id) ON DELETE RESTRICT,
    CONSTRAINT user_entitlements_order_fk FOREIGN KEY (order_id) REFERENCES shop_orders (id) ON DELETE RESTRICT,
    CONSTRAINT user_entitlements_qty_ck CHECK (quantity >= 1 AND remaining_quantity >= 0 AND remaining_quantity <= quantity)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX user_entitlements_user_status_idx ON user_entitlements (user_id, status, valid_from);
CREATE INDEX user_entitlements_product_idx ON user_entitlements (product_id);

CREATE TABLE user_presentations (
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    nickname_decoration_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    nickname_color_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    avatar_frame_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    avatar_attachment_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    profile_effect_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    title_prefix_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    profile_badge_ids_json TEXT NULL,
    post_effect_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    version INT NOT NULL DEFAULT 1,
    updated_at BIGINT NOT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (user_id),
    CONSTRAINT user_presentations_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
