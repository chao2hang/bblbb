-- BBLBB seed data normalization (M01-DB-08)
-- 修复 0005 种子的跨库表示违约（不改写 0005 本身，保持不可变迁移）：
--   1. board id 归一化为合法 UUID v7（36 字符小写、时间有序、ASCII 二进制排序）；
--   2. created_at/updated_at 从 Unix 秒修正为 Unix 毫秒（BIGINT 毫秒契约）。
-- 0005 的种子尚无任何外键引用，重设 id 是安全的。

UPDATE boards SET
    id = '01911fd5-f000-7561-a2a5-3dd6434157f0',
    created_at = 1722816000000,
    updated_at = 1722816000000
WHERE id = '01jx5a00000000000000000001';

UPDATE boards SET
    id = '01911fd5-f001-758e-a95d-a58489fbb61d',
    created_at = 1722816000000,
    updated_at = 1722816000000
WHERE id = '01jx5a00000000000000000002';

UPDATE boards SET
    id = '01911fd5-f002-7222-8742-68e793fcdbd5',
    created_at = 1722816000000,
    updated_at = 1722816000000
WHERE id = '01jx5a00000000000000000003';

UPDATE boards SET
    id = '01911fd5-f003-7772-b594-c29b2b8c9021',
    created_at = 1722816000000,
    updated_at = 1722816000000
WHERE id = '01jx5a00000000000000000004';

UPDATE boards SET
    id = '01911fd5-f004-7d9c-b6c0-d2c3387e5534',
    created_at = 1722816000000,
    updated_at = 1722816000000
WHERE id = '01jx5a00000000000000000005';
