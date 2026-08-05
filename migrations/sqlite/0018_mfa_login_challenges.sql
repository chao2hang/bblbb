-- BBLBB 两步登录 MFA challenge 迁移（M02-UX-03）
-- 启用 TOTP 的用户：密码验证成功后签发一次性 challenge token（只存
-- SHA-256 hash，5 分钟过期），登录第二步行 /api/v1/auth/login/mfa
-- 提交 TOTP code 或恢复码完成登录；原子消费防重放。

CREATE TABLE mfa_login_challenges (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    consumed_at INTEGER,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX mfa_login_challenges_user_idx ON mfa_login_challenges (user_id);
