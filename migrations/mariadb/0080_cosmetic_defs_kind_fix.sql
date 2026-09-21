-- 修复 cosmetic_defs kind CHECK 与代码枚举不一致（M07-SHOP-STUDIO，MariaDB）
--
-- 0079 的 CHECK 只允许 ('nickname_color','avatar_frame','profile_effect',
-- 'post_effect','badge')，而 API 层（cosmetics.rs CUSTOMIZABLE_KINDS）实际
-- 写入 cosmetic_badge / reaction_pack / utility / title_prefix，导致这四类
-- 样式定义被数据库拒绝。放宽为代码实际枚举（保留 'badge' 兼容历史种子）。

ALTER TABLE cosmetic_defs DROP CONSTRAINT cosmetic_defs_kind_ck;
ALTER TABLE cosmetic_defs ADD CONSTRAINT cosmetic_defs_kind_ck
    CHECK (kind IN ('nickname_color', 'avatar_frame', 'profile_effect', 'post_effect', 'cosmetic_badge', 'badge', 'reaction_pack', 'utility', 'title_prefix'));
