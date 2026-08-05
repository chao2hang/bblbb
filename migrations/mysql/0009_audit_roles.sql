-- BBLBB 审计日志扩展（M01-AUDIT-01）
-- 增加 effective role、reason 与 policy version。
-- 审计表只追加、不可关闭：无 status/disabled 列，代码不提供删除/修改路径。

ALTER TABLE audit_logs ADD COLUMN effective_role VARCHAR(50) NULL;
ALTER TABLE audit_logs ADD COLUMN reason TEXT NULL;
ALTER TABLE audit_logs ADD COLUMN policy_version VARCHAR(32) NULL;
