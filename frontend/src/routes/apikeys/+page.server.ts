// GAP-FIX（社交域·API Key，apikeys.rs）：/apikeys——密钥管理。
//
// - load：未登录 → /login；listApiKeys（表格：名称/prefix/scopes/创建
//   时间/状态）；
// - create action：POST createApiKey（name 1-64 + scopes 白名单子集 +
//   client_request_id 幂等）→ 返回一次性明文 key（仅此一次，页面突出
//   展示 + 复制按钮；secret_hash 绝不回传）；
// - revoke action：DELETE revokeApiKey（软删除，立即失效）；
// - 失败态：返回 Problem（ProblemState 渲染），不抛 500。
import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { createApiKey, listApiKeys, newClientRequestId, revokeApiKey } from '$lib/api/client';
import type { ApiKeyItem } from '$lib/api/types';
import { problemMessage, type Problem } from '$lib/errors';

/** 与后端 apikeys.rs 白名单一致（超出即 422）。注意：SvelteKit 禁止
 *  +page.server.ts 导出 load/actions 之外的具名常量，故保持模块私有。 */
const API_KEY_SCOPES = ['posts:read', 'drafts:write', 'notifications:read', 'me:read'] as const;

export interface ApiKeysPageData {
  keys: ApiKeyItem[];
  problem: Problem | null;
  error: string | null;
  /** 创建表单幂等键（SSR 生成，hydration 稳定）。 */
  clientRequestId: string;
}

export interface ApiKeyCreatedView {
  id: string;
  name: string;
  prefix: string;
  scopes: string[];
  /** 一次性明文密钥（仅 create 成功返回这一次）。 */
  key: string;
}

export interface ApiKeysActionData {
  ok?: boolean;
  message?: string;
  /** create 成功的一次性密钥（展示后不再出现）。 */
  created?: ApiKeyCreatedView | null;
}

function asProblem(e: unknown): Problem | null {
  return e && typeof e === 'object' ? (e as Problem) : null;
}

function failStatus(problem: Problem | null): number {
  const status = problem?.status;
  return typeof status === 'number' && status >= 400 && status <= 599 ? status : 503;
}

export const load: PageServerLoad = async ({ fetch }) => {
  const clientRequestId = newClientRequestId();
  try {
    const page = await listApiKeys(fetch);
    return {
      keys: page.items ?? [],
      problem: null,
      error: null,
      clientRequestId
    } satisfies ApiKeysPageData;
  } catch (e) {
    const p = asProblem(e);
    if (p?.status === 401) throw redirect(303, '/login');
    return {
      keys: [],
      problem: p,
      error: problemMessage(p),
      clientRequestId
    } satisfies ApiKeysPageData;
  }
};

export const actions: Actions = {
  create: async ({ request, fetch }) => {
    const form = await request.formData();
    const name = String(form.get('name') ?? '').trim();
    const clientRequestId = String(form.get('client_request_id') ?? '').trim();
    const scopes = form
      .getAll('scopes')
      .map((s) => String(s))
      .filter((s) => (API_KEY_SCOPES as readonly string[]).includes(s));
    const nameChars = [...name].length;
    if (nameChars < 1 || nameChars > 64) {
      return fail(422, { message: '密钥名称需 1-64 字符', created: null } satisfies ApiKeysActionData);
    }
    if (clientRequestId.length < 16) {
      return fail(422, {
        message: '请求标识缺失，请刷新页面后重试',
        created: null
      } satisfies ApiKeysActionData);
    }
    try {
      const created = await createApiKey(fetch, name, scopes, clientRequestId);
      return {
        ok: true,
        message: '密钥已创建',
        created: {
          id: created.id,
          name: created.name,
          prefix: created.prefix,
          scopes: created.scopes ?? scopes,
          key: created.key
        }
      } satisfies ApiKeysActionData;
    } catch (e) {
      const p = asProblem(e);
      if (p?.status === 401) {
        return fail(401, { message: '登录已过期，请重新登录', created: null } satisfies ApiKeysActionData);
      }
      return fail(failStatus(p), {
        message: problemMessage(p),
        created: null
      } satisfies ApiKeysActionData);
    }
  },
  revoke: async ({ request, fetch }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    if (!id) {
      return fail(422, { message: '缺少密钥标识，请刷新后重试' } satisfies ApiKeysActionData);
    }
    try {
      await revokeApiKey(fetch, id);
      return { ok: true, message: '密钥已撤销，立即失效' } satisfies ApiKeysActionData;
    } catch (e) {
      const p = asProblem(e);
      if (p?.status === 401) {
        return fail(401, { message: '登录已过期，请重新登录' } satisfies ApiKeysActionData);
      }
      return fail(failStatus(p), { message: problemMessage(p) } satisfies ApiKeysActionData);
    }
  }
};
