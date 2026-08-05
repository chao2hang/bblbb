// M03-UI-01：用户主页 SSR load——公开资料安全投影
//
// - 服务端转发 GET /api/v1/users/{username}（getAuthed：转发会话 Cookie 与
//   X-Request-ID，回传 Set-Cookie 属性，解析 RFC 7807 Problem）；
// - 404（不存在/已注销/匿名化）→ SvelteKit error(404)——与后端一致不泄漏
//   存在性；5xx → error(500)；其余 → error(status)；
// - banned/pending_delete → 后端 200 安全降级投影（bio/signature/头像/Cover
//   置空，不泄漏状态），页面按公开投影渲染降级态；
// - 返回类型仅 PUBLIC_PROFILE allowlist 九字段（$lib/api/types PublicProfile）。
import { error } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import type { PublicProfile } from '$lib/api/types';

export interface UserPageData {
  user: PublicProfile;
}

export const load: PageServerLoad = async ({ params, cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const username = params.username;
  const result = await getAuthed<PublicProfile>(
    cookies,
    `/api/v1/users/${encodeURIComponent(username)}`,
    requestId
  );
  if (result.ok) {
    return { user: result.data } satisfies UserPageData;
  }
  if (result.status === 404) {
    throw error(404, '用户不存在或已注销');
  }
  if (result.status >= 500) {
    throw error(500, '服务暂时不可用，请稍后重试');
  }
  throw error(result.status, result.message || '获取用户资料失败');
};
