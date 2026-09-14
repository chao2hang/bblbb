// M06-UI-06/07：管理端存储配置——local/S3 配置、path-style、TTL、测试连接与
// 脱敏状态（Secret 只返回 secret_configured；env 来源字段只读）。
//
// - load：GET /admin/storage/config（脱敏视图）。
// - save action：PATCH /admin/storage/config（If-Match 版本 + reason；空
//   Secret 输入表示保持原值）。当前部署由环境变量管理（M06-ADAPTER-03），
//   保存语义 = 校验 + 审计意图，生效需改 BBLBB__* 环境变量并重启。
// - test action：POST /admin/storage/test（测试候选/当前配置，脱敏诊断）。
// - reauth action：POST /api/v1/auth/re-auth（step-up 窗口过期后重新验证，
//   M02-MFA-07；save/test 命中 403 step_up_required 时展示）。
// - M06-UI-07：TTL 修改只影响新签发 URL；后端切换需预演/hash/回滚——界面
//   明确提示，不提供“一键切换”。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPatch, authedPost, getAuthed } from '$lib/api/server';
import type { StorageConfig, StorageTestResult } from '$lib/api/types';

export interface AdminStoragePageData {
  config: StorageConfig | null;
  loadError: string | null;
}

/** form action 返回投影。 */
export interface AdminStorageActionData {
  message?: string;
  /** message 类型：success（保存/重认证成功）或 error（fail 分支）。 */
  messageKind?: 'success' | 'error';
  /** 403 step_up_required → 页面展示重新验证（reauth）表单。 */
  stepUpRequired?: boolean;
  requestId?: string | null;
  testResult?: StorageTestResult | null;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<StorageConfig>(cookies, '/api/v1/admin/storage/config', requestId);
  if (!result.ok && result.status === 401) throw redirect(303, '/login');
  if (!result.ok) {
    return { config: null, loadError: result.message } satisfies AdminStoragePageData;
  }
  return { config: result.data, loadError: null } satisfies AdminStoragePageData;
};

function boolForm(form: FormData, key: string): boolean | undefined {
  const raw = form.get(key);
  if (raw === null) return undefined;
  return raw === 'on' || raw === 'true' || raw === '1';
}

function numOrNull(raw: unknown): number | null | undefined {
  if (raw === null || raw === undefined) return undefined;
  const s = String(raw).trim();
  if (s === '') return undefined;
  const v = Number(s);
  return Number.isFinite(v) ? v : null;
}

function buildPatch(form: FormData, current: StorageConfig | null): Record<string, unknown> {
  const patch: Record<string, unknown> = {};
  const set = (key: string, value: unknown, managed = false) => {
    if (managed && current?.managed_fields?.includes(key)) return; // env 只读
    if (value !== undefined) patch[key] = value;
  };
  set('backend', form.get('backend') ? String(form.get('backend')) : undefined);
  set('local_path', String(form.get('local_path') ?? '').trim() || null);
  set('s3_endpoint', String(form.get('s3_endpoint') ?? '').trim() || null);
  set('s3_region', String(form.get('s3_region') ?? '').trim() || 'us-east-1');
  set('s3_bucket', String(form.get('s3_bucket') ?? '').trim() || null);
  set('s3_path_style', boolForm(form, 's3_path_style'));
  set('s3_presigned_uploads', boolForm(form, 's3_presigned_uploads'));
  set('s3_public_base_url', String(form.get('s3_public_base_url') ?? '').trim() || null);
  set('signed_url_ttl_seconds', numOrNull(form.get('signed_url_ttl_seconds')));
  const maxSizeMb = numOrNull(form.get('max_size_mb'));
  const uploadMaxBytes = numOrNull(form.get('upload_max_bytes')) ?? (maxSizeMb ? Math.round(maxSizeMb * 1048576) : undefined);
  set('upload_max_bytes', uploadMaxBytes);
  const accessKey = String(form.get('s3_access_key_id') ?? '').trim();
  if (accessKey) set('s3_access_key_id', accessKey, true);
  const secret = String(form.get('s3_secret_access_key') ?? '').trim();
  if (secret) set('s3_secret_access_key', secret, true);
  // 站点上传类型类目开关（image/pdf/text/office/av；全不勾 = 仅保底，后端拒绝空集）。
  const categories = form.getAll('allowed_upload_types').map((v) => String(v).trim()).filter(Boolean);
  if (form.has('allowed_upload_types_submitted')) {
    set('allowed_upload_types', categories);
  }
  return patch;
}

