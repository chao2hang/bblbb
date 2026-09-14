// GAP-FIX（管理域·附件管理）：/admin/attachments 重写——附件列表 + 删除
// （替换原存储配置脱敏视图；存储配置仍在 /admin/storage）。
// - load：GET /api/v1/admin/attachments（?q= 文件名/上传者模糊 + ?after= 分页）；
// - delete：DELETE /api/v1/admin/attachments/{id} body {reason}（软删除 +
//   reason 写审计；storage.manage 权限门）。
import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedDeleteBody, authedPatch, getAuthed } from '$lib/api/server';
import { parseBatchIds, batchResult, type BatchOutcome } from '$lib/admin-batch';
import type { AdminAttachmentItem } from '$lib/api/types';

export type AdminAttachmentsState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminAttachmentsPageData {
  state: AdminAttachmentsState;
  items: AdminAttachmentItem[] | null;
  nextCursor: string | null;
  q: string;
  after: string | null;
  error: string | null;
}

export interface AdminAttachmentsActionData {
  message?: string;
  requestId?: string | null;
}

export const load: PageServerLoad = async ({
  cookies,
  request,
  url
}): Promise<AdminAttachmentsPageData> => {
  const requestId = request.headers.get('x-request-id');
  const q = (url.searchParams.get('q') ?? '').trim();
  const after = url.searchParams.get('after');

  const params = new URLSearchParams({ limit: '30' });
  if (q) params.set('q', q);
  if (after) params.set('after', after);

  const result = await getAuthed<{ items: AdminAttachmentItem[]; next_cursor: string | null }>(
    cookies,
    `/api/v1/admin/attachments?${params.toString()}`,
    requestId
  );
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) {
      return { state: 'forbidden', items: null, nextCursor: null, q, after, error: result.message };
    }
    if (result.status === 501) {
      return { state: 'not_implemented', items: null, nextCursor: null, q, after, error: result.message };
    }
    return { state: 'error', items: null, nextCursor: null, q, after, error: result.message };
  }
  return {
    state: 'ok',
    items: result.data.items,
    nextCursor: result.data.next_cursor,
    q,
    after,
    error: null
  };
};

export const actions: Actions = {
  delete: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!id) return fail(422, { message: '缺少附件 ID' });
    if (!reason) return fail(422, { message: '删除原因必填（写审计）' });
    try {
      const result = await authedDeleteBody<unknown>(
        cookies,
        `/api/v1/admin/attachments/${encodeURIComponent(id)}`,
        { reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `附件 ${id} 已删除（软删除，记录保留）` };
      }
      return fail(result.status, { message: result.message, requestId: result.requestId });
    } catch {
      return fail(503, { message: '删除失败，请稍后重试' });
    }
  },
  // M18-ADMIN-BATCH：批量删除 = 循环调用与单条 delete 完全相同的既有端点
  // （DELETE /admin/attachments/{id} body {reason}，软删除 + reason 写审计；
  // 该端点无 If-Match 乐观锁，AdminAttachmentItem 亦无 version 字段，故不提交 versions）。
  batchDelete: async ({ request, cookies }) => {
    const form = await request.formData();
    const ids = parseBatchIds(form);
    const reason = String(form.get('reason') ?? '').trim();
    if (!reason) return fail(422, { message: '删除原因必填（写审计）' });
    if (ids.length === 0) return fail(422, { message: '未选择任何附件' });
    const outcome: BatchOutcome = { okCount: 0, failures: [] };
    for (const id of ids) {
      try {
        const r = await authedDeleteBody<unknown>(
          cookies,
          `/api/v1/admin/attachments/${encodeURIComponent(id)}`,
          { reason },
          request.headers.get('x-request-id')
        );
        if (r.ok) outcome.okCount++;
        else outcome.failures.push({ id, message: r.message });
      } catch {
        outcome.failures.push({ id, message: '网络错误' });
      }
    }
    const r = batchResult(outcome, '批量删除附件');
    return r.ok ? { message: r.message } : fail(r.status, { message: r.message });
  },

  /** 快速封禁恶意上传者（可同时勾选软删除当前违规附件）。 */
  banUser: async ({ request, cookies }) => {
    const form = await request.formData();
    const username = String(form.get('username') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    const deleteAttachmentId = String(form.get('delete_attachment_id') ?? '').trim();

    if (!username) return fail(422, { message: '缺少用户名' });
    if (!reason) return fail(422, { message: '封禁原因必填（写审计）' });

    const requestId = request.headers.get('x-request-id');

    // 1. 查询用户以获取用户真实 id 与 If-Match 乐规锁版本
    const userResult = await getAuthed<{ items: Array<{ id: string; username: string; status: string; version: number }> }>(
      cookies,
      `/api/v1/admin/users?q=${encodeURIComponent(username)}`,
      requestId
    );

    if (!userResult.ok || !userResult.data?.items || userResult.data.items.length === 0) {
      return fail(404, { message: `未检索到用户「${username}」` });
    }

    const targetUser = userResult.data.items.find(
      (u) => u.username.toLowerCase() === username.toLowerCase()
    ) ?? userResult.data.items[0];

    if (targetUser.status === 'banned') {
      return fail(400, { message: `用户「${username}」已处于封禁状态` });
    }

    // 2. 封禁用户：PATCH /api/v1/admin/users/{id}
    const banResult = await authedPatch<unknown>(
      cookies,
      `/api/v1/admin/users/${encodeURIComponent(targetUser.id)}`,
      { status: 'banned', reason: `附件管理处置违规用户: ${reason}` },
      { 'If-Match': String(targetUser.version) },
      requestId
    );

    if (!banResult.ok) {
      return fail(banResult.status, { message: banResult.message, requestId: banResult.requestId });
    }

    // 3. 若勾选删除违规附件，一并执行软删除
    if (deleteAttachmentId) {
      try {
        await authedDeleteBody<unknown>(
          cookies,
          `/api/v1/admin/attachments/${encodeURIComponent(deleteAttachmentId)}`,
          { reason: `封禁违规用户一并清理: ${reason}` },
          requestId
        );
      } catch {
        // 软删除失败不阻止封禁成功的主结果
      }
    }

    return {
      message: `已成功封禁违规用户「${username}」${deleteAttachmentId ? '，并清理对应违规附件' : ''}`
    };
  }
};
