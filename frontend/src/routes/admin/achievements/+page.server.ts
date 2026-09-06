// GAP-FIX（管理域·成就管理）：/admin/achievements 新建页。
// client.ts 无 listAdminAchievements → 全部直连后端（server.ts getAuthed /
// authedPost / authedPatch / authedDeleteBody，路径与 GAP-FIX-SPEC 一节一致）：
// - load：GET /api/v1/admin/achievements（含隐藏条件与解锁计数；admin.manage）；
// - create：POST /api/v1/admin/achievements（code 唯一，reason 写审计）；
// - toggle：PATCH /api/v1/admin/achievements/{code} + If-Match version
//   （is_enabled 启停；409 冲突态提示刷新）；
// - grant：POST /api/v1/admin/achievements/{code}/grant（username + reason，
//   手工授予 manual 类成就 + 通知用户）；
// - delete：DELETE /api/v1/admin/achievements/{code} body {reason}（级联解锁记录）。
import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedDeleteBody, authedPatch, authedPost, getAuthed } from '$lib/api/server';

/** 解锁判定条件类型（achievements::evaluate 支持的钩子集合）。 */
const CONDITION_TYPES = [
  'post_count',
  'comment_count',
  'reaction_received',
  'checkin_streak',
  'follower_count',
  'manual'
] as const;

/** 管理端成就行（GET /api/v1/admin/achievements 投影；隐藏条件对管理员可见）。 */
export interface AdminAchievementItem {
  code: string;
  name: string;
  description: string;
  category: string;
  condition_type: string;
  condition_threshold: number;
  reward_exp: number;
  reward_coin: number;
  is_hidden: boolean;
  is_enabled: boolean;
  sort_order: number;
  unlocked_count: number;
  version: number;
}

export type AdminAchievementsState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminAchievementsPageData {
  state: AdminAchievementsState;
  items: AdminAchievementItem[] | null;
  error: string | null;
}

export interface AdminAchievementsActionData {
  message?: string;
  requestId?: string | null;
  conflict?: boolean;
}

export const load: PageServerLoad = async ({
  cookies,
  request
}): Promise<AdminAchievementsPageData> => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<{ items: AdminAchievementItem[] }>(
    cookies,
    '/api/v1/admin/achievements',
    requestId
  );
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) {
      return { state: 'forbidden', items: null, error: result.message };
    }
    if (result.status === 501) {
      return { state: 'not_implemented', items: null, error: result.message };
    }
    return { state: 'error', items: null, error: result.message };
  }
  return { state: 'ok', items: result.data.items, error: null };
};

function intField(form: FormData, name: string): number {
  return Number(form.get(name) ?? NaN);
}

