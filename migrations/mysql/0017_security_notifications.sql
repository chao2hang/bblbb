-- BBLBB security notifications migration (MySQL)
-- notifications.security_kind marks security notifications (new device,
-- password/MFA change, session revocation, recovery code use); type stays
-- 'system' and security_kind NOT NULL means a security notification
-- (M05-NOTIFY: security notifications cannot be fully disabled by prefs).

ALTER TABLE notifications ADD COLUMN security_kind VARCHAR(40) NULL;
