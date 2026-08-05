-- BBLBB 近期认证/step-up 迁移（M02-MFA-07）
-- user_sessions 增加 auth_verified_at：最近一次“完整认证”（密码 + 可选 MFA）
-- 的时间（Unix 毫秒，NULL = 从未或已过期）。
-- 高风险操作（改密、停用 MFA、角色提升、退款、密钥/Secret 操作，SECURITY.md
-- §14 + PERMISSION-MATRIX）要求近期重新认证：auth_verified_at 距今超过
-- BBLBB__STEP_UP_WINDOW_SECS 时必须 step-up 重认证后刷新。

ALTER TABLE user_sessions ADD COLUMN auth_verified_at INTEGER;