export const actions: Actions = {
  save: async ({ request, cookies }) => {
    const form = await request.formData();
    const reason = String(form.get('reason') ?? '').trim();
    const expectedVersion = Number(form.get('expected_version') ?? 0);
    if (!reason) {
      return fail(422, {
        message: '操作原因必填',
        messageKind: 'error'
      } satisfies AdminStorageActionData);
    }
    if (!Number.isInteger(expectedVersion) || expectedVersion < 1) {
      return fail(422, {
        message: '配置版本缺失或无效，请刷新后重试',
        messageKind: 'error'
      } satisfies AdminStorageActionData);
    }
    // 用上次 load 的 config 判断 managed 字段；action 无法重取 load 数据，
    // 由页面把 managed_fields 一起提交（仅用于跳过只读字段）。
    const managed = String(form.get('managed_fields') ?? '')
      .split(',')
      .filter(Boolean);
    const current: StorageConfig | null = managed.length > 0 ? ({ managed_fields: managed } as StorageConfig) : null;
    const patch = buildPatch(form, current);
    if (Object.keys(patch).length === 0) {
      return fail(422, {
        message: '没有需要保存的变更',
        messageKind: 'error'
      } satisfies AdminStorageActionData);
    }
    try {
      const result = await authedPatch<StorageConfig>(
        cookies,
        '/api/v1/admin/storage/config',
        { ...patch, expected_version: expectedVersion, reason },
        { 'If-Match': String(expectedVersion) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        const message =
          result.data.managed_by === 'database'
            ? '存储配置已保存并立即热生效（已更新数据库与服务实例，无需重启进程）。'
            : '存储配置已保存（只影响新上传/新签发 URL）';
        return { message, messageKind: 'success' } satisfies AdminStorageActionData;
      }
      if (result.code === 'step_up_required') {
        return fail(403, {
          message: '此操作需要重新验证身份（登录已超过有效期），请输入密码重新验证后重试',
          messageKind: 'error',
          stepUpRequired: true,
          requestId: result.requestId
        } satisfies AdminStorageActionData);
      }
      if (result.status === 409) {
        return fail(409, {
          message: `版本冲突或字段由部署配置管理：${result.message}`,
          messageKind: 'error',
          requestId: result.requestId
        } satisfies AdminStorageActionData);
      }
      const detailedMsg = result.message || '保存失败，请检查请求参数';
      return fail(result.status, {
        message: detailedMsg,
        messageKind: 'error',
        requestId: result.requestId
      } satisfies AdminStorageActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, {
        message: '保存失败，请稍后重试',
        messageKind: 'error'
      } satisfies AdminStorageActionData);
    }
  },
  test: async ({ request, cookies }) => {
    const form = await request.formData();
    const candidate: Record<string, unknown> = {
      backend: String(form.get('backend') ?? ''),
      local_path: String(form.get('local_path') ?? '').trim() || null,
      s3_endpoint: String(form.get('s3_endpoint') ?? '').trim() || null,
      s3_region: String(form.get('s3_region') ?? '').trim() || null,
      s3_bucket: String(form.get('s3_bucket') ?? '').trim() || null,
      s3_path_style: boolForm(form, 's3_path_style') ?? false,
      s3_public_base_url: String(form.get('s3_public_base_url') ?? '').trim() || null,
      signed_url_ttl_seconds: numOrNull(form.get('signed_url_ttl_seconds')),
      reason: String(form.get('reason') ?? '').trim() || '测试存储连接'
    };
    const accessKey = String(form.get('s3_access_key_id') ?? '').trim();
    if (accessKey) candidate.s3_access_key_id = accessKey;
    const secret = String(form.get('s3_secret_access_key') ?? '').trim();
    if (secret) candidate.s3_secret_access_key = secret;
    try {
      const result = await authedPost<StorageTestResult>(
        cookies,
        '/api/v1/admin/storage/test',
        candidate,
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { testResult: result.data, messageKind: 'success' } satisfies AdminStorageActionData;
      }
      if (result.code === 'step_up_required') {
        return fail(403, {
          message: '此操作需要重新验证身份（登录已超过有效期），请输入密码重新验证后重试',
          messageKind: 'error',
          stepUpRequired: true,
          requestId: result.requestId,
          testResult: null
        } satisfies AdminStorageActionData);
      }
      return fail(result.status, {
        message: result.message,
        messageKind: 'error',
        requestId: result.requestId,
        testResult: null
      } satisfies AdminStorageActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, {
        message: '测试连接失败，请稍后重试',
        messageKind: 'error',
        testResult: null
      } satisfies AdminStorageActionData);
    }
  },
  reauth: async ({ request, cookies }) => {
    const form = await request.formData();
    const password = String(form.get('password') ?? '');
    if (!password) {
      return fail(422, {
        message: '请输入当前密码',
        messageKind: 'error'
      } satisfies AdminStorageActionData);
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
          message: '已重新验证身份，请重试保存或测试连接',
          messageKind: 'success'
        } satisfies AdminStorageActionData;
      }
      return fail(result.status, {
        message: result.message,
        messageKind: 'error',
        requestId: result.requestId
      } satisfies AdminStorageActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, {
        message: '重新验证服务暂不可用，请稍后重试',
        messageKind: 'error'
      } satisfies AdminStorageActionData);
    }
  }
};
