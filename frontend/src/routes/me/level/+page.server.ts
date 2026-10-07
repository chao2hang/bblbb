import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import type { TrustLevelProgress, User } from '$lib/api/types';

export interface LevelPageData {
  trust: TrustLevelProgress | null;
  /** 会话安全投影：仅供 MeIdentityBar 身份条展示（昵称/用户名/头像/等级） */
  user: User | null;
  error: string | null;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  // 信任进度与当前用户投影并行获取（身份条展示用，失败不阻塞等级内容）
  const [trustResult, meResult] = await Promise.all([
    getAuthed<TrustLevelProgress>(cookies, '/api/v1/me/trust-level', requestId),
    getAuthed<User>(cookies, '/api/v1/me', requestId)
  ]);

  if (!trustResult.ok && trustResult.status === 401) {
    throw redirect(303, '/login?next=/me/level');
  }

  return {
    trust: trustResult.ok ? trustResult.data : null,
    user: meResult.ok ? meResult.data : null,
    error: trustResult.ok ? null : trustResult.message
  } satisfies LevelPageData;
};
