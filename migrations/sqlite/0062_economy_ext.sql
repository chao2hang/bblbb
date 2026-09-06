-- BBLBB 经济与个人域扩展（GAP-FIX 管理域 Part B，SQLite）
--
-- 与 mysql/mariadb 同版本同构：
-- 1) level_rules：等级规则表（GET/PATCH /api/v1/admin/levels 的存储）。
--    0050 的 levels 是「等级方案阈值表」（scheme 维度、UUID 主键、无管理
--    规则列），与按整数 level 寻址的管理规则语义不同，因此新建独立
--    level_rules（level INTEGER 主键）而不是给 levels 加列；users.level
--    （0019）为用户当前等级，user_count 按它聚合。种子 5 级；
--    version 列支撑 PATCH /admin/levels/{level} 的 If-Match 乐观锁；
-- 2) posts.summary：作者手写摘要（≤300；文章类型）。注意与既有
--    post_contents.excerpt（渲染管线自动生成的正文摘录）语义不同，
--    0023 的 posts.excerpt 为 SEO 摘要遗留列——不复用避免双源漂移；
--    posts.price_coin 已由 0061 添加（付费解锁定价）；
-- 3) drafts 快照字段：price_coin/summary/tags_json（草稿保存付费定价/
--    摘要/标签，发布时同步到 posts——0034 drafts 原表无这些列）。
--
-- oauth_grants **不新建**：0055_oidc 的 oauth_consents
-- （user_id, client_id, scope, granted_at, revoked_at）已是用户授权记录表，
-- GET/DELETE /api/v1/me/oauth-grants 直接查询/撤销它（client_name 取
-- oauth_clients.name，last_used_at 取 oauth_tokens 最近使用时间的聚合）。

CREATE TABLE level_rules (
    level INTEGER NOT NULL,
    name TEXT NOT NULL,
    min_exp INTEGER NOT NULL,
    daily_post_limit INTEGER NOT NULL,
    daily_comment_limit INTEGER NOT NULL,
    attachment_quota INTEGER NOT NULL,
    is_enabled INTEGER NOT NULL DEFAULT 1,
    version INTEGER NOT NULL DEFAULT 1,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (level),
    CONSTRAINT level_rules_min_exp_ck CHECK (min_exp >= 0),
    CONSTRAINT level_rules_limits_ck CHECK (daily_post_limit >= 0 AND daily_comment_limit >= 0 AND attachment_quota >= 0)
);

INSERT INTO level_rules (level, name, min_exp, daily_post_limit, daily_comment_limit, attachment_quota, is_enabled, version, updated_at)
VALUES
    (1, 'Lv1 新人', 0, 5, 20, 52428800, 1, 1, 1722816000),
    (2, 'Lv2 学徒', 500, 10, 50, 104857600, 1, 1, 1722816000),
    (3, 'Lv3 常客', 2000, 20, 100, 209715200, 1, 1, 1722816000),
    (4, 'Lv4 资深', 8000, 40, 200, 524288000, 1, 1, 1722816000),
    (5, 'Lv5 元老', 30000, 80, 400, 1073741824, 1, 1, 1722816000);

CREATE INDEX level_rules_enabled_idx ON level_rules (is_enabled, level);

-- 作者手写摘要（文章类型；post_contents.excerpt 是自动摘录，语义不同）。
ALTER TABLE posts ADD COLUMN summary TEXT;

-- 草稿快照：付费定价/摘要/标签（发布路径同步到 posts）。
ALTER TABLE drafts ADD COLUMN price_coin INTEGER;
ALTER TABLE drafts ADD COLUMN summary TEXT;
ALTER TABLE drafts ADD COLUMN tags_json TEXT;
