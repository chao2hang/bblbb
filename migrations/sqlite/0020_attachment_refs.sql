-- BBLBB 头像/Cover 稳定 attachment_id 引用迁移（M03-SCHEMA-02）
-- users 增加 cover_attachment_id（与 0019 的 avatar_attachment_id 对齐）。
-- 头像与 Cover 只保存附件 UUID（attachments 表 M6 落地后补 FK），
-- 禁止保存远程 URL 或签名 URL——格式与来源校验在 M3-PROFILE 服务层
-- （ProfileCoverSet.attachment_id format: uuid），DB 不做跨库 URL 判定。

ALTER TABLE users ADD COLUMN cover_attachment_id TEXT;
