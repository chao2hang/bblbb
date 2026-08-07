// M06-UI-06/07：管理端存储配置——local/S3 配置、path-style、TTL、测试连接与
// 脱敏状态（Secret 只返回 secret_configured；env 来源字段只读）。
//
// - load：GET /admin/storage/config（脱敏视图）。
// - save action：PATCH /admin/storage/config（If-Match 版本 + reason；空
//   Secret 输入表示保持原值）。
// - test action：POST /admin/storage/test（测试候选/当前配置，脱敏诊断）。
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
  set('s3_region', String(form.get('s3_region') ?? '').trim() || null);
  set('s3_bucket', String(form.get('s3_bucket') ?? '').trim() || null);
  set('s3_path_style', boolForm(form, 's3_path_style'));
  set('s3_presigned_uploads', boolForm(form, 's3_presigned_uploads'));
  set('signed_url_ttl_seconds', numOrNull(form.get('signed_url_ttl_seconds')));
  set('upload_max_bytes', numOrNull(form.get('upload_max_bytes')));
  const secret = String(form.get('s3_secret_access_key') ?? '').trim();
  if (secret) set('s3_secret_access_key', secret, true);
  return patch;
}

export const actions: Actions = {
  save: async ({ request, cookies }) => {
    const form = await request.formData();
    const reason = String(form.get('reason') ?? '').trim();
    const expectedVersion = Number(form.get('expected_version') ?? 0);
    if (!reason) {
      return fail(422, { message: '操作原因必填' } satisfies AdminStorageActionData);
    }
    if (!Number.isInteger(expectedVersion) || expectedVersion < 1) {
      return fail(422, { message: '配置版本缺失或无效，请刷新后重试' } satisfies AdminStorageActionData);
    }
    // 用上次 load 的 config 判断 managed 字段；action 无法重取 load 数据，
    // 由页面把 managed_fields 一起提交（仅用于跳过只读字段）。
    const managed = String(form.get('managed_fields') ?? '')
      .split(',')
      .filter(Boolean);
    const current: StorageConfig | null = managed.length > 0 ? ({ managed_fields: managed } as StorageConfig) : null;
    const patch = buildPatch(form, current);
    if (Object.keys(patch).length === 0) {
      return fail(422, { message: '没有需要保存的变更' } satisfies AdminStorageActionData);
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
        return { message: '存储配置已保存（只影响新上传/新签发 URL）' } satisfies AdminStorageActionData;
      }
      if (result.status === 409) {
        return fail(409, { message: `版本冲突或字段由部署配置管理：${result.message}` } satisfies AdminStorageActionData);
      }
      return fail(result.status, { message: result.message, requestId: result.requestId } satisfies AdminStorageActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '保存失败，请稍后重试' } satisfies AdminStorageActionData);
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
      signed_url_ttl_seconds: numOrNull(form.get('signed_url_ttl_seconds'))
    };
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
        return { testResult: result.data } satisfies AdminStorageActionData;
      }
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId,
        testResult: null
      } satisfies AdminStorageActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '测试连接失败，请稍后重试' } satisfies AdminStorageActionData);
    }
  }
};
