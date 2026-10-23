-- 1. 更新 site_settings 中的公开源、存储与允许上传类型
UPDATE site_settings SET 
    public_source = 'https://bblbb.com',
    -- 存储后端与凭据必须通过受审计的管理 API 配置，不在此脚本写入。
    storage_allowed_upload_types = 'image,pdf,text,office,av',
    site_description = 'BBLBB 社区论坛',
    version = version + 1
WHERE id = 'singleton';

-- 2. 补齐 quota_policy_revisions（Level 5~10 阶梯配额）
INSERT INTO quota_policy_revisions (id, level, single_file_max_bytes, total_bytes, daily_upload_bytes, retention_days, policy_version, created_by, created_at)
SELECT UUID(), 6, 20971520, 2147483648, 419430400, 30, 1, '01a07448-9000-7000-8000-000000000001', UNIX_TIMESTAMP() * 1000
WHERE NOT EXISTS (SELECT 1 FROM quota_policy_revisions WHERE level = 6);
INSERT INTO quota_policy_revisions (id, level, single_file_max_bytes, total_bytes, daily_upload_bytes, retention_days, policy_version, created_by, created_at)
SELECT UUID(), 7, 20971520, 2147483648, 419430400, 30, 1, '01a07448-9000-7000-8000-000000000001', UNIX_TIMESTAMP() * 1000
WHERE NOT EXISTS (SELECT 1 FROM quota_policy_revisions WHERE level = 7);
INSERT INTO quota_policy_revisions (id, level, single_file_max_bytes, total_bytes, daily_upload_bytes, retention_days, policy_version, created_by, created_at)
SELECT UUID(), 8, 20971520, 2147483648, 419430400, 30, 1, '01a07448-9000-7000-8000-000000000001', UNIX_TIMESTAMP() * 1000
WHERE NOT EXISTS (SELECT 1 FROM quota_policy_revisions WHERE level = 8);
INSERT INTO quota_policy_revisions (id, level, single_file_max_bytes, total_bytes, daily_upload_bytes, retention_days, policy_version, created_by, created_at)
SELECT UUID(), 9, 20971520, 2147483648, 419430400, 30, 1, '01a07448-9000-7000-8000-000000000001', UNIX_TIMESTAMP() * 1000
WHERE NOT EXISTS (SELECT 1 FROM quota_policy_revisions WHERE level = 9);
INSERT INTO quota_policy_revisions (id, level, single_file_max_bytes, total_bytes, daily_upload_bytes, retention_days, policy_version, created_by, created_at)
SELECT UUID(), 10, 20971520, 2147483648, 419430400, 30, 1, '01a07448-9000-7000-8000-000000000001', UNIX_TIMESTAMP() * 1000
WHERE NOT EXISTS (SELECT 1 FROM quota_policy_revisions WHERE level = 10);

-- 3. 补齐 activity_rules（post/comment/reaction/task/leaderboard 奖励规则）
INSERT INTO activity_rules (id, kind, currency_id, amount, daily_limit, cooldown_seconds, conditions_json, version, is_enabled, created_at, updated_at)
SELECT UUID(), 'post', '01911fd5-0047-0000-0000-000000000002', 20, 5, 120, NULL, 1, 1, UNIX_TIMESTAMP() * 1000, UNIX_TIMESTAMP() * 1000
WHERE NOT EXISTS (SELECT 1 FROM activity_rules WHERE kind = 'post');
INSERT INTO activity_rules (id, kind, currency_id, amount, daily_limit, cooldown_seconds, conditions_json, version, is_enabled, created_at, updated_at)
SELECT UUID(), 'comment', '01911fd5-0047-0000-0000-000000000002', 10, 10, 120, NULL, 1, 1, UNIX_TIMESTAMP() * 1000, UNIX_TIMESTAMP() * 1000
WHERE NOT EXISTS (SELECT 1 FROM activity_rules WHERE kind = 'comment');
INSERT INTO activity_rules (id, kind, currency_id, amount, daily_limit, cooldown_seconds, conditions_json, version, is_enabled, created_at, updated_at)
SELECT UUID(), 'reaction', '01911fd5-0047-0000-0000-000000000002', 2, 20, NULL, NULL, 1, 1, UNIX_TIMESTAMP() * 1000, UNIX_TIMESTAMP() * 1000
WHERE NOT EXISTS (SELECT 1 FROM activity_rules WHERE kind = 'reaction');
INSERT INTO activity_rules (id, kind, currency_id, amount, daily_limit, cooldown_seconds, conditions_json, version, is_enabled, created_at, updated_at)
SELECT UUID(), 'task', '01911fd5-0047-0000-0000-000000000002', 50, 1, NULL, NULL, 1, 0, UNIX_TIMESTAMP() * 1000, UNIX_TIMESTAMP() * 1000
WHERE NOT EXISTS (SELECT 1 FROM activity_rules WHERE kind = 'task');
INSERT INTO activity_rules (id, kind, currency_id, amount, daily_limit, cooldown_seconds, conditions_json, version, is_enabled, created_at, updated_at)
SELECT UUID(), 'leaderboard', '01911fd5-0047-0000-0000-000000000002', 100, 1, NULL, NULL, 1, 0, UNIX_TIMESTAMP() * 1000, UNIX_TIMESTAMP() * 1000
WHERE NOT EXISTS (SELECT 1 FROM activity_rules WHERE kind = 'leaderboard');

