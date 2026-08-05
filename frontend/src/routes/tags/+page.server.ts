// M03-UI-06：标签页 SSR——服务端取标签与分组，按组展示；标签点击进入
// 搜索页标签筛选（?tag=slug，M03-UI-06 标签筛选入口；帖子级标签过滤在
// M8 搜索实现后收敛）。
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import type { Tag, TagGroup } from '$lib/api/types';

export interface TagsPageData {
  tags: Tag[];
  groups: TagGroup[];
  error: string | null;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<{ items: Tag[]; groups: TagGroup[] }>(
    cookies,
    '/api/v1/tags',
    requestId
  );
  if (!result.ok) {
    return { tags: [], groups: [], error: result.message } satisfies TagsPageData;
  }
  return { tags: result.data.items, groups: result.data.groups, error: null } satisfies TagsPageData;
};
