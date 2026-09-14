-- BBLBB shop cosmetic assets (MariaDB)
-- Product-managed PNG assets are public only through the existing attachment
-- ready/content authorization path; the product stores the attachment id.

ALTER TABLE shop_products ADD COLUMN asset_attachment_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NULL;
CREATE INDEX shop_products_asset_attachment_idx ON shop_products (asset_attachment_id);

