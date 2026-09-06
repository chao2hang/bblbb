
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