-- 4. 迁移可配置装扮样式库 cosmetic_defs
INSERT INTO cosmetic_defs (id, kind, name, style_json, status, created_by, created_at, updated_at)
VALUES ('c01a0afd5f8b874439b25114ef91d9355', 'avatar_frame', 'Avatar frame: Lightning', '{"animate": "none", "borderStyle": "solid", "color": "#f472b6", "css": "/* Steam 动效头像框 - Avatar frame: Lightning */\nfilter: drop-shadow(0 0 6px rgba(102, 192, 244, 0.35));", "durationMs": 5000, "frameScale": 120, "glowPx": 10, "glowSpread": 0, "mode": "steam_frame", "shape": "rounded", "widthPx": 3, "url": "/api/v1/steam-assets/frames/fa436a2fe1eed769337795b27e30c4581627a36b.png", "image": "fa436a2fe1eed769337795b27e30c4581627a36b.png", "appid": 2496920}', 'active', '01a07448-9000-7000-8000-000000000001', UNIX_TIMESTAMP() * 1000, UNIX_TIMESTAMP() * 1000)
ON DUPLICATE KEY UPDATE
    name = VALUES(name),
    style_json = VALUES(style_json),
    status = VALUES(status),
    updated_at = VALUES(updated_at);
INSERT INTO cosmetic_defs (id, kind, name, style_json, status, created_by, created_at, updated_at)
VALUES ('c01a0b1a065447f92916272e14207ca7f', 'profile_effect', 'A City to Burn', '{"accentColor": "#8b5cf6", "animate": "none", "baseColor": "#090514", "css": "/* Steam 资料背景 - A City to Burn (AppID: 4246860) */\nopacity: 0.95;", "durationMs": 5000, "mode": "profile", "opacity": 100, "texture": "sparkle", "image": "ab78d1ab87ad1d93029960fea8ac848b31520de0.jpg", "webm": "/api/v1/steam-assets/backgrounds/3aadb770cd22a909a2c5f47b301aef8266fb2a76.webm", "mp4": "/api/v1/steam-assets/backgrounds/6b43f333f7d705a1e7c76df8f8bd640aaeb7b393.mp4", "appid": 4246860, "url": "/api/v1/steam-assets/backgrounds/ab78d1ab87ad1d93029960fea8ac848b31520de0.jpg"}', 'active', '01a07448-9000-7000-8000-000000000001', UNIX_TIMESTAMP() * 1000, UNIX_TIMESTAMP() * 1000)
ON DUPLICATE KEY UPDATE
    name = VALUES(name),
    style_json = VALUES(style_json),
    status = VALUES(status),
    updated_at = VALUES(updated_at);
INSERT INTO cosmetic_defs (id, kind, name, style_json, status, created_by, created_at, updated_at)
VALUES ('c01a0b1b20a077d23b78bb6348c9a43ac', 'profile_effect', 'Girls Animated Miniback', '{"accentColor": "#8b5cf6", "animate": "none", "baseColor": "#090514", "css": "/* Steam 资料背景 - Girls Animated Miniback (AppID: 4013320) */\nopacity: 0.95;", "durationMs": 5000, "mode": "profile", "opacity": 100, "texture": "sparkle", "image": "37377cf42c6c742f41dce45cfc5f0f5f6776b9f4.jpg", "webm": "/api/v1/steam-assets/backgrounds/517f3f3dfaadeccb40c98698558527cabb8700c1.webm", "mp4": "/api/v1/steam-assets/backgrounds/9f96be649ef4ccc6f74e373fd454541742d9d307.mp4", "appid": 3824080, "url": "/api/v1/steam-assets/backgrounds/37377cf42c6c742f41dce45cfc5f0f5f6776b9f4.jpg"}', 'active', '01a07448-9000-7000-8000-000000000001', UNIX_TIMESTAMP() * 1000, UNIX_TIMESTAMP() * 1000)
ON DUPLICATE KEY UPDATE
    name = VALUES(name),
    style_json = VALUES(style_json),
    status = VALUES(status),
    updated_at = VALUES(updated_at);
INSERT INTO cosmetic_defs (id, kind, name, style_json, status, created_by, created_at, updated_at)
VALUES ('c01a0b1bdc0f674f18fb2936395989bd5', 'profile_effect', 'Animated Heart Miniback', '{"accentColor": "#8b5cf6", "animate": "none", "baseColor": "#090514", "css": "/* Steam 资料背景 - Animated Heart Miniback (AppID: 3426440) */\nopacity: 0.95;", "durationMs": 5000, "mode": "profile", "opacity": 100, "texture": "sparkle", "image": "08eecb7e11de4875f51eeef31ba9f79fff27eb40.jpg", "webm": "/api/v1/steam-assets/backgrounds/ec5124df7e08dd310aa50687f4f87f40b5d4327c.webm", "mp4": "/api/v1/steam-assets/backgrounds/0b34dbbca6134a289efe864e312be6f1d3f33f84.mp4", "appid": 3426440, "url": "/api/v1/steam-assets/backgrounds/08eecb7e11de4875f51eeef31ba9f79fff27eb40.jpg"}', 'active', '01a07448-9000-7000-8000-000000000001', UNIX_TIMESTAMP() * 1000, UNIX_TIMESTAMP() * 1000)
ON DUPLICATE KEY UPDATE
    name = VALUES(name),
    style_json = VALUES(style_json),
    status = VALUES(status),
    updated_at = VALUES(updated_at);
