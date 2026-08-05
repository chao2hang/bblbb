-- BBLBB Session 迁移扩展（M02-SESSION-01）
-- user_sessions 增加设备与版本字段：
--   user_agent      设备/UA（截断），设备列表展示
--   ip_prefix_hash  可选 IP 前缀哈希，用于安全提醒（不作唯一身份依据）
--   revoke_reason   撤销原因（登出/改密/管理撤销/过期）
--   version         Session 旋转计数：登录、权限提升、改密、高风险重认证
--                   时递增并签发新 token_hash，防止 Session fixation

ALTER TABLE user_sessions ADD COLUMN user_agent TEXT NULL;
ALTER TABLE user_sessions ADD COLUMN ip_prefix_hash TEXT NULL;
ALTER TABLE user_sessions ADD COLUMN revoke_reason TEXT NULL;
ALTER TABLE user_sessions ADD COLUMN version INT NOT NULL DEFAULT 0;
