-- Steam 装扮目录（M07-SHOP-ASSETS，SQLite）
--
-- steam_catalog_items：Steam 点数商店头像框（class 14）/ 迷你资料背景
-- （class 13）的元数据镜像。管理端「同步 Steam 目录」实时拉取合并；
-- 资产公开路由在存储未命中时按 image 反查 appid 实时补拉素材。

CREATE TABLE steam_catalog_items (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL,
    defid INTEGER NOT NULL UNIQUE,
    appid INTEGER NOT NULL,
    name TEXT NOT NULL,
    image TEXT NOT NULL,
    webm TEXT,
    mp4 TEXT,
    cost INTEGER NOT NULL DEFAULT 0,
    extra_json TEXT,
    updated_at INTEGER NOT NULL,
    CONSTRAINT steam_catalog_items_kind_ck CHECK (kind IN ('frames', 'backgrounds'))
);

CREATE INDEX steam_catalog_items_kind_idx ON steam_catalog_items (kind, defid);
CREATE INDEX steam_catalog_items_image_idx ON steam_catalog_items (image);