export const actions: Actions = {
  create: async ({ request, cookies }) => {
    const form = await request.formData();
    const code = String(form.get('code') ?? '').trim();
    const name = String(form.get('name') ?? '').trim();
    const description = String(form.get('description') ?? '').trim();
    const category = String(form.get('category') ?? '').trim();
    const conditionType = String(form.get('condition_type') ?? '').trim();
    const conditionThreshold = intField(form, 'condition_threshold');
    const rewardExp = intField(form, 'reward_exp');
    const rewardCoin = intField(form, 'reward_coin');
    const sortOrder = intField(form, 'sort_order');
    const reason = String(form.get('reason') ?? '').trim();

    if (!/^[a-z0-9_-]{1,64}$/.test(code)) {
      return fail(422, { message: 'code 须为 1-64 位小写字母/数字/_/-' });
    }
    if (!name || [...name].length > 120) return fail(422, { message: '名称须为 1-120 字' });
    if (!description || [...description].length > 500) {
      return fail(422, { message: '描述须为 1-500 字' });
    }
    if (!category || [...category].length > 32) return fail(422, { message: '分类须为 1-32 字' });
    if (!(CONDITION_TYPES as readonly string[]).includes(conditionType)) {
      return fail(422, { message: `无效条件类型：${conditionType}` });
    }
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' });
    if (
      !Number.isInteger(conditionThreshold) ||
      conditionThreshold < 0 ||
      !Number.isInteger(rewardExp) ||
      rewardExp < 0 ||
      !Number.isInteger(rewardCoin) ||
      rewardCoin < 0 ||
      !Number.isInteger(sortOrder)
    ) {
      return fail(422, { message: '阈值/奖励/排序须为非负整数' });
    }

    try {
      const result = await authedPost<unknown>(
        cookies,
        '/api/v1/admin/achievements',
        {
          code,
          name,
          description,
          category,
          condition_type: conditionType,
          condition_threshold: conditionThreshold,
          reward_exp: rewardExp,
          reward_coin: rewardCoin,
          is_hidden: form.has('is_hidden'),
          is_enabled: form.has('is_enabled'),
          sort_order: sortOrder,
          reason
        },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `成就 ${code} 已创建` };
      }
      if (result.status === 409) {
        return fail(409, { conflict: true, message: `成就 code 已存在：${result.message}` });
      }
      return fail(result.status, { message: result.message, requestId: result.requestId });
    } catch {
      return fail(503, { message: '创建失败，请稍后重试' });
    }
  },

  toggle: async ({ request, cookies }) => {
    const form = await request.formData();
    const code = String(form.get('code') ?? '').trim();
    const version = Number(form.get('version') ?? 0);
    const nextEnabled = String(form.get('is_enabled') ?? '') === 'true';
    const reason = String(form.get('reason') ?? '').trim();
    if (!code) return fail(422, { message: '缺少成就 code' });
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' });
    try {
      const result = await authedPatch<unknown>(
        cookies,
        `/api/v1/admin/achievements/${encodeURIComponent(code)}`,
        { is_enabled: nextEnabled, reason },
        { 'If-Match': String(version) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `成就 ${code} 已${nextEnabled ? '启用' : '停用'}` };
      }
      if (result.status === 409) {
        return fail(409, {
          conflict: true,
          message: `版本冲突：${result.message}（成就已被修改，请刷新后重试）`
        });
      }
      return fail(result.status, { message: result.message, requestId: result.requestId });
    } catch {
      return fail(503, { message: '操作失败，请稍后重试' });
    }
  },

  /** 批量启停（M17-GAPFIX-07）：逐个 PATCH（If-Match 取当前版本），
   * 单个失败不中断；返回成功/失败清单。 */
  bulk: async ({ request, cookies }) => {
    const form = await request.formData();
    const codes = form.getAll('codes').map(String).map((c) => c.trim()).filter(Boolean);
    const nextEnabled = String(form.get('is_enabled') ?? '') === 'true';
    const reason = String(form.get('reason') ?? '').trim();
    if (codes.length === 0) return fail(422, { message: '请先勾选要操作的成就' });
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' });

    const requestId = request.headers.get('x-request-id');
    // 先取当前列表拿各 code 的 version（If-Match）。
    const listResult = await getAuthed<{ items: Array<{ code: string; version: number }> }>(
      cookies,
      '/api/v1/admin/achievements',
      requestId
    );
    const versionByCode = new Map(
      (listResult.ok ? listResult.data.items : []).map((i) => [i.code, i.version])
    );

    const ok: string[] = [];
    const failed: Array<{ code: string; message: string }> = [];
    for (const code of codes) {
      const version = versionByCode.get(code);
      if (!version) {
        failed.push({ code, message: '未找到（可能已被删除）' });
        continue;
      }
      try {
        const result = await authedPatch<unknown>(
          cookies,
          `/api/v1/admin/achievements/${encodeURIComponent(code)}`,
          { is_enabled: nextEnabled, reason },
          { 'If-Match': String(version) },
          requestId
        );
        if (result.ok) ok.push(code);
        else failed.push({ code, message: result.message });
      } catch {
        failed.push({ code, message: '请求失败' });
      }
    }
    if (failed.length) {
      return fail(207, {
        message: `批量${nextEnabled ? '启用' : '停用'}完成：成功 ${ok.length} 个，失败 ${failed.length} 个（${failed
          .slice(0, 3)
          .map((f) => `${f.code}: ${f.message}`)
          .join('；')}${failed.length > 3 ? '…' : ''}）`
      });
    }
    return { message: `批量${nextEnabled ? '启用' : '停用'}完成：共 ${ok.length} 个` };
  },

  grant: async ({ request, cookies }) => {
    const form = await request.formData();
    const code = String(form.get('code') ?? '').trim();
    const username = String(form.get('username') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!code) return fail(422, { message: '缺少成就 code' });
    if (!username) return fail(422, { message: '用户名必填' });
    if (!reason) return fail(422, { message: '授予原因必填（写审计）' });
    try {
      const result = await authedPost<{ code: string; unlocked_at: number }>(
        cookies,
        `/api/v1/admin/achievements/${encodeURIComponent(code)}/grant`,
        { username, reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `已将成就 ${code} 授予 ${username}（幂等：已解锁则保持原解锁时间）` };
      }
      return fail(result.status, { message: result.message, requestId: result.requestId });
    } catch {
      return fail(503, { message: '授予失败，请稍后重试' });
    }
  },

  delete: async ({ request, cookies }) => {
    const form = await request.formData();
    const code = String(form.get('code') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!code) return fail(422, { message: '缺少成就 code' });
    if (!reason) return fail(422, { message: '删除原因必填（写审计）' });
    try {
      const result = await authedDeleteBody<unknown>(
        cookies,
        `/api/v1/admin/achievements/${encodeURIComponent(code)}`,
        { reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `成就 ${code} 已删除（级联删除解锁记录）` };
      }
      return fail(result.status, { message: result.message, requestId: result.requestId });
    } catch {
      return fail(503, { message: '删除失败，请稍后重试' });
    }
  }
};
