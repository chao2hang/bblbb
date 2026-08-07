-- BBLBB Video plugin schema (M10-VIDEO, MySQL)
--
-- video_embeds：resolve→create 绑定 target；状态机 pending/ready/blocked/error/removed；
-- resolution_id 一次性短效；policy_version 版本化。Source 只存 hash + 官方 URL。
-- video_provider_policies：direct/hls/xigua 每 Provider 策略，版本化。

CREATE TABLE video_embeds (
    id VARCHAR(36) PRIMARY KEY NOT NULL,
    user_id VARCHAR(36) NOT NULL,
    resolution_id VARCHAR(36) NOT NULL,
    source VARCHAR(2048) NOT NULL,
    source_hash VARCHAR(64) NOT NULL,
    provider VARCHAR(16) NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'pending',
    target_type VARCHAR(16) NOT NULL,
    target_id VARCHAR(36) NOT NULL,
    title VARCHAR(240) NULL,
    poster_attachment_id VARCHAR(36) NULL,
    official_url VARCHAR(2048) NULL,
    error_class VARCHAR(64) NULL,
    policy_version BIGINT NOT NULL DEFAULT 1,
    version BIGINT NOT NULL DEFAULT 1,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    CONSTRAINT video_embeds_provider_ck CHECK (provider IN ('direct', 'hls', 'xigua')),
    CONSTRAINT video_embeds_status_ck CHECK (status IN ('pending', 'ready', 'blocked', 'error', 'removed')),
    CONSTRAINT video_embeds_target_ck CHECK (target_type IN ('post', 'comment')),
    CONSTRAINT video_embeds_resolution_uq UNIQUE (resolution_id),
    CONSTRAINT video_embeds_target_uq UNIQUE (target_type, target_id),
    CONSTRAINT video_embeds_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE INDEX video_embeds_user_idx ON video_embeds (user_id, created_at);
CREATE INDEX video_embeds_status_idx ON video_embeds (status, updated_at);

CREATE TABLE video_provider_policies (
    provider VARCHAR(16) PRIMARY KEY NOT NULL,
    enabled TINYINT(1) NOT NULL DEFAULT 0,
    allow_hosts_json TEXT NULL,
    max_redirects BIGINT NOT NULL DEFAULT 3,
    max_response_bytes BIGINT NOT NULL DEFAULT 5242880,
    max_playlist_depth BIGINT NOT NULL DEFAULT 5,
    max_segments BIGINT NOT NULL DEFAULT 200,
    max_duration_ms BIGINT NOT NULL DEFAULT 3600000,
    config_json TEXT NULL,
    version BIGINT NOT NULL DEFAULT 1,
    updated_by VARCHAR(36) NOT NULL,
    updated_at BIGINT NOT NULL,
    CONSTRAINT video_policies_provider_ck CHECK (provider IN ('direct', 'hls', 'xigua'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
