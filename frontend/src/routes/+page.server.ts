// M00-FRONTEND-08：无 JavaScript 基线 —— 公开阅读数据在 SSR 阶段取回。
// 无 JS 时首页仍能展示板块、热门标签与最新讨论（数据来自 SSR HTML）。
import { listBoards, listTags, search } from '$lib/api/client';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ fetch }) => {
  const [boards, tags, posts] = await Promise.allSettled([
    listBoards(fetch),
    listTags(fetch),
    search(fetch, '', 8)
  ]);

  return {
    boards: boards.status === 'fulfilled' ? boards.value.items : [],
    tags: tags.status === 'fulfilled' ? tags.value.items : [],
    posts: posts.status === 'fulfilled' ? posts.value.items : []
  };
};