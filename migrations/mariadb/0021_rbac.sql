-- BBLBB RBAC data model migration (MySQL)
-- 1) roles: global roles (system roles with is_system=1 are not deletable;
--    roles do not store a permission JSON to avoid a dual source of truth);
-- 2) permissions: permission reference table (name maps to OpenAPI
--    x-permission / the permission matrix);
-- 3) role_permissions: role-permission mapping (composite PK, cascade on
--    role/permission delete);
-- 4) user_roles: global role assignments (composite PK, granted_by/expires_at;
--    board-scoped assignments live in M03-SCHEMA-04 board_role_assignments).

CREATE TABLE roles (
    id VARCHAR(36) NOT NULL PRIMARY KEY,
    name VARCHAR(64) NOT NULL UNIQUE,
    display_name VARCHAR(100) NOT NULL,
    description VARCHAR(500) NULL,
    is_system TINYINT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE permissions (
    id VARCHAR(36) NOT NULL PRIMARY KEY,
    name VARCHAR(100) NOT NULL UNIQUE,
    description VARCHAR(500) NULL,
    risk_level VARCHAR(16) NOT NULL DEFAULT 'normal',
    is_system TINYINT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    CONSTRAINT permissions_risk_level_check CHECK (risk_level IN ('normal', 'sensitive', 'system'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE role_permissions (
    role_id VARCHAR(36) NOT NULL,
    permission_id VARCHAR(36) NOT NULL,
    PRIMARY KEY (role_id, permission_id),
    FOREIGN KEY (role_id) REFERENCES roles (id) ON DELETE CASCADE,
    FOREIGN KEY (permission_id) REFERENCES permissions (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE user_roles (
    user_id VARCHAR(36) NOT NULL,
    role_id VARCHAR(36) NOT NULL,
    granted_by VARCHAR(36) NULL,
    granted_at BIGINT NOT NULL,
    expires_at BIGINT NULL,
    PRIMARY KEY (user_id, role_id),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (role_id) REFERENCES roles (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX user_roles_user_idx ON user_roles (user_id);
CREATE INDEX user_roles_role_idx ON user_roles (role_id);
