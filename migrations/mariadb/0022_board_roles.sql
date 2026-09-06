
ALTER TABLE boards ADD COLUMN parent_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NULL;
ALTER TABLE boards ADD COLUMN visibility VARCHAR(16) NOT NULL DEFAULT 'public';
ALTER TABLE boards ADD COLUMN posting_mode VARCHAR(16) NOT NULL DEFAULT 'normal';

CREATE TABLE board_roles (
    board_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    role_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    granted_by CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NULL,
    granted_at BIGINT NOT NULL,
    PRIMARY KEY (board_id, role_id),
    FOREIGN KEY (board_id) REFERENCES boards (id) ON DELETE CASCADE,
    FOREIGN KEY (role_id) REFERENCES roles (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE board_role_assignments (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL PRIMARY KEY,
    board_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    role_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    granted_by CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NULL,
    granted_at BIGINT NOT NULL,
    expires_at BIGINT NULL,
    UNIQUE (board_id, user_id, role_id),
    FOREIGN KEY (board_id) REFERENCES boards (id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (role_id) REFERENCES roles (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX board_role_assignments_user_idx ON board_role_assignments (user_id);
CREATE INDEX board_role_assignments_board_idx ON board_role_assignments (board_id);
