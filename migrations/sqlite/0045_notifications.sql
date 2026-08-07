-- BBLBB 通知扩展与偏好（M05-SCHEMA-05，SQLite）
--
-- notifications 扩展（0004 建表，此处 ALTER 追加）：
-- template_key 模板键；resource_type/resource_id 多态资源引用
-- （开放字符串，避免枚举阻塞通知引用演进）；delivery_dedup_key 投递去重键
-- ——同一 (user_id, delivery_dedup_key) 至多一条通知（NULL 不去重，
-- 三库一致）；category 通知类别 activity/moderation/system/security/digest
-- （与遗留 type 枚举正交，security_kind 仍标记具体安全事件）。
--
-- notification_preferences：每用户每类别一条（PRIMARY KEY），channel 开关
-- email/in_app/push；「安全通知不可被普通偏好全关」由 CHECK 强制——
-- category='security' 时至少保留一个 channel 开启。

ALTER TABLE notifications ADD COLUMN template_key TEXT;
ALTER TABLE notifications ADD COLUMN resource_type TEXT;
ALTER TABLE notifications ADD COLUMN resource_id TEXT;
ALTER TABLE notifications ADD COLUMN delivery_dedup_key TEXT;
ALTER TABLE notifications ADD COLUMN category TEXT NOT NULL DEFAULT 'activity'
    CHECK (category IN ('activity', 'moderation', 'system', 'security', 'digest'));

CREATE UNIQUE INDEX notifications_delivery_dedup_uq
    ON notifications (user_id, delivery_dedup_key);

CREATE TABLE notification_preferences (
    user_id TEXT NOT NULL,
    category TEXT NOT NULL
        CHECK (category IN ('activity', 'moderation', 'system', 'security', 'digest')),
    email_enabled INTEGER NOT NULL DEFAULT 1,
    in_app_enabled INTEGER NOT NULL DEFAULT 1,
    push_enabled INTEGER NOT NULL DEFAULT 1,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, category),
    CHECK (category != 'security' OR email_enabled = 1 OR in_app_enabled = 1 OR push_enabled = 1),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);
