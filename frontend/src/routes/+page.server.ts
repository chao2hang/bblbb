// M00-FRONTEND-08：无 JavaScript 基线 —— 公开阅读数据在 SSR 阶段取回。
// 无 JS 时首页仍能展示板块、热门标签与最新讨论（数据来自 SSR HTML）。
//
// M00-FRONTEND-09：hydration/预取隐私守卫 —— load 输出只保留公开字段白名单。
// 即使后端意外多返回了邮箱以外的凭据/令牌/隐藏正文，也不会进入 SSR HTML、
// hydration payload（__data.json）或客户端 store（预取与 load 共用同一输出）。
import { listBoards, listTags, search, type Board, type Tag, type PostSummary } from '$lib/api/client';
import type { PageServerLoad } from './$types';

/** 公开字段白名单：与 frontend/src/lib/api/types.ts 中的投影一一对应。
 *  任何新字段必须先过权限/隐私评审，再补充到白名单与类型。 */
const BOARD_PUBLIC = ['id', 'slug', 'name', 'description', 'post_count', 'is_active'] as const;
const TAG_PUBLIC = ['id', 'slug', 'name', 'usage_count'] as const;
const POST_PUBLIC = [
  'id',
  'title',
  'author_id',
  'reply_count',
  'view_count',
  'pinned',
  'created_at',
  'last_reply_at'
] as const;

/** 按白名单挑选字段：`Pick<T, K>` 保证漏掉字段会立即编译失败，无需运行时校验。 */
function pick<T extends object, K extends keyof T>(item: T, keys: readonly K[]): Pick<T, K> {
  const out = {} as Pick<T, K>;
  for (const key of keys) out[key] = item[key];
  return out;
}

export const load: PageServerLoad = async ({ fetch }) => {
  const [boards, tags, posts] = await Promise.allSettled([
    listBoards(fetch),
    listTags(fetch),
    search(fetch, '', 8)
  ]);

  return {
    boards:
      boards.status === 'fulfilled'
        ? boards.value.items.map((b: Board) => pick(b, BOARD_PUBLIC))
        : [],
    tags:
      tags.status === 'fulfilled'
        ? tags.value.items.map((t: Tag) => pick(t, TAG_PUBLIC))
        : [],
    posts:
      posts.status === 'fulfilled'
        ? posts.value.items.map((p: PostSummary) => pick(p, POST_PUBLIC))
        : []
  };
};