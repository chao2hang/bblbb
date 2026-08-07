-- BBLBB 内容访问授权（M04-VISIBILITY-05/06，SQLite）
--
-- content_access_grants：用户对帖子/回复的访问授权——
-- 1) after_reply（M04-VISIBILITY-05）：用户在本主题发布有效回复后写入
--    reply 来源 grant（reply_grant_persists 冻结规则决定删除/处罚后是否保留）；
-- 2) paid（M04-VISIBILITY-06）：M7 账本扣款成功后写入 purchase 来源 grant
--    （本里程碑只读 grant，扣款/grant 创建由 M7 原子完成）；
-- 3) moderator/import：管理显式查看与数据迁移。
--
-- 约束：post_id 与 comment_id 二者恰有一个（由 grant_target_key 归一化保证，
-- 避免跨库 NULL 唯一语义差异）；同一 (user_id, grant_target_key) 至多一条
-- 有效 grant，重复请求不会重复扣费。

CREATE TABLE content_access_grants (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    post_id TEXT,
    comment_id TEXT,
    policy_id TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    source_id TEXT,
    point_operation_id TEXT,
    grant_target_key TEXT NOT NULL,
    granted_at INTEGER NOT NULL,
    revoked_at INTEGER,
    UNIQUE (user_id, grant_target_key),
    CHECK (source_kind IN ('reply', 'purchase', 'moderator', 'import')),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE,
    FOREIGN KEY (comment_id) REFERENCES comments (id) ON DELETE CASCADE,
    FOREIGN KEY (policy_id) REFERENCES content_access_policies (id) ON DELETE CASCADE
);

CREATE INDEX content_access_grants_user_idx ON content_access_grants (user_id);
CREATE INDEX content_access_grants_target_idx ON content_access_grants (grant_target_key);
