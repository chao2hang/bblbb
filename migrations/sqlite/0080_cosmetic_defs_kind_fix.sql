-- 修复 cosmetic_defs kind CHECK 与代码枚举不一致（M07-SHOP-STUDIO，SQLite）
--
-- 0079 的 CHECK 只允许 ('nickname_color','avatar_frame','profile_effect',
-- 'post_effect','badge')，而 API 层（cosmetics.rs CUSTOMIZABLE_KINDS）实际
-- 写入 cosmetic_badge / reaction_pack / utility / title_prefix，导致这四类
-- 样式定义被数据库拒绝。SQLite 不支持 ALTER CHECK，按惯例重建表
-- （保留 'badge' 兼容历史种子；索引随旧表删除后重建）。

CREATE TABLE cosmetic_defs_new (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    style_json TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CONSTRAINT cosmetic_defs_kind_ck CHECK (kind IN ('nickname_color', 'avatar_frame', 'profile_effect', 'post_effect', 'cosmetic_badge', 'badge', 'reaction_pack', 'utility', 'title_prefix')),
    CONSTRAINT cosmetic_defs_status_ck CHECK (status IN ('active', 'archived'))
);

INSERT INTO cosmetic_defs_new (id, kind, name, style_json, status, created_by, created_at, updated_at)
    SELECT id, kind, name, style_json, status, created_by, created_at, updated_at FROM cosmetic_defs;

DROP TABLE cosmetic_defs;

ALTER TABLE cosmetic_defs_new RENAME TO cosmetic_defs;

CREATE INDEX cosmetic_defs_kind_idx ON cosmetic_defs (kind, status);
