-- BBLBB 管理域扩展（GAP-FIX 管理域 Part A，SQLite）
--
-- 与 mysql/mariadb 同版本同构：
-- 1) site_settings：站点设置单行表（id 恒为 'singleton'，CHECK 强制），
--    version 乐观并发（PATCH /api/v1/admin/settings 的 If-Match）；种子
--    一行合理默认（开放注册/邮件验证开、匿名回复关、RSS 开、维护模式关）；
-- 2) notification_broadcasts：通知广播发件箱；notifications.broadcast_id
--    把广播产出的通知行精确关联到 broadcasts 行——召回（recall）删除
--    broadcast_id 匹配且 is_read=0 的行（已读保留），不做 title 模糊匹配；
-- 3) posts 补列 price_coin（付费解锁定价，NULL=免费）。is_featured /
--    is_pinned / is_locked **不加列**：0003 已有 posts.pinned（布尔）与
--    0003/0032 的 pinned_at/featured_at/closed_at 同义列（STATE-MACHINES
--    §Post：closed_at 非空即锁帖），复用既有列避免双源漂移——
--    is_featured = featured_at IS NOT NULL、is_pinned = pinned、
--    is_locked = closed_at IS NOT NULL；
-- 4) tags.status：标签生命周期标记（NULL=正常；'merged'=已并入目标标签，
--    标签合并流程使用；与既有 is_active 启停语义正交）；
-- 5) ai_site_config：站点级 AI 配置单行表（总开关/数据模式/用途 flags/
--    预算 JSON，version 乐观并发）。这是 GET/PATCH /api/v1/admin/ai/config
--    顶层 version 与站点级字段的存储位置（0052 的 ai_providers 只有
--    Provider 行级配置，无站点级聚合），三方言等价的固定列 + JSON 文本
--    是最简单且跨库安全的方案。

CREATE TABLE site_settings (
    id TEXT PRIMARY KEY NOT NULL,
    open_registration INTEGER NOT NULL DEFAULT 1,
    email_verification INTEGER NOT NULL DEFAULT 1,
    anonymous_replies INTEGER NOT NULL DEFAULT 0,
    public_rss INTEGER NOT NULL DEFAULT 1,
    maintenance_mode INTEGER NOT NULL DEFAULT 0,
    site_name TEXT NOT NULL DEFAULT 'BBLBB',
    default_lang TEXT NOT NULL DEFAULT 'zh-CN',
    api_rate_limit INTEGER,
    version INTEGER NOT NULL DEFAULT 1,
    updated_at INTEGER NOT NULL,
    CHECK (id = 'singleton')
);

INSERT INTO site_settings (id, open_registration, email_verification, anonymous_replies, public_rss, maintenance_mode, site_name, default_lang, api_rate_limit, version, updated_at)
VALUES ('singleton', 1, 1, 0, 1, 0, 'BBLBB', 'zh-CN', 60, 1, 1722816000);

CREATE TABLE notification_broadcasts (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    target_count INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);

CREATE INDEX notification_broadcasts_created_idx ON notification_broadcasts (created_at);

-- 召回按精确关联删除（broadcast_id），不依赖 title+时间模糊匹配。
ALTER TABLE notifications ADD COLUMN broadcast_id TEXT;

CREATE INDEX notifications_broadcast_idx ON notifications (broadcast_id);

ALTER TABLE posts ADD COLUMN price_coin INTEGER;

ALTER TABLE tags ADD COLUMN status TEXT;

CREATE TABLE ai_site_config (
    id TEXT PRIMARY KEY NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 0,
    data_mode TEXT NOT NULL DEFAULT 'redacted',
    flags_json TEXT NOT NULL DEFAULT '{}',
    budgets_json TEXT NOT NULL DEFAULT '{}',
    version INTEGER NOT NULL DEFAULT 1,
    updated_at INTEGER NOT NULL,
    CHECK (id = 'singleton')
);

INSERT INTO ai_site_config (id, enabled, data_mode, flags_json, budgets_json, version, updated_at)
VALUES ('singleton', 0, 'redacted', '{}', '{}', 1, 1722816000);
