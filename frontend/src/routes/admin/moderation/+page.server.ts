// 统一审核治理中心（Moderation Center）
// 聚合「待发内容审核（前置）」与「用户举报案件（后置）」于同一工作台。
import { fail, redirect, type Cookies } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPatch, authedPost, getAuthed } from '$lib/api/server';
import { parseBatchIds, batchResult, type BatchOutcome } from '$lib/admin-batch';
import type { ModerationCaseItem } from '$lib/api/types';
import type {
  AdminContentPost,
  AdminContentRevision,
  AdminContentDiff,
  AdminContentPageData
} from '../content/+page.server';
import type { CasesPageData } from './cases/+page.server';

export interface ModerationCenterStats {
  pendingContentCount: number;
  openCasesCount: number;
  totalPending: number;
}

export interface ModerationCenterPageData {
  activeTab: 'content' | 'cases';
  content: AdminContentPageData;
  cases: CasesPageData;
  stats: ModerationCenterStats;
}

const CASE_STATUSES = ['open', 'triaged', 'investigating', 'resolved', 'rejected', 'reopened'] as const;

async function transitionEachCase(
  cookies: Cookies,
  ids: string[],
  status: string,
  resolution: string | null,
  requestId: string | null
): Promise<BatchOutcome> {
  const outcome: BatchOutcome = { okCount: 0, failures: [] };
  for (const id of ids) {
    try {
      const r = await authedPatch<{ id: string; status: string }>(
        cookies,
        `/api/v1/admin/moderation/cases/${encodeURIComponent(id)}`,
        { status, resolution },
        {},
        requestId
      );
      if (r.ok) outcome.okCount++;
      else outcome.failures.push({ id, message: r.message });
    } catch {
      outcome.failures.push({ id, message: '网络错误' });
    }
  }
  return outcome;
}

export const load: PageServerLoad = async ({ cookies, request, url }) => {
  const requestId = request.headers.get('x-request-id');
  const rawTab = url.searchParams.get('tab');
  const activeTab: 'content' | 'cases' = rawTab === 'cases' ? 'cases' : 'content';

  // 1. 获取待审内容队列 (status=pending_review)
  const postsResult = await getAuthed<{ items: AdminContentPost[] }>(
    cookies,
    '/api/v1/admin/posts?status=pending_review&limit=30',
    requestId
  );
  if (!postsResult.ok && postsResult.status === 401) throw redirect(303, '/login');

  const contentPosts = postsResult.ok ? (postsResult.data.items ?? []) : [];
  const requestedPostId = url.searchParams.get('post');
  const currentPost = contentPosts.find((p) => p.id === requestedPostId) ?? contentPosts[0] ?? null;

  let diff: AdminContentDiff | null = null;
  let diff_error: string | null = null;
  if (currentPost) {
    const revisions = await getAuthed<{ items: AdminContentRevision[] }>(
      cookies,
      `/api/v1/admin/posts/${encodeURIComponent(currentPost.id)}/revisions`,
      requestId
    );
    if (!revisions.ok) {
      diff_error = `无法获取修订快照（${revisions.status}）：${revisions.message ?? '未知错误'}`;
    } else {
      const withBody = (revisions.data.items ?? []).filter(
        (r): r is AdminContentRevision & { body_markdown: string } =>
          typeof r.body_markdown === 'string' && r.body_markdown.trim().length > 0
      );
      if (withBody.length === 0) {
        diff_error = '该帖子没有任何修订快照，无法生成版本对比';
      } else {
        const after = withBody[withBody.length - 1];
        const before = withBody.length > 1 ? withBody[withBody.length - 2] : null;
        diff = {
          from_version: before ? before.version : null,
          to_version: after.version,
          reason: after.reason ?? null,
          before_body: before ? before.body_markdown : null,
          after_body: after.body_markdown,
          after_html: after.body_html ?? null
        };
      }
    }
  }

  const contentState: AdminContentPageData = postsResult.ok
    ? {
        posts: contentPosts,
        current: currentPost,
        diff,
        diff_error,
        error: null,
        state: 'ok'
      }
    : {
        posts: [],
        current: null,
        diff: null,
        diff_error: null,
        error: postsResult.message,
        state: postsResult.status === 403 ? 'forbidden' : 'error'
      };

  // 2. 获取举报案件列表
  const caseStatus = url.searchParams.get('status') ?? '';
  const casesPath = caseStatus
    ? `/api/v1/admin/moderation/cases?status=${encodeURIComponent(caseStatus)}`
    : '/api/v1/admin/moderation/cases';
  const casesResult = await getAuthed<{ items: ModerationCaseItem[] }>(cookies, casesPath, requestId);
  if (!casesResult.ok && casesResult.status === 401) throw redirect(303, '/login');

  const casesData: CasesPageData = casesResult.ok
    ? { items: casesResult.data.items ?? [] }
    : {
        items: [],
        forbidden: casesResult.status === 403,
        error: casesResult.message
      };

  const pendingContentCount = contentPosts.length;
  const openCasesCount = (casesData.items ?? []).filter(
    (c) => c.status === 'open' || c.status === 'triaged' || c.status === 'investigating'
  ).length;

  return {
    activeTab,
    content: contentState,
    cases: casesData,
    stats: {
      pendingContentCount,
      openCasesCount,
      totalPending: pendingContentCount + openCasesCount
    }
  } satisfies ModerationCenterPageData;
};

