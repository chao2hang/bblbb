// GAP-FIX（管理域·通知管理）：/admin/notifications 重写——全站广播 + 发件箱。
// - load：GET /api/v1/admin/notifications/outbox（?after= cursor 分页）；
// - broadcast：POST /api/v1/admin/notifications/broadcast（title 1-100 /
//   body 1-2000，client_request_id 幂等；成功返回目标成员数）；
// - recall：POST /api/v1/admin/notifications/outbox/{id}/recall（reason 写审计）。
import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, getAuthed } from '$lib/api/server';
import { parseBatchIds, batchResult, type BatchOutcome } from '$lib/admin-batch';
import type { BroadcastItem } from '$lib/api/types';

export type AdminNotificationsState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

/** 通知模板注册表行（GET /admin/notifications/templates；代码内常量）。 */
export interface NotificationTemplate {
  id: string;
  name: string;
  trigger: string;
  channel: string;
  queue: string;
}

export interface AdminNotificationsPageData {
  state: AdminNotificationsState;
  items: BroadcastItem[] | null;
  nextCursor: string | null;
  after: string | null;
  error: string | null;
  templates: NotificationTemplate[];
  queue: { mode: string; failed: number; outbox_count: number } | null;
}

export interface AdminNotificationsActionData {
  message?: string;
  requestId?: string | null;
}

/** 幂等键（16-200 字符；与 client.ts newClientRequestId 同策略）。 */
function newRequestId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `bblbb-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 12)}`;
}

export const load: PageServerLoad = async ({
  cookies,
  request,
  url
}): Promise<AdminNotificationsPageData> => {
  const requestId = request.headers.get('x-request-id');
  const after = url.searchParams.get('after');

  const params = new URLSearchParams({ limit: '30' });
  if (after) params.set('after', after);

  const result = await getAuthed<{ items: BroadcastItem[]; next_cursor: string | null }>(
    cookies,
    `/api/v1/admin/notifications/outbox?${params.toString()}`,
    requestId
  );
  // 模板注册表 best-effort：失败不阻塞广播管理。
  const tplResult = await getAuthed<{
    templates: NotificationTemplate[];
    queue: { mode: string; failed: number; outbox_count: number };
  }>(cookies, '/api/v1/admin/notifications/templates', request.headers.get('x-request-id'));
  const templates = tplResult.ok ? tplResult.data.templates : [];
  const queue = tplResult.ok ? tplResult.data.queue : null;

  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) {
      return { state: 'forbidden', items: null, nextCursor: null, after, error: result.message, templates, queue };
    }
    if (result.status === 501) {
      return { state: 'not_implemented', items: null, nextCursor: null, after, error: result.message, templates, queue };
    }
    return { state: 'error', items: null, nextCursor: null, after, error: result.message, templates, queue };
  }
  return {
    state: 'ok',
    templates,
    queue,
    items: result.data.items,
    nextCursor: result.data.next_cursor,
    after,
    error: null
  };
};

export const actions: Actions = {
  broadcast: async ({ request, cookies }) => {
    const form = await request.formData();
    const title = String(form.get('title') ?? '').trim();
    const body = String(form.get('body') ?? '').trim();
    if (title.length < 1 || title.length > 100) {
      return fail(422, { message: '标题长度须为 1-100 字' });
    }
    if (body.length < 1 || body.length > 2000) {
      return fail(422, { message: '正文长度须为 1-2000 字' });
    }
    // 目标受众（M17-GAPFIX-07）：all（默认）| admins。
    const targetRaw = String(form.get('target') ?? '').trim();
    const target = targetRaw === 'admins' ? 'admins' : 'all';
    const clientRequestId = newRequestId();
    try {
      const result = await authedPost<{ id: string; target_count: number }>(
        cookies,
        '/api/v1/admin/notifications/broadcast',
        { title, body, target, client_request_id: clientRequestId },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `已发送给 ${result.data.target_count} 位成员` };
      }
      return fail(result.status, { message: result.message, requestId: result.requestId });
    } catch {
      return fail(503, { message: '发送失败，请稍后重试' });
    }
  },
  recall: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!id) return fail(422, { message: '缺少广播 ID' });
    if (!reason) return fail(422, { message: '撤回原因必填（写审计）' });
    try {
      const result = await authedPost<unknown>(
        cookies,
        `/api/v1/admin/notifications/outbox/${encodeURIComponent(id)}/recall`,
        { reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: '广播已撤回（未读通知已删除，已读保留）' };
      }
      return fail(result.status, { message: result.message, requestId: result.requestId });
    } catch {
      return fail(503, { message: '撤回失败，请稍后重试' });
    }
  },
  // M18-ADMIN-BATCH：批量撤回 = 循环调用与单条 recall 完全相同的既有端点
  // （POST /admin/notifications/outbox/{id}/recall，reason 写审计；该端点无
  // If-Match 乐观锁，BroadcastItem 亦无 version 字段，故不提交 versions）。
  batchRecall: async ({ request, cookies }) => {
    const form = await request.formData();
    const ids = parseBatchIds(form);
    const reason = String(form.get('reason') ?? '').trim();
    if (!reason) return fail(422, { message: '撤回原因必填（写审计）' });
    if (ids.length === 0) return fail(422, { message: '未选择任何广播' });
    const outcome: BatchOutcome = { okCount: 0, failures: [] };
    for (const id of ids) {
      try {
        const r = await authedPost<unknown>(
          cookies,
          `/api/v1/admin/notifications/outbox/${encodeURIComponent(id)}/recall`,
          { reason },
          request.headers.get('x-request-id')
        );
        if (r.ok) outcome.okCount++;
        else outcome.failures.push({ id, message: r.message });
      } catch {
        outcome.failures.push({ id, message: '网络错误' });
      }
    }
    const r = batchResult(outcome, '批量撤回广播');
    return r.ok ? { message: r.message } : fail(r.status, { message: r.message });
  }
};
