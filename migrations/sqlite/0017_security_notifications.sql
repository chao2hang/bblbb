-- BBLBB 安全通知迁移（M02-MFA-08）
-- notifications 增加 security_kind：安全通知标记列（新设备登录、密码/MFA
-- 变化、Session 撤销、恢复码使用）。type 保持 'system'，security_kind
-- 非空即表示安全通知（M05-NOTIFY 偏好强制“安全通知不可关闭”）。

ALTER TABLE notifications ADD COLUMN security_kind TEXT;
