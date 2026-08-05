-- BBLBB 匿名预认证 CSRF 状态（M02-SESSION-08）
-- 为 register/login/verify-email/resend-verification/password-reset 等
-- 预认证写端点提供服务端可回溯校验的 CSRF 状态，防止 login CSRF
-- （SECURITY.md §4：匿名登录/注册流程使用独立的预认证 CSRF Cookie/状态）：
--   id               记录 ID（uuid v7，参与派生 CSRF token）
--   token_hash       `__Host-bblbb_csrf` cookie 令牌的 SHA-256（只存 hash）
--   csrf_secret_hash 派生 CSRF token 的秘密（与 user_sessions 一致取 token_hash）
--   created_at       签发时间（Unix 毫秒）
--   expires_at       过期时间（Unix 毫秒；TTL 10 分钟）

CREATE TABLE preauth_csrf_tokens (
    id TEXT PRIMARY KEY NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    csrf_secret_hash TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE INDEX preauth_csrf_tokens_expires_at_idx ON preauth_csrf_tokens (expires_at);
