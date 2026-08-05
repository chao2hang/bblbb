-- BBLBB 匿名预认证 CSRF 状态（M02-SESSION-08）
-- 为 register/login/verify-email/resend-verification/password-reset 等
-- 预认证写端点提供服务端可回溯校验的 CSRF 状态，防止 login CSRF
-- （SECURITY.md §4：匿名登录/注册流程使用独立的预认证 CSRF Cookie/状态）。

CREATE TABLE preauth_csrf_tokens (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    token_hash CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    csrf_secret_hash CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY preauth_csrf_tokens_token_hash_uq (token_hash),
    KEY preauth_csrf_tokens_expires_at_idx (expires_at)
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_bin;
