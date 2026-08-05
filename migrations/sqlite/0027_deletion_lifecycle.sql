-- BBLBB 注销生命周期：法律保留列（M03-PROFILE-08）
-- legal_hold_at 非空 = 用户处于法律保留/调查冻结（RETENTION-PRIVACY.md §1
-- 最高优先级）：禁止发起注销请求；到期执行 Job 跳过并写审计
-- （user.deletion_deferred_legal_hold）。与 delete_requested_at（请求时间）/
-- deleted_at（终态时间）并列，构成注销生命周期时间列。

ALTER TABLE users ADD COLUMN legal_hold_at INTEGER;
