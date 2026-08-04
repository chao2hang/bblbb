-- BBLBB community migration: boards, posts, comments, tags

-- 板块/版块
CREATE TABLE boards (
    id TEXT PRIMARY KEY NOT NULL,
    slug TEXT NOT NULL COLLATE BINARY,
    name TEXT NOT NULL,
    description TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    post_count INTEGER NOT NULL DEFAULT 0,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX boards_slug_uq ON boards (slug);

-- 帖子/主题
CREATE TABLE posts (
    id TEXT PRIMARY KEY NOT NULL,
    board_id TEXT NOT NULL,
    author_id TEXT NOT NULL,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    content_format TEXT NOT NULL DEFAULT 'markdown',
    status TEXT NOT NULL DEFAULT 'published'
        CHECK (status IN ('draft', 'published', 'hidden', 'locked', 'deleted')),
    visibility TEXT NOT NULL DEFAULT 'public'
        CHECK (visibility IN ('public', 'logged_in', 'after_reply', 'level', 'paid')),
    reply_count INTEGER NOT NULL DEFAULT 0,
    view_count INTEGER NOT NULL DEFAULT 0,
    last_reply_at INTEGER,
    last_reply_by TEXT,
    pinned INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (board_id) REFERENCES boards (id) ON DELETE CASCADE,
    FOREIGN KEY (author_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX posts_board_id_idx ON posts (board_id);
CREATE INDEX posts_author_id_idx ON posts (author_id);
CREATE INDEX posts_status_visibility_idx ON posts (status, visibility);
CREATE INDEX posts_created_at_idx ON posts (created_at);

-- 评论/回复
CREATE TABLE comments (
    id TEXT PRIMARY KEY NOT NULL,
    post_id TEXT NOT NULL,
    author_id TEXT NOT NULL,
    parent_id TEXT,
    content TEXT NOT NULL,
    content_format TEXT NOT NULL DEFAULT 'markdown',
    status TEXT NOT NULL DEFAULT 'published'
        CHECK (status IN ('published', 'hidden', 'deleted')),
    floor INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE,
    FOREIGN KEY (author_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (parent_id) REFERENCES comments (id) ON DELETE CASCADE
);

CREATE INDEX comments_post_id_idx ON comments (post_id);
CREATE INDEX comments_author_id_idx ON comments (author_id);
CREATE INDEX comments_parent_id_idx ON comments (parent_id);

-- 标签
CREATE TABLE tags (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL COLLATE BINARY,
    usage_count INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX tags_name_uq ON tags (name);

-- 帖子-标签关联
CREATE TABLE post_tags (
    post_id TEXT NOT NULL,
    tag_id TEXT NOT NULL,
    PRIMARY KEY (post_id, tag_id),
    FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags (id) ON DELETE CASCADE
);
