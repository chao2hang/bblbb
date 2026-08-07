// M07-UI-01：余额、等级、经验与签到状态安全投影（/me/balance）。
//
// - load：GET /activity/summary（等级/经验/签到/余额，字段缺失容忍）。
// - visit action：POST /activity/visit（幂等，每日首次有效访问自动签到；
//   前端也可显式领取）。429 → 返回 retryAfterSecs 供冷却提示。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, getAuthed } from '$lib/api/server';
import type { ActivitySummary, Money } from '$lib/api/types';

export interface BalancePageData {
  summary: ActivitySummary | null;
  error: string | null;
}

export interface BalanceActionData {
  ok?: boolean;
  message?: string;
  requestId?: string | null;
  retryAfterSecs?: number | null;
  todayEarned?: Money[];
  streakDays?: number;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<ActivitySummary>(cookies, '/api/v1/activity/summary', requestId);
  if (!result.ok && result.status === 401) throw redirect(303, '/login');
  if (!result.ok) {
    return { summary: null, error: result.message } satisfies BalancePageData;
  }
  return { summary: result.data, error: null } satisfies BalancePageData;
};

export const actions: Actions = {
  visit: async ({ request, cookies }) => {
    const form = await request.formData();
    const clientRequestId = String(form.get('client_request_id') ?? '').trim();
    if (clientRequestId.length < 16) {
      return fail(422, { message: '请求标识缺失，请刷新页面后重试' } satisfies BalanceActionData);
    }
    try {
      const result = await authedPost<{
        checked_in_today: boolean;
        streak_days: number;
        today_earned: Money[];
      }>(
        cookies,
        '/api/v1/activity/visit',
        { client_request_id: clientRequestId },
        request.headers.get('x-request-id'),
        { 'Idempotency-Key': clientRequestId }
      );
      if (result.ok) {
        return {
          ok: true,
          message: result.data.checked_in_today ? '签到成功' : '今日已签到',
          todayEarned: result.data.today_earned,
          streakDays: result.data.streak_days
        } satisfies BalanceActionData;
      }
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId,
        retryAfterSecs: result.retryAfterSecs
      } satisfies BalanceActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '签到服务暂不可用，请稍后重试' } satisfies BalanceActionData);
    }
  }
};
