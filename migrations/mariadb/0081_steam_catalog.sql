-- Steam 装扮目录（M07-SHOP-ASSETS，MariaDB）
--
-- steam_catalog_items：Steam 点数商店头像框（class 14）/ 迷你资料背景
-- （class 13）的元数据镜像。管理端「同步 Steam 目录」实时拉取合并；
-- 资产公开路由在存储未命中时按 image 反查 appid 实时补拉素材。

CREATE TABLE steam_catalog_items (
    id VARCHAR(64) CHARACTER SET ascii COLLATE ascii_general_ci PRIMARY KEY NOT NULL,
    kind VARCHAR(16) NOT NULL,
    defid BIGINT NOT NULL UNIQUE,
    appid BIGINT NOT NULL,
    name VARCHAR(255) NOT NULL,
    image VARCHAR(128) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    webm VARCHAR(128) CHARACTER SET ascii COLLATE ascii_general_ci,
    mp4 VARCHAR(128) CHARACTER SET ascii COLLATE ascii_general_ci,
    cost INTEGER NOT NULL DEFAULT 0,
    extra_json TEXT,
    updated_at BIGINT NOT NULL,
    CONSTRAINT steam_catalog_items_kind_ck CHECK (kind IN ('frames', 'backgrounds'))
);

CREATE INDEX steam_catalog_items_kind_idx ON steam_catalog_items (kind, defid);
CREATE INDEX steam_catalog_items_image_idx ON steam_catalog_items (image);
