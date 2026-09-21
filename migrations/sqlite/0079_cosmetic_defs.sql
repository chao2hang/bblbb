-- BBLBB 可配置装扮样式库（M07-SHOP-UI-10，SQLite）
--
-- cosmetic_defs：管理员自定义的命名装扮样式。商品 Token 通过
--   `nickname.color.<id>` / `avatar.frame.<id>` 引用这里的定义；
--   style_json 是服务端 schema 校验过的结构化参数（颜色 #rrggbb、
--   动画枚举、时长钳制），前端只渲染校验过的字段——禁止任意 CSS。
-- v1 开放 kind：nickname_color / avatar_frame（表 CHECK 预留其余 kind，
--   由 API 层拒绝未开放类型）。

CREATE TABLE cosmetic_defs (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    style_json TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CONSTRAINT cosmetic_defs_kind_ck CHECK (kind IN ('nickname_color', 'avatar_frame', 'profile_effect', 'post_effect', 'badge')),
    CONSTRAINT cosmetic_defs_status_ck CHECK (status IN ('active', 'archived'))
);

CREATE INDEX cosmetic_defs_kind_idx ON cosmetic_defs (kind, status);
