import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import type { TrustLevelProgress } from '$lib/api/types';

export interface LevelPageData {
  trust: TrustLevelProgress | null;
  error: string | null;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const trustResult = await getAuthed<TrustLevelProgress>(cookies, '/api/v1/me/trust-level', requestId);

  if (!trustResult.ok && trustResult.status === 401) {
    throw redirect(303, '/login?next=/me/level');
  }

  return {
    trust: trustResult.ok ? trustResult.data : null,
    error: trustResult.ok ? null : trustResult.message
  } satisfies LevelPageData;
};
