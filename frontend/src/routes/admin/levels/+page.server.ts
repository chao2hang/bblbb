// 2026-09 等级合并单轨（原 M13-UI-04 + GAP-FIX + M20-TRUST 演进）：
// /admin/levels（等级管理）= LinuxDo 信任等级（TL0–TL4）唯一管理入口——
// 1) 每级规则 + 用户数（GET /api/v1/admin/trust-levels，level.manage）；
// 2) 手动授予（POST /api/v1/admin/users/{user_id}/trust-level：body
//    {level, reason}，TL4 唯一授予通道，写审计 admin.trust_level.set）；
// 3) 附件空间配额（M06-QUOTA）：档位键 = users.trust_level（TL0–4），
//    读取 GET /admin/levels/{level}/attachment-quota，编辑 PATCH：If-Match =
//    policy_version + reason 审计 + step-up 重新验证。
// 原 /admin/trust-levels 独立路由并入本页（导航单入口）。
import { fail, isRedirect, redirect, type Cookies } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPatch, authedPost, getAuthed } from '$lib/api/server';

/** 每级信任规则行（GET /admin/trust-levels 投影）。 */
export interface AdminTrustLevelItem {
  level: number;
  name: string;
  summary: string | null;
  /** requirements_json 解析结果（键值阈值；结构见 docs/TRUST-LEVELS.md §4）。 */
  requirements: Record<string, unknown> | null;
  is_enabled: boolean;
  version: number;
  user_count: number;
}

/** 等级附件配额策略（quota_policy_revisions 最新修订，M06-QUOTA）。 */
export interface LevelQuotaPolicy {
  level: number;
  single_file_max_bytes: number;
  total_bytes: number;
  daily_upload_bytes: number;
  retention_days: number;
  policy_version: number;
}

/** GET /api/v1/admin/levels/{id}/attachment-quota 响应投影。 */
export interface LevelQuotaView {
  level: number;
  policy: LevelQuotaPolicy | null;
}

export type AdminLevelsState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminLevelsPageData {
  state: AdminLevelsState;
  /** 信任等级规则行（TL0–TL4，按 level 升序；空 = 后端无规则行）。 */
  items: AdminTrustLevelItem[];
  /** 各等级附件配额（key 为等级数字字符串；无策略的等级缺省）。 */
  quotas: Record<string, LevelQuotaPolicy>;
  error: string | null;
}

/** setLevel/updateQuota/reauth action 返回投影（SvelteKit Actions 联合类型）。 */
export interface AdminLevelsActionData {
  /** 结果提示（成功/校验失败/版本冲突）。 */
  message?: string | null;
  /** message 类型：success（保存/重认证成功）或 error（fail 分支）。 */
  messageKind?: 'success' | 'error';
  /** 403 step_up_required → 页面展示重新验证（reauth）表单。 */
  stepUpRequired?: boolean;
  requestId?: string | null;
}

/** 配额档位清单：信任规则等级；无规则时回退 TL0–TL4 全档。 */
function quotaTargets(items: AdminTrustLevelItem[]): number[] {
  const fromItems = Array.from(
    new Set(items.map((l) => Number(l.level)).filter((n) => Number.isInteger(n) && n >= 0))
  ).sort((a, b) => a - b);
  return fromItems.length > 0 ? fromItems : [0, 1, 2, 3, 4];
}

/** 并发读取各等级附件配额（单个等级失败不阻塞其余等级；401 统一重定向登录）。 */
async function loadLevelQuotas(
  cookies: Cookies,
  levels: number[],
  requestId: string | null
): Promise<{ quotas: Record<string, LevelQuotaPolicy>; firstError: string | null }> {
  const quotas: Record<string, LevelQuotaPolicy> = {};
  let firstError: string | null = null;
  const results = await Promise.all(
    levels.map(async (level) => {
      const result = await getAuthed<LevelQuotaView>(
        cookies,
        `/api/v1/admin/levels/${level}/attachment-quota`,
        requestId
      );
      return { level, result };
    })
  );
  for (const { level, result } of results) {
    if (result.ok && result.data?.policy) {
      quotas[String(level)] = result.data.policy;
    } else if (!result.ok) {
      if (result.status === 401) throw redirect(303, '/login');
      if (result.status !== 404 && !firstError) firstError = result.message;
    }
  }
  return { quotas, firstError };
}

