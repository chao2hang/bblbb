-- BBLBB Video plugin schema (M10-VIDEO, SQLite)
--
-- video_embeds：resolve→create 绑定 target（post/comment）；状态机
-- pending/ready/blocked/error/removed；resolution_id 一次性短效；policy_version
-- 版本化（策略变更触发重新检查历史引用）。Source 原始 URL 只存 hash 加可安全
-- 展示的官方 URL（降级外链），不存签名/Key/iframe HTML。
-- video_provider_policies：direct/hls/xigua 每 Provider 出站/解析策略，
-- 版本化（行更新 bump updated_at）。

CREATE TABLE video_embeds (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    resolution_id TEXT NOT NULL,
    source TEXT NOT NULL,
    source_hash TEXT NOT NULL,
    provider TEXT NOT NULL CHECK (provider IN ('direct', 'hls', 'xigua')),
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'ready', 'blocked', 'error', 'removed')),
    target_type TEXT NOT NULL CHECK (target_type IN ('post', 'comment')),
    target_id TEXT NOT NULL,
    title TEXT NULL,
    poster_attachment_id TEXT NULL,
    official_url TEXT NULL,
    error_class TEXT NULL,
    policy_version INTEGER NOT NULL DEFAULT 1,
    version INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CONSTRAINT video_embeds_resolution_uq UNIQUE (resolution_id),
    CONSTRAINT video_embeds_target_uq UNIQUE (target_type, target_id),
    CONSTRAINT video_embeds_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX video_embeds_user_idx ON video_embeds (user_id, created_at);
CREATE INDEX video_embeds_status_idx ON video_embeds (status, updated_at);

CREATE TABLE video_provider_policies (
    provider TEXT PRIMARY KEY NOT NULL CHECK (provider IN ('direct', 'hls', 'xigua')),
    enabled INTEGER NOT NULL DEFAULT 0,
    allow_hosts_json TEXT NULL,
    max_redirects INTEGER NOT NULL DEFAULT 3,
    max_response_bytes INTEGER NOT NULL DEFAULT 5242880,
    max_playlist_depth INTEGER NOT NULL DEFAULT 5,
    max_segments INTEGER NOT NULL DEFAULT 200,
    max_duration_ms INTEGER NOT NULL DEFAULT 3600000,
    config_json TEXT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    updated_by TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
