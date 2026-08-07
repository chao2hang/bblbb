// M07-UI-08：管理端活跃——签到/任务配置（If-Match 版本冲突提示）。
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
      check_in_enabled: form.get('check_in_enabled') === 'on'
    };
    const amountRaw = String(form.get('check_in_amount') ?? '').trim();
    if (amountRaw !== '') {
      changes.check_in_reward = { currency: 'coin', amount: Number(amountRaw) } satisfies Money;
    }
    try {
      const result = await authedPatch<ActivityConfig>(
        cookies,
        '/api/v1/admin/activity/config',
        { expected_version: expectedVersion, reason, changes },
        { 'If-Match': String(expectedVersion) },
        request.headers.get('x-request-id')
      );
      if (result.ok) return { message: '活跃配置已保存' } satisfies AdminActivityActionData;
      if (result.status === 409) {
        return fail(409, { message: `版本冲突：${result.message}` } satisfies AdminActivityActionData);
      }
      return fail(result.status, { message: result.message } satisfies AdminActivityActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '保存失败，请稍后重试' } satisfies AdminActivityActionData);
    }
  },
  'create-task': async ({ request, cookies }) => {
    const form = await request.formData();
    const reason = String(form.get('reason') ?? '').trim();
    if (!reason) {
      return fail(422, { message: '操作原因必填' } satisfies AdminActivityActionData);
    }
    const body: Record<string, unknown> = {
      reason,
      kind: String(form.get('kind') ?? 'task'),
      currency: String(form.get('currency') ?? 'coin'),
      amount: Number(form.get('amount') ?? 0)
    };
    const daily = String(form.get('daily_limit') ?? '').trim();
    if (daily !== '') body.daily_limit = Number(daily);
    try {
      const result = await authedPost<ActivityTask>(
        cookies,
        '/api/v1/admin/activity/tasks',
        body,
        request.headers.get('x-request-id')
      );
      if (result.ok) return { message: '任务已创建' } satisfies AdminActivityActionData;
      return fail(result.status, { message: result.message } satisfies AdminActivityActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '创建失败，请稍后重试' } satisfies AdminActivityActionData);
    }
  },
  'update-task': async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const version = Number(form.get('version') ?? 0);
    if (!id) return fail(422, { message: '缺少任务标识' } satisfies AdminActivityActionData);
    if (!Number.isInteger(version) || version < 1) {
      return fail(422, { message: '任务版本缺失或无效' } satisfies AdminActivityActionData);
    }
    const body: Record<string, unknown> = {
      reason: String(form.get('reason') ?? '').trim(),
      is_enabled: form.get('is_enabled') === 'on'
    };
    const amount = String(form.get('amount') ?? '').trim();
    if (amount !== '') body.amount = Number(amount);
    try {
      const result = await authedPatch<ActivityTask>(
        cookies,
        `/api/v1/admin/activity/tasks/${encodeURIComponent(id)}`,
        body,
        { 'If-Match': String(version) },
        request.headers.get('x-request-id')
      );
      if (result.ok) return { message: '任务已更新' } satisfies AdminActivityActionData;
      if (result.status === 409) {
        return fail(409, { message: `版本冲突：${result.message}` } satisfies AdminActivityActionData);
      }
      return fail(result.status, { message: result.message } satisfies AdminActivityActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '更新失败，请稍后重试' } satisfies AdminActivityActionData);
    }
  }
};
