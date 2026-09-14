// P1 整改：/admin/moderation/risk —— 风险审核策略（M05-RISK-08 管理入口）。
// - load：GET /api/v1/admin/moderation/risk-policy（admin.manage）；
// - save action：PATCH /api/v1/admin/moderation/risk-policy
//   （expected_version 乐观锁 + reason 审计；service 侧同事务写审计）。
import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPatch, getAuthed } from '$lib/api/server';

export interface RiskThresholds {
  new_user_max_posts: number;
  new_user_grace_secs: number;
  max_links: number;
  sensitive_words: string[];
  max_frequency_posts: number;
  frequency_window_secs: number;
  duplicate_window_secs: number;
}

export type AdminRiskState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminRiskPageData {
  state: AdminRiskState;
  version: number;
  thresholds: RiskThresholds | null;
  error: string | null;
}

export interface AdminRiskActionData {
  message?: string;
  conflict?: boolean;
}

export const load: PageServerLoad = async ({ cookies, request }): Promise<AdminRiskPageData> => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<{ version: number; thresholds: RiskThresholds }>(
    cookies,
    '/api/v1/admin/moderation/risk-policy',
    requestId
  );
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) {
      return { state: 'forbidden', version: 0, thresholds: null, error: result.message };
    }
    if (result.status === 501) {
      return { state: 'not_implemented', version: 0, thresholds: null, error: result.message };
    }
    return { state: 'error', version: 0, thresholds: null, error: result.message };
  }
  return {
    state: 'ok',
    version: result.data.version,
    thresholds: result.data.thresholds,
    error: null
  };
};

export const actions: Actions = {
  save: async ({ request, cookies }) => {
    const form = await request.formData();
    const version = Number(form.get('expected_version') ?? 0);
    const reason = String(form.get('reason') ?? '').trim();
    const sensitiveWords = String(form.get('sensitive_words') ?? '')
      .split(/[,，\n]/)
      .map((w) => w.trim())
      .filter(Boolean);
    const num = (key: string, min: number): number | null => {
      const v = Number(form.get(key) ?? NaN);
      return Number.isFinite(v) && v >= min ? v : null;
    };
    const newMax = num('new_user_max_posts', 0);
    const grace = num('new_user_grace_secs', 0);
    const maxLinks = num('max_links', 0);
    const freqMax = num('max_frequency_posts', 0);
    const freqWin = num('frequency_window_secs', 1);
    const dupWin = num('duplicate_window_secs', 1);
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' });
    if (version <= 0 || newMax === null || grace === null || maxLinks === null || freqMax === null || freqWin === null || dupWin === null) {
      return fail(422, { message: '参数缺失或非法' });
    }
    try {
      const result = await authedPatch(
        cookies,
        '/api/v1/admin/moderation/risk-policy',
        {
          expected_version: version,
          reason,
          thresholds: {
            new_user_max_posts: newMax,
            new_user_grace_secs: grace,
            max_links: maxLinks,
            sensitive_words: sensitiveWords,
            max_frequency_posts: freqMax,
            frequency_window_secs: freqWin,
            duplicate_window_secs: dupWin
          }
        },
        {},
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: '风险策略已保存（新版本生效）' };
      }
      if (result.status === 409) {
        return fail(409, {
          conflict: true,
          message: `版本冲突：${result.message}（策略已被其他人更新，请刷新后重试）`
        });
      }
      return fail(result.status, { message: result.message });
    } catch {
      return fail(503, { message: '保存失败，请稍后重试' });
    }
  }
};