export const load: PageServerLoad = async ({ cookies, request }): Promise<AdminLevelsPageData> => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<{ items: AdminTrustLevelItem[] }>(
    cookies,
    '/api/v1/admin/trust-levels',
    requestId
  );
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) {
      return { state: 'forbidden', items: [], quotas: {}, error: result.message };
    }
    if (result.status === 501) {
      return { state: 'not_implemented', items: [], quotas: {}, error: result.message };
    }
    return { state: 'error', items: [], quotas: {}, error: result.message };
  }

  const items = Array.isArray(result.data.items) ? result.data.items : [];
  const { quotas, firstError } = await loadLevelQuotas(cookies, quotaTargets(items), requestId);

  return {
    state: firstError ? 'error' : 'ok',
    items,
    quotas,
    error: firstError
  };
};

export const actions: Actions = {
  /** 编辑单级规则（PATCH /admin/trust-levels/{level}；If-Match=version + reason 审计）。 */
  updateRule: async ({ request, cookies }) => {
    const form = await request.formData();
    const level = Number(form.get('level') ?? -1);
    const version = Number(form.get('version') ?? 0);
    const name = String(form.get('name') ?? '').trim();
    const summaryRaw = String(form.get('summary') ?? '').trim();
    const isEnabled = form.get('is_enabled') === '1';
    const reason = String(form.get('reason') ?? '').trim();
    const requirementsRaw = String(form.get('requirements_json') ?? '').trim();

    if (!Number.isInteger(level) || level < 0 || level > 4) {
      return fail(422, { message: '等级标识无效', messageKind: 'error' } satisfies AdminLevelsActionData);
    }
    if (!Number.isInteger(version) || version < 1) {
      return fail(409, { message: '规则版本缺失或无效，请刷新后重试', messageKind: 'error' } satisfies AdminLevelsActionData);
    }
    if (!name || name.length > 50) {
      return fail(422, { message: '名称必填且不超过 50 字', messageKind: 'error' } satisfies AdminLevelsActionData);
    }
    if (summaryRaw.length > 200) {
      return fail(422, { message: '摘要不超过 200 字', messageKind: 'error' } satisfies AdminLevelsActionData);
    }
    if (!reason) {
      return fail(422, { message: '操作原因必填（写审计）', messageKind: 'error' } satisfies AdminLevelsActionData);
    }
    let requirements: Record<string, unknown>;
    try {
      const parsed: unknown = JSON.parse(requirementsRaw || '{}');
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) throw new Error('not an object');
      requirements = parsed as Record<string, unknown>;
    } catch {
      return fail(422, { message: '阈值条件数据无效', messageKind: 'error' } satisfies AdminLevelsActionData);
    }

    try {
      const result = await authedPatch<{ item: AdminTrustLevelItem }>(
        cookies,
        `/api/v1/admin/trust-levels/${level}`,
        {
          name,
          summary: summaryRaw === '' ? null : summaryRaw,
          is_enabled: isEnabled,
          requirements,
          reason
        },
        { 'If-Match': String(version) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return {
          message: `TL${level} 规则已更新（v${result.data?.item?.version ?? version + 1}，立即生效）`,
          messageKind: 'success'
        } satisfies AdminLevelsActionData;
      }
      if (result.status === 401) throw redirect(303, '/login');
      if (result.status === 409) {
        return fail(409, {
          message: `版本冲突：${result.message}，请刷新页面后重试`,
          messageKind: 'error',
          requestId: result.requestId
        } satisfies AdminLevelsActionData);
      }
      return fail(result.status, {
        message: result.message || '规则保存失败，请检查参数',
        messageKind: 'error',
        requestId: result.requestId
      } satisfies AdminLevelsActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '保存失败，请稍后重试', messageKind: 'error' } satisfies AdminLevelsActionData);
    }
  },

  /** 恢复单级规则为内置 LinuxDo 默认（POST /admin/trust-levels/{level}/reset）。 */
  resetRule: async ({ request, cookies }) => {
    const form = await request.formData();
    const level = Number(form.get('level') ?? -1);
    const reason = String(form.get('reason') ?? '').trim();
    if (!Number.isInteger(level) || level < 0 || level > 4) {
      return fail(422, { message: '等级标识无效', messageKind: 'error' } satisfies AdminLevelsActionData);
    }
    if (!reason) {
      return fail(422, { message: '操作原因必填（写审计）', messageKind: 'error' } satisfies AdminLevelsActionData);
    }
    try {
      const result = await authedPost<{ item: AdminTrustLevelItem }>(
        cookies,
        `/api/v1/admin/trust-levels/${level}/reset`,
        { reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return {
          message: `TL${level} 已恢复为默认规则`,
          messageKind: 'success'
        } satisfies AdminLevelsActionData;
      }
      if (result.status === 401) throw redirect(303, '/login');
      return fail(result.status, {
        message: result.message || '恢复默认失败',
        messageKind: 'error',
        requestId: result.requestId
      } satisfies AdminLevelsActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '恢复默认失败，请稍后重试', messageKind: 'error' } satisfies AdminLevelsActionData);
    }
  },

  /** 手动设置信任等级（0–4；TL4 唯一授予入口；原因写审计 admin.trust_level.set）。 */
  setLevel: async ({ request, cookies }) => {
    const form = await request.formData();
    const userId = String(form.get('user_id') ?? '').trim();
    const level = Number(form.get('level') ?? -1);
    const reason = String(form.get('reason') ?? '').trim();
    if (!userId) return fail(422, { message: '请填写目标用户的 user_id', messageKind: 'error' } satisfies AdminLevelsActionData);
    if (!Number.isInteger(level) || level < 0 || level > 4) {
      return fail(422, { message: '信任等级必须为 0–4', messageKind: 'error' } satisfies AdminLevelsActionData);
    }
    if (!reason) {
      return fail(422, { message: '操作原因必填（写审计）', messageKind: 'error' } satisfies AdminLevelsActionData);
    }
    try {
      const result = await authedPost<{ from_level: number; to_level: number; changed: boolean }>(
        cookies,
        `/api/v1/admin/users/${encodeURIComponent(userId)}/trust-level`,
        { level, reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        const suffix = result.data.changed === false ? '（等级未变化）' : '';
        return {
          message: `信任等级已设置为 TL${result.data.to_level}${suffix}`,
          messageKind: 'success'
        } satisfies AdminLevelsActionData;
      }
      if (result.status === 401) throw redirect(303, '/login');
      if (result.status === 404) {
        return fail(404, {
          message: '目标用户不存在（user_id 需为用户管理页中的 UUID）',
          messageKind: 'error',
          requestId: result.requestId
        } satisfies AdminLevelsActionData);
      }
      return fail(result.status, {
        message: result.message,
        messageKind: 'error',
        requestId: result.requestId
      } satisfies AdminLevelsActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '保存失败，请稍后重试', messageKind: 'error' } satisfies AdminLevelsActionData);
    }
  },

  /**
   * 批量设置附件配额（约定 B）：对多档执行同一配额。服务端循环单条
   * PATCH（每档各自 If-Match=该档当前 policy_version + reason 审计 + step-up）；
   * 单档冲突不阻塞其余档，结果按档汇总反馈。
   */
  batchQuota: async ({ request, cookies }) => {
    const form = await request.formData();
    const reason = String(form.get('reason') ?? '').trim();
    if (!reason) {
      return fail(422, { message: '操作原因必填（写入审计日志）', messageKind: 'error' } satisfies AdminLevelsActionData);
    }
    let targets: number[] = [];
    let versions: Record<string, number> = {};
    try {
      const t: unknown = JSON.parse(String(form.get('targets_json') ?? '[]'));
      const v: unknown = JSON.parse(String(form.get('versions_json') ?? '{}'));
      if (!Array.isArray(t) || !t.every((n) => Number.isInteger(n) && n >= 0 && n <= 4)) throw new Error('targets');
      if (!v || typeof v !== 'object' || Array.isArray(v)) throw new Error('versions');
      targets = t as number[];
      versions = v as Record<string, number>;
    } catch {
      return fail(422, { message: '批量目标或版本数据无效，请刷新后重试', messageKind: 'error' } satisfies AdminLevelsActionData);
    }
    if (targets.length === 0) {
      return fail(422, { message: '未选择任何等级档位', messageKind: 'error' } satisfies AdminLevelsActionData);
    }

    const mbToBytes = (key: string): number | null => {
      const raw = String(form.get(key) ?? '').trim();
      if (raw === '') return null;
      const mb = Number(raw);
      if (!Number.isFinite(mb) || mb <= 0) return Number.NaN;
      return Math.round(mb * 1048576);
    };
    const singleFile = mbToBytes('single_file_mb');
    const totalBytes = mbToBytes('total_mb');
    const dailyBytes = mbToBytes('daily_mb');
    if (singleFile === Number.NaN || totalBytes === Number.NaN || dailyBytes === Number.NaN) {
      return fail(422, { message: '空间大小需为正数（单位 MB）', messageKind: 'error' } satisfies AdminLevelsActionData);
    }
    if (singleFile === null || totalBytes === null || dailyBytes === null) {
      return fail(422, { message: '单文件上限、总容量与每日上传量均必填', messageKind: 'error' } satisfies AdminLevelsActionData);
    }
    const retentionDays = Number(String(form.get('retention_days') ?? '').trim());
    if (!Number.isInteger(retentionDays) || retentionDays < 0 || retentionDays > 3650) {
      return fail(422, { message: '保留期需为 0..=3650 的整数（天）', messageKind: 'error' } satisfies AdminLevelsActionData);
    }

    const ok: number[] = [];
    const conflicts: number[] = [];
    let firstError: string | null = null;
    try {
      for (const level of targets) {
        const policyVersion = versions[String(level)];
        if (!Number.isInteger(policyVersion) || (policyVersion ?? 0) < 1) {
          conflicts.push(level);
          continue;
        }
        const result = await authedPatch<{ level: number; policy: LevelQuotaPolicy }>(
          cookies,
          `/api/v1/admin/levels/${level}/attachment-quota`,
          {
            single_file_max_bytes: singleFile,
            total_bytes: totalBytes,
            daily_upload_bytes: dailyBytes,
            retention_days: retentionDays,
            reason
          },
          { 'If-Match': String(policyVersion) },
          request.headers.get('x-request-id')
        );
        if (result.ok) {
          ok.push(level);
        } else if (result.status === 401) {
          throw redirect(303, '/login');
        } else if (result.code === 'step_up_required') {
          return fail(403, {
            message: '此操作需要重新验证身份（登录已超过有效期），请输入密码重新验证后重试',
            messageKind: 'error',
            stepUpRequired: true,
            requestId: result.requestId
          } satisfies AdminLevelsActionData);
        } else if (result.status === 409) {
          conflicts.push(level);
        } else if (!firstError) {
          firstError = result.message || null;
        }
      }
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '批量保存失败，请稍后重试', messageKind: 'error' } satisfies AdminLevelsActionData);
    }

    if (conflicts.length === 0 && ok.length === targets.length) {
      return {
        message: `已批量更新 TL${targets.join('、TL')} 附件配额（每档独立 policy_version，仅影响新上传）`,
        messageKind: 'success'
      } satisfies AdminLevelsActionData;
    }
    const parts: string[] = [];
    if (ok.length > 0) parts.push(`成功 TL${ok.join('、TL')}`);
    if (conflicts.length > 0) parts.push(`版本冲突 TL${conflicts.join('、TL')}（请刷新后单独重试）`);
    if (firstError) parts.push(firstError);
    return fail(207, {
      message: `批量完成：${parts.join('；')}`,
      messageKind: 'error'
    } satisfies AdminLevelsActionData);
  },

  /**
   * 附件空间配额：PATCH /admin/levels/{level}/attachment-quota（M06-QUOTA-02）。
   * 表单以 MB 填写（单文件上限/总容量/每日上传），服务端换算为字节提交；
   * If-Match 为当前 policy_version（乐观锁），保留期单位为天。
   */
  updateQuota: async ({ request, cookies }) => {
    const form = await request.formData();
    const level = Number(form.get('level') ?? 0);
    const policyVersion = Number(form.get('policy_version') ?? 0);
    const reason = String(form.get('reason') ?? '').trim();

    if (!Number.isInteger(level) || level < 0) {
      return fail(422, { message: '缺少等级标识', messageKind: 'error' } satisfies AdminLevelsActionData);
    }
    if (!Number.isInteger(policyVersion) || policyVersion < 1) {
      return fail(409, { message: '配额版本缺失或无效，请展开刷新后重试', messageKind: 'error' } satisfies AdminLevelsActionData);
    }
    if (!reason) {
      return fail(422, { message: '操作原因必填（写入审计日志）', messageKind: 'error' } satisfies AdminLevelsActionData);
    }

    // MB → 字节（必须为正数；后端会再校验 单文件 ≤ 总容量 ≤ 站点硬上限）。
    const mbToBytes = (key: string): number | null => {
      const raw = String(form.get(key) ?? '').trim();
      if (raw === '') return null;
      const mb = Number(raw);
      if (!Number.isFinite(mb) || mb <= 0) return Number.NaN;
      return Math.round(mb * 1048576);
    };
    const singleFile = mbToBytes('single_file_mb');
    const totalBytes = mbToBytes('total_mb');
    const dailyBytes = mbToBytes('daily_mb');
    if (singleFile === Number.NaN || totalBytes === Number.NaN || dailyBytes === Number.NaN) {
      return fail(422, { message: '空间大小需为正数（单位 MB）', messageKind: 'error' } satisfies AdminLevelsActionData);
    }
    if (singleFile === null || totalBytes === null || dailyBytes === null) {
      return fail(422, { message: '单文件上限、总容量与每日上传量均必填', messageKind: 'error' } satisfies AdminLevelsActionData);
    }
    const retentionRaw = String(form.get('retention_days') ?? '').trim();
    const retentionDays = Number(retentionRaw);
    if (!Number.isInteger(retentionDays) || retentionDays < 0 || retentionDays > 3650) {
      return fail(422, { message: '保留期需为 0..=3650 的整数（天）', messageKind: 'error' } satisfies AdminLevelsActionData);
    }

    try {
      const result = await authedPatch<{ level: number; policy: LevelQuotaPolicy }>(
        cookies,
        `/api/v1/admin/levels/${level}/attachment-quota`,
        {
          single_file_max_bytes: singleFile,
          total_bytes: totalBytes,
          daily_upload_bytes: dailyBytes,
          retention_days: retentionDays,
          reason
        },
        { 'If-Match': String(policyVersion) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        const policy = result.data?.policy;
        return {
          message: `TL${level} 附件配额已更新（新版本 v${policy?.policy_version ?? policyVersion + 1}，仅影响新上传）`,
          messageKind: 'success'
        } satisfies AdminLevelsActionData;
      }
      if (result.status === 401) throw redirect(303, '/login');
      if (result.code === 'step_up_required') {
        return fail(403, {
          message: '此操作需要重新验证身份（登录已超过有效期），请输入密码重新验证后重试',
          messageKind: 'error',
          stepUpRequired: true,
          requestId: result.requestId
        } satisfies AdminLevelsActionData);
      }
      if (result.status === 409) {
        return fail(409, {
          message: `版本冲突：${result.message}，请刷新页面后重试`,
          messageKind: 'error',
          requestId: result.requestId
        } satisfies AdminLevelsActionData);
      }
      return fail(result.status, {
        message: result.message || '配额保存失败，请检查参数',
        messageKind: 'error',
        requestId: result.requestId
      } satisfies AdminLevelsActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '保存失败，请稍后重试', messageKind: 'error' } satisfies AdminLevelsActionData);
    }
  },

  /** step-up 重新验证（M02-MFA-07）：配额修改命中 403 step_up_required 时展示。 */
  reauth: async ({ request, cookies }) => {
    const form = await request.formData();
    const password = String(form.get('password') ?? '');
    if (!password) {
      return fail(422, { message: '请输入当前密码', messageKind: 'error' } satisfies AdminLevelsActionData);
    }
    try {
      const result = await authedPost(
        cookies,
        '/api/v1/auth/re-auth',
        { password },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: '已重新验证身份，请重试保存配额', messageKind: 'success' } satisfies AdminLevelsActionData;
      }
      return fail(result.status, {
        message: result.message,
        messageKind: 'error',
        requestId: result.requestId
      } satisfies AdminLevelsActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '重新验证服务暂不可用，请稍后重试', messageKind: 'error' } satisfies AdminLevelsActionData);
    }
  }
};