INSERT INTO cosmetic_defs (id, kind, name, style_json, status, created_by, created_at, updated_at)
VALUES ('c01a0b26de71c7a218e04ba7761d09f8d', 'profile_effect', 'Mini Fleeting Moments', '{"accentColor": "#8b5cf6", "animate": "none", "baseColor": "#090514", "css": "/* Steam 资料背景 - Mini Fleeting Moments (AppID: 3180070) */\nopacity: 0.95;", "durationMs": 5000, "mode": "profile", "opacity": 100, "texture": "sparkle", "image": "4585d854fc3ac903e024cfd709d8a06344a0ae44.jpg", "webm": "/api/v1/steam-assets/backgrounds/5baf85c05765a097f1cb7456a8a394711aa18ddb.webm", "mp4": "/api/v1/steam-assets/backgrounds/5fe1024bc95f6e3edb51e8f1f9ab0f4b58b8ecc3.mp4", "appid": 3180070, "url": "/api/v1/steam-assets/backgrounds/4585d854fc3ac903e024cfd709d8a06344a0ae44.jpg"}', 'active', '01a07448-9000-7000-8000-000000000001', UNIX_TIMESTAMP() * 1000, UNIX_TIMESTAMP() * 1000)
ON DUPLICATE KEY UPDATE
    name = VALUES(name),
    style_json = VALUES(style_json),
    status = VALUES(status),
    updated_at = VALUES(updated_at);
INSERT INTO cosmetic_defs (id, kind, name, style_json, status, created_by, created_at, updated_at)
VALUES ('c01a0b2eab9977d81af456f204b7dd4a4', 'profile_effect', 'Casting Animated Miniback', '{"accentColor": "#8b5cf6", "animate": "none", "baseColor": "#090514", "css": "/* Steam 资料背景 - Casting Animated Miniback (AppID: 4602300) */\nopacity: 0.95;", "durationMs": 5000, "mode": "profile", "opacity": 100, "texture": "sparkle", "image": "9a87bd267cfe136b9d348e464e782e8c99bf7831.jpg", "webm": "/api/v1/steam-assets/backgrounds/f885628dbc09b86737429170b738cc87f13d996d.webm", "mp4": "/api/v1/steam-assets/backgrounds/a75d1179597534d1079db6d94fce032e4deef99e.mp4", "appid": 4602300, "url": "/api/v1/steam-assets/backgrounds/9a87bd267cfe136b9d348e464e782e8c99bf7831.jpg"}', 'active', '01a07448-9000-7000-8000-000000000001', UNIX_TIMESTAMP() * 1000, UNIX_TIMESTAMP() * 1000)
ON DUPLICATE KEY UPDATE
    name = VALUES(name),
    style_json = VALUES(style_json),
    status = VALUES(status),
    updated_at = VALUES(updated_at);
INSERT INTO cosmetic_defs (id, kind, name, style_json, status, created_by, created_at, updated_at)
VALUES ('c01a0c88717427101ba7e527787a72433', 'nickname_color', '梦幻色彩', '{"angle":90,"animate":"flow","colors":["#f43f5e","#f59e0b","#8b5cf6"],"durationMs":20000,"mode":"gradient"}', 'active', '01a07448-9000-7000-8000-000000000001', UNIX_TIMESTAMP() * 1000, UNIX_TIMESTAMP() * 1000)
ON DUPLICATE KEY UPDATE
    name = VALUES(name),
    style_json = VALUES(style_json),
    status = VALUES(status),
    updated_at = VALUES(updated_at);
INSERT INTO cosmetic_defs (id, kind, name, style_json, status, created_by, created_at, updated_at)
VALUES ('c01a0ffb223a3777082b6cabb218f005b', 'avatar_frame', '磁带', '{"color":"#f59e0b","css":"/* Steam 动效头像框 - Tape2.1 (AppID: 995680) */\nfilter: drop-shadow(0 0 6px rgba(102, 192, 244, 0.35));","frameScale":120,"image":"8e3d13440a5f4680a15a67621136164feb3eff13.png","mode":"steam_frame","shape":"rounded","url":"/api/v1/steam-assets/frames/8e3d13440a5f4680a15a67621136164feb3eff13.png"}', 'active', '01a07448-9000-7000-8000-000000000001', UNIX_TIMESTAMP() * 1000, UNIX_TIMESTAMP() * 1000)
ON DUPLICATE KEY UPDATE
    name = VALUES(name),
    style_json = VALUES(style_json),
    status = VALUES(status),
    updated_at = VALUES(updated_at);
