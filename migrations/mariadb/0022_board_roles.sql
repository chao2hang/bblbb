-- BBLBB boards and board-scoped roles migration (MySQL)
-- 1) boards gains parent_id (soft self-reference hierarchy; ALTER cannot carry
--    an FK, see SCHEMA.md), visibility (public/members/restricted/hidden) and
--    posting_mode (normal/approval/readonly/closed);
-- 2) board_roles: roles enabled for a board (composite PK (board_id, role_id),
--    cascade on board/role delete);
-- 3) board_role_assignments: board role grants with expiry (UNIQUE(board_id,
--    user_id, role_id), expires_at NULL = permanent, cascade on board/user/role
--    delete).

ALTER TABLE boards ADD COLUMN parent_id VARCHAR(36) NULL;
ALTER TABLE boards ADD COLUMN visibility VARCHAR(16) NOT NULL DEFAULT 'public';
ALTER TABLE boards ADD COLUMN posting_mode VARCHAR(16) NOT NULL DEFAULT 'normal';

CREATE TABLE board_roles (
    board_id VARCHAR(36) NOT NULL,
    role_id VARCHAR(36) NOT NULL,
    granted_by VARCHAR(36) NULL,
    granted_at BIGINT NOT NULL,
    PRIMARY KEY (board_id, role_id),
    FOREIGN KEY (board_id) REFERENCES boards (id) ON DELETE CASCADE,
    FOREIGN KEY (role_id) REFERENCES roles (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE board_role_assignments (
    id VARCHAR(36) NOT NULL PRIMARY KEY,
    board_id VARCHAR(36) NOT NULL,
    user_id VARCHAR(36) NOT NULL,
    role_id VARCHAR(36) NOT NULL,
    granted_by VARCHAR(36) NULL,
    granted_at BIGINT NOT NULL,
    expires_at BIGINT NULL,
    UNIQUE (board_id, user_id, role_id),
    FOREIGN KEY (board_id) REFERENCES boards (id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (role_id) REFERENCES roles (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX board_role_assignments_user_idx ON board_role_assignments (user_id);
CREATE INDEX board_role_assignments_board_idx ON board_role_assignments (board_id);
