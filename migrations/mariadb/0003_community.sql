-- BBLBB community migration: boards, posts, comments, tags (MariaDB 10.11)
-- Identical to MySQL 8.0 schema

CREATE TABLE boards (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    slug VARCHAR(100) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    name VARCHAR(100) NOT NULL,
    description TEXT,
    sort_order INT NOT NULL DEFAULT 0,
    post_count INT NOT NULL DEFAULT 0,
    is_active TINYINT NOT NULL DEFAULT 1,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY boards_slug_uq (slug)
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_bin;

CREATE TABLE posts (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    board_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    author_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    title VARCHAR(200) NOT NULL,
    content MEDIUMTEXT NOT NULL,
    content_format VARCHAR(20) NOT NULL DEFAULT 'markdown',
    status VARCHAR(32) NOT NULL DEFAULT 'published',
    visibility VARCHAR(32) NOT NULL DEFAULT 'public',
    reply_count INT NOT NULL DEFAULT 0,
    view_count INT NOT NULL DEFAULT 0,
    last_reply_at BIGINT NULL,
    last_reply_by CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    pinned TINYINT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    KEY posts_board_id_idx (board_id),
    KEY posts_author_id_idx (author_id),
    KEY posts_status_visibility_idx (status, visibility),
    KEY posts_created_at_idx (created_at),
    CONSTRAINT posts_status_ck CHECK (status IN ('draft', 'published', 'hidden', 'locked', 'deleted')),
    CONSTRAINT posts_visibility_ck CHECK (visibility IN ('public', 'logged_in', 'after_reply', 'level', 'paid')),
    CONSTRAINT posts_board_fk FOREIGN KEY (board_id) REFERENCES boards (id) ON DELETE CASCADE,
    CONSTRAINT posts_author_fk FOREIGN KEY (author_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_bin;

CREATE TABLE comments (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    post_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    author_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    parent_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    content MEDIUMTEXT NOT NULL,
    content_format VARCHAR(20) NOT NULL DEFAULT 'markdown',
    status VARCHAR(32) NOT NULL DEFAULT 'published',
    floor INT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    KEY comments_post_id_idx (post_id),
    KEY comments_author_id_idx (author_id),
    KEY comments_parent_id_idx (parent_id),
    CONSTRAINT comments_status_ck CHECK (status IN ('published', 'hidden', 'deleted')),
    CONSTRAINT comments_post_fk FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE,
    CONSTRAINT comments_author_fk FOREIGN KEY (author_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT comments_parent_fk FOREIGN KEY (parent_id) REFERENCES comments (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_bin;

CREATE TABLE tags (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    name VARCHAR(50) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    usage_count INT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY tags_name_uq (name)
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_bin;

CREATE TABLE post_tags (
    post_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    tag_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    PRIMARY KEY (post_id, tag_id),
    CONSTRAINT post_tags_post_fk FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE,
    CONSTRAINT post_tags_tag_fk FOREIGN KEY (tag_id) REFERENCES tags (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_bin;
