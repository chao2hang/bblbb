// M07-UI-08 + M18-ADMIN-POINTS-03 优化：管理端签到页——
// 角色收敛为「签到运行概览 + 签到全局配置」：
// - load：GET /admin/activity/config（If-Match 版本 + 总闸/自动打卡/时区）+
//   GET /admin/activity/tasks（只用于其他活跃规则的只读摘要计数）；
// - save-config：PATCH /admin/activity/config（reason 必填 + step-up + 审计）。
// 任务规则（发帖/回复/表态/任务/榜单）的数额/上限/冷却/启停编辑统一移至
// 「积分规则配置」/admin/points/rules（单一编辑面，避免两处改同一 activity_rules
// 行造成版本打架）；本页仅展示摘要计数与跳转入口。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPatch, authedPost, getAuthed } from '$lib/api/server';
import { adminListState, type AdminLoadState } from '$lib/admin';
import type { ActivityConfig, ActivityTask, Money } from '$lib/api/types';

export interface AdminActivityPageData {
  config: { state: 'ok'; data: ActivityConfig } | { state: 'error' | 'forbidden' | 'not_implemented'; message: string };
  tasks: AdminLoadState<ActivityTask>;
}

/** form action 返回投影。 */
export interface AdminActivityActionData {
  message?: string;
  requestId?: string | null;
  stepUpRequired?: boolean;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const configResult = await getAuthed<ActivityConfig>(cookies, '/api/v1/admin/activity/config', requestId);
  if (!configResult.ok && configResult.status === 401) throw redirect(303, '/login');
  let config: AdminActivityPageData['config'];
  if (configResult.ok) {
    config = { state: 'ok', data: configResult.data };
  } else {
    config = { state: configResult.status === 403 ? 'forbidden' : 'error', message: configResult.message };
  }

  const tasksResult = await getAuthed<{ items: ActivityTask[] }>(cookies, '/api/v1/admin/activity/tasks', requestId);
  const tasks = adminListState(tasksResult);
  return { config, tasks } satisfies AdminActivityPageData;
};

export const actions: Actions = {
  'save-config': async ({ request, cookies }) => {
    const form = await request.formData();
    const expectedVersion = Number(form.get('expected_version') ?? 0);
    const reason = String(form.get('reason') ?? '').trim();
    if (!Number.isInteger(expectedVersion) || expectedVersion < 1) {
      return fail(422, { message: '配置版本缺失或无效，请刷新后重试' } satisfies AdminActivityActionData);
    }
    if (!reason) {
      return fail(422, { message: '操作原因必填' } satisfies AdminActivityActionData);
    }
    const changes: Record<string, unknown> = {
      check_in_enabled: form.get('check_in_enabled') === 'on',
      auto_check_in_enabled: form.get('auto_check_in_enabled') === 'on',
      rewards_enabled: form.get('rewards_enabled') === 'on',
      site_timezone: String(form.get('site_timezone') ?? 'Asia/Shanghai').trim(),
      day_reset_hour: Number(form.get('day_reset_hour') ?? 0)
    };
    const amountRaw = String(form.get('check_in_amount') ?? '').trim();
    const currency = String(form.get('check_in_currency') ?? 'coin').trim().toLowerCase();
    // 管理端签到奖励统一使用 B币；拒绝任何其他币种，避免绕过仅 coin 的表单选项。
    if (currency !== 'coin') {
      return fail(422, { message: '签到奖励币种仅支持 B币' } satisfies AdminActivityActionData);
    }
    changes.check_in_currency = 'coin';
    if (amountRaw !== '') {
      changes.check_in_amount = Number(amountRaw);
      changes.check_in_reward = { currency: 'coin', amount: Number(amountRaw) } satisfies Money;
    }
    const limitRaw = String(form.get('check_in_daily_limit') ?? '').trim();
    if (limitRaw !== '') {
      changes.check_in_daily_limit = Number(limitRaw);
    }
    try {
      const result = await authedPatch<ActivityConfig>(
        cookies,
        '/api/v1/admin/activity/config',
        { expected_version: expectedVersion, reason, ...changes, changes },
        { 'If-Match': String(expectedVersion) },
        request.headers.get('x-request-id')
      );
      if (result.ok) return { message: '签到与活跃配置已保存' } satisfies AdminActivityActionData;
      if (result.code === 'step_up_required') {
        return fail(403, {
          message: '此操作需要重新验证身份，请输入密码重新验证后重试',
          stepUpRequired: true
        } satisfies AdminActivityActionData);
      }
      if (result.status === 409) {
        return fail(409, { message: `版本冲突：${result.message}` } satisfies AdminActivityActionData);
      }
      return fail(result.status, { message: result.message, requestId: result.requestId } satisfies AdminActivityActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '保存失败，请稍后重试' } satisfies AdminActivityActionData);
    }
  },

  /** 重新验证身份（step-up 窗口过期后；与 storage/roles 页同款交互）。 */
  reauth: async ({ request, cookies }) => {
    const form = await request.formData();
    const password = String(form.get('password') ?? '');
    if (!password) {
      return fail(422, {
        message: '请输入当前密码'
      } satisfies AdminActivityActionData);
    }
    try {
      const result = await authedPost(
        cookies,
        '/api/v1/auth/re-auth',
        { password },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return {
          message: '已重新验证身份，请重试刚才的操作'
        } satisfies AdminActivityActionData;
      }
      return fail(result.status, {
        message: result.message
      } satisfies AdminActivityActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, {
        message: '验证失败，请稍后重试'
      } satisfies AdminActivityActionData);
    }
  }
};
