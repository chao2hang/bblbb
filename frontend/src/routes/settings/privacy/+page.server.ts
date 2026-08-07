// M08-UI-03/04：隐私与索引设置（/settings/privacy）。
//
// - load：GET /api/v1/me（登录态；401 → 登录）；
// - 逐帖退出搜索索引 / AI 摘要的实际控件位于每帖编辑器
//   （/editor，search_index_opt_out / ai_summary_opt_out 随草稿/发布提交，
//   见 M08-INDEX-03）；本页展示这些设置的位置、管理员全站/板块策略优先级
//   与 robots/索引状态说明（不承诺 robots 能阻止恶意抓取）。
import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import type { User } from '$lib/api/types';

export interface PrivacyPageData {
  user: User | null;
  error: string | null;
}

export const load: PageServerLoad = async ({ cookies, request }): Promise<PrivacyPageData> => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<User>(cookies, '/api/v1/me', requestId);
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    return { user: null, error: result.message } satisfies PrivacyPageData;
  }
  return { user: result.data, error: null } satisfies PrivacyPageData;
};
