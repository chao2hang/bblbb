-- BBLBB shop cosmetic assets (SQLite)
-- Product-managed PNG assets are public only through the existing attachment
-- ready/content authorization path; the product stores the attachment id.

ALTER TABLE shop_products ADD COLUMN asset_attachment_id TEXT NULL;
CREATE INDEX shop_products_asset_attachment_idx ON shop_products (asset_attachment_id);

-- Keep accidental hard deletion from leaving a dangling product reference.
-- SQLite cannot add this FK to an existing table without table rebuild, so the
-- service validates referenced attachments before create/update/publish.
