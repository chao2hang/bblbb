-- BBLBB 板块与板块级角色迁移（M03-SCHEMA-04）
-- 1) boards 增加 parent_id（软自引用层级，ALTER 不能带 FK，见 SCHEMA.md）、
--    visibility（公开/成员/受限/隐藏）与 posting_mode（正常/审核/只读/关闭）；
-- 2) board_roles：板块启用的角色（复合主键 (board_id, role_id)，删板块/角色级联）；
-- 3) board_role_assignments：带有效期的板块角色 assignment（UNIQUE(board_id,
--    user_id, role_id)，expires_at 可空=永久，删板块/用户/角色级联）。

ALTER TABLE boards ADD COLUMN parent_id TEXT;
ALTER TABLE boards ADD COLUMN visibility TEXT NOT NULL DEFAULT 'public'
    CHECK (visibility IN ('public', 'members', 'restricted', 'hidden'));
ALTER TABLE boards ADD COLUMN posting_mode TEXT NOT NULL DEFAULT 'normal'
    CHECK (posting_mode IN ('normal', 'approval', 'readonly', 'closed'));

CREATE TABLE board_roles (
    board_id TEXT NOT NULL,
    role_id TEXT NOT NULL,
    granted_by TEXT,
    granted_at INTEGER NOT NULL,
    PRIMARY KEY (board_id, role_id),
    FOREIGN KEY (board_id) REFERENCES boards (id) ON DELETE CASCADE,
    FOREIGN KEY (role_id) REFERENCES roles (id) ON DELETE CASCADE
);

CREATE TABLE board_role_assignments (
    id TEXT PRIMARY KEY NOT NULL,
    board_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    role_id TEXT NOT NULL,
    granted_by TEXT,
    granted_at INTEGER NOT NULL,
    expires_at INTEGER,
    UNIQUE (board_id, user_id, role_id),
    FOREIGN KEY (board_id) REFERENCES boards (id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (role_id) REFERENCES roles (id) ON DELETE CASCADE
);

CREATE INDEX board_role_assignments_user_idx ON board_role_assignments (user_id);
CREATE INDEX board_role_assignments_board_idx ON board_role_assignments (board_id);