export const actions: Actions = {
  // ── 内容审核动作 ──
  approve: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const reason = String(form.get('reason') ?? '内容合规，同意发布').trim() || '内容合规，同意发布';
    try {
      const result = await authedPost(
        cookies,
        `/api/v1/admin/posts/${encodeURIComponent(id)}/action`,
        { action: 'approve', reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) return { success: true, tab: 'content', message: '已通过审核并公开发布' };
      return fail(result.status, { success: false, tab: 'content', error: result.message });
    } catch {
      return fail(503, { success: false, tab: 'content', error: '操作失败，请稍后重试' });
    }
  },
  reject: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!reason) {
      return fail(422, { success: false, tab: 'content', error: '驳回必须填写驳回原因' });
    }
    try {
      const result = await authedPost(
        cookies,
        `/api/v1/admin/posts/${encodeURIComponent(id)}/action`,
        { action: 'reject', reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) return { success: true, tab: 'content', message: '已驳回并退回草稿' };
      return fail(result.status, { success: false, tab: 'content', error: result.message });
    } catch {
      return fail(503, { success: false, tab: 'content', error: '操作失败，请稍后重试' });
    }
  },

  // ── 案件批量操作动作 ──
  batchStatus: async ({ request, cookies }) => {
    const form = await request.formData();
    const ids = parseBatchIds(form);
    const status = String(form.get('status') ?? '').trim();
    if (!ids.length) return fail(422, { tab: 'cases', message: '未选择任何案件' });
    if (!(CASE_STATUSES as readonly string[]).includes(status)) {
      return fail(422, { tab: 'cases', message: '缺少合法的目标状态' });
    }
    const outcome = await transitionEachCase(cookies, ids, status, null, request.headers.get('x-request-id'));
    const r = batchResult(outcome, '批量标记处理中');
    return r.ok ? { success: true, tab: 'cases', message: r.message } : fail(r.status, { success: false, tab: 'cases', message: r.message });
  },
  batchClose: async ({ request, cookies }) => {
    const form = await request.formData();
    const ids = parseBatchIds(form);
    const reason = String(form.get('reason') ?? '').trim();
    if (!ids.length) return fail(422, { tab: 'cases', message: '未选择任何案件' });
    if (!reason) return fail(422, { tab: 'cases', message: '关闭原因必填（写审计）' });
    const outcome = await transitionEachCase(cookies, ids, 'resolved', reason, request.headers.get('x-request-id'));
    const r = batchResult(outcome, '批量关闭案件');
    return r.ok ? { success: true, tab: 'cases', message: r.message } : fail(r.status, { success: false, tab: 'cases', message: r.message });
  },
  batchReject: async ({ request, cookies }) => {
    const form = await request.formData();
    const ids = parseBatchIds(form);
    const reason = String(form.get('reason') ?? '').trim();
    if (!ids.length) return fail(422, { tab: 'cases', message: '未选择任何案件' });
    if (!reason) return fail(422, { tab: 'cases', message: '驳回原因必填（写审计）' });
    const outcome = await transitionEachCase(cookies, ids, 'rejected', reason, request.headers.get('x-request-id'));
    const r = batchResult(outcome, '批量驳回案件');
    return r.ok ? { success: true, tab: 'cases', message: r.message } : fail(r.status, { success: false, tab: 'cases', message: r.message });
  }
};
