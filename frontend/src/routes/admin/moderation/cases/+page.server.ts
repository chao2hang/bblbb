// M05-UI-03：版主案件队列——列表 + 状态筛选。
import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import type { ModerationCaseItem } from '$lib/api/types';

export interface CasesPageData {
  items: ModerationCaseItem[];
  forbidden?: boolean;
  error?: string;
}

export const load: PageServerLoad = async (
  { url, cookies, request }
): Promise<CasesPageData> => {
  const requestId = request.headers.get('x-request-id');
  const status = url.searchParams.get('status') ?? '';
  const path = status ? `/api/v1/admin/moderation/cases?status=${encodeURIComponent(status)}` : '/api/v1/admin/moderation/cases';
  const result = await getAuthed<{ items: ModerationCaseItem[] }>(cookies, path, requestId);
  if (!result.ok && result.status === 401) throw redirect(303, '/login');
  if (!result.ok && result.status === 403) return { items: [], forbidden: true, error: result.message } satisfies CasesPageData;
  if (!result.ok) return { items: [], error: result.message } satisfies CasesPageData;
  return { items: result.data.items } satisfies CasesPageData;
};
