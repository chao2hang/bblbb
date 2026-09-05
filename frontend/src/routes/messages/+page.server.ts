// GAP-FIX（社交域·私信，conversations.rs）：/messages——会话列表 + 选中
// 线程（?c=）SSR。
//
// - load：未登录 → /login；listConversations（左栏）；URL 带 ?c=<id> 时
//   listMessages（created_at ASC）并 readConversation 标记已读
//   （best-effort，失败只影响角标，不影响线程展示）。
// - send action：POST sendMessage（body 1-2000 + client_request_id 幂等）；
//   页面 enhance 回调成功后 invalidateAll 刷新线程并滚到底部。
// - 失败态：load 不抛 500，返回 Problem（ProblemState 渲染）。
import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import {
  listConversations,
  listMessages,
  newClientRequestId,
  readConversation,
  sendMessage
} from '$lib/api/client';
import type { ConversationItem, ConversationMessage } from '$lib/api/types';
import { problemMessage, type Problem } from '$lib/errors';

export interface MessagesPageData {
  conversations: ConversationItem[];
  /** URL ?c=<id> 选中的会话 id（无则 null）。 */
  conversationId: string | null;
  /** 选中会话的对方投影（来自列表匹配）。 */
  conversation: ConversationItem | null;
  messages: ConversationMessage[];
  /** 会话列表加载失败（Problem 态，整页 ProblemState）。 */
  problem: Problem | null;
  error: string | null;
  /** 线程加载失败（局部 Problem 态，仅右栏）。 */
  threadProblem: Problem | null;
  threadError: string | null;
  /** 发送表单幂等键（SSR 生成，hydration 稳定）。 */
  clientRequestId: string;
}

export interface MessagesActionData {
  ok?: boolean;
  message?: string;
}

/** client.ts 抛出的 Problem（或 ensureCsrf 的普通 Error）统一转状态码。 */
function failStatus(problem: Problem | null): number {
  const status = problem?.status;
  return typeof status === 'number' && status >= 400 && status <= 599 ? status : 503;
}

export const load: PageServerLoad = async ({ fetch, url }) => {
  const clientRequestId = newClientRequestId();
  let conversations: ConversationItem[] = [];
  let problem: Problem | null = null;
  try {
    const page = await listConversations(fetch);
    conversations = page.items ?? [];
  } catch (e) {
    const p = e as Problem;
    if (p?.status === 401) throw redirect(303, '/login');
    problem = p && typeof p === 'object' ? p : null;
  }

  const conversationId = url.searchParams.get('c');
  let conversation: ConversationItem | null = null;
  let messages: ConversationMessage[] = [];
  let threadProblem: Problem | null = null;
  if (conversationId) {
    conversation = conversations.find((c) => c.id === conversationId) ?? null;
    try {
      const page = await listMessages(fetch, conversationId);
      messages = page.items ?? [];
      // 进入会话即标记已读（action read；best-effort，失败静默重试下次）。
      try {
        await readConversation(fetch, conversationId);
      } catch {
        /* 未读角标由下次进入修正，不阻断线程展示 */
      }
    } catch (e) {
      const p = e as Problem;
      if (p?.status === 401) throw redirect(303, '/login');
      threadProblem = p && typeof p === 'object' ? p : null;
    }
  }

  return {
    conversations,
    conversationId,
    conversation,
    messages,
    problem,
    error: problem ? problemMessage(problem) : null,
    threadProblem,
    threadError: threadProblem ? problemMessage(threadProblem) : null,
    clientRequestId
  } satisfies MessagesPageData;
};

export const actions: Actions = {
  send: async ({ request, fetch }) => {
    const form = await request.formData();
    const conversationId = String(form.get('conversation_id') ?? '').trim();
    const body = String(form.get('body') ?? '').trim();
    const clientRequestId = String(form.get('client_request_id') ?? '').trim();
    if (!conversationId) {
      return fail(422, { message: '缺少会话标识，请刷新后重试' } satisfies MessagesActionData);
    }
    if (body.length < 1 || body.length > 2000) {
      return fail(422, { message: '私信内容需 1-2000 字' } satisfies MessagesActionData);
    }
    if (clientRequestId.length < 16) {
      return fail(422, { message: '请求标识缺失，请刷新页面后重试' } satisfies MessagesActionData);
    }
    try {
      await sendMessage(fetch, conversationId, body, clientRequestId);
      return { ok: true, message: '已发送' } satisfies MessagesActionData;
    } catch (e) {
      const p = e as Problem;
      if (p?.status === 401) {
        return fail(401, { message: '登录已过期，请重新登录' } satisfies MessagesActionData);
      }
      return fail(failStatus(p), { message: problemMessage(p) } satisfies MessagesActionData);
    }
  }
};
