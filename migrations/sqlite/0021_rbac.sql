-- BBLBB RBAC 数据模型迁移（M03-SCHEMA-03）
-- 1) roles：全局角色（内置角色 is_system=1 不可删除；角色不保存权限 JSON，
--    避免双事实来源）；
-- 2) permissions：权限参考表（name 对应 OpenAPI x-permission / 权限矩阵）；
-- 3) role_permissions：角色-权限映射（复合主键，删角色/权限级联清理）；
-- 4) user_roles：全局角色 assignment（复合主键，granted_by/expires_at；
--    板块级 assignment 见 M03-SCHEMA-04 board_role_assignments）。

CREATE TABLE roles (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    description TEXT,
    is_system INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE permissions (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    risk_level TEXT NOT NULL DEFAULT 'normal'
        CHECK (risk_level IN ('normal', 'sensitive', 'system')),
    is_system INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);

CREATE TABLE role_permissions (
    role_id TEXT NOT NULL,
    permission_id TEXT NOT NULL,
    PRIMARY KEY (role_id, permission_id),
    FOREIGN KEY (role_id) REFERENCES roles (id) ON DELETE CASCADE,
    FOREIGN KEY (permission_id) REFERENCES permissions (id) ON DELETE CASCADE
);

CREATE TABLE user_roles (
    user_id TEXT NOT NULL,
    role_id TEXT NOT NULL,
    granted_by TEXT,
    granted_at INTEGER NOT NULL,
    expires_at INTEGER,
    PRIMARY KEY (user_id, role_id),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (role_id) REFERENCES roles (id) ON DELETE CASCADE
);

CREATE INDEX user_roles_user_idx ON user_roles (user_id);
CREATE INDEX user_roles_role_idx ON user_roles (role_id);
