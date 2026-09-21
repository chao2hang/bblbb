// M18-ADMIN-POINTS-03（管理域·积分规则配置）：/admin/points/rules——
// 「什么操作获得多少」专门管理页，activity_rules 为单一事实来源：
// - load：GET /api/v1/admin/activity/tasks（全部规则，含隐藏 conditions）+
//   GET /api/v1/admin/activity/config（rewards_enabled 总闸只读展示；
//   签到全局开关/时区在 /admin/activity 管理，本页不重复编辑）；
// - create：POST /api/v1/admin/activity/tasks（kind/amount/currency_id/
//   daily_limit/cooldown_seconds/is_enabled，reason 必填写审计，
//   activity.manage 权限）；
// - update：PATCH /api/v1/admin/activity/tasks/{id}（字段子集 + If-Match
//   version 附头；服务端当前为 last-write-wins，409 保留透传）。
// 注意：后端写入口字段名是 currency_id（读侧投影叫 currency），两页一致传 currency_id。
import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPatch, authedPost, getAuthed } from '$lib/api/server';
import { adminListState, type AdminLoadState } from '$lib/admin';
import type { ActivityConfig, ActivityTask } from '$lib/api/types';

export interface AdminPointsRulesPageData {
  rules: AdminLoadState<ActivityTask>;
  /** 奖励总闸（rewards_enabled）只读状态；null = 配置接口不可用（降级展示）。 */
  masterSwitch: { enabled: boolean; version: number } | { unavailable: true; message: string } | null;
  error: string | null;
}

export interface AdminPointsRulesActionData {
  message?: string;
  requestId?: string | null;
}

/** 与后端 RULE_KINDS 一致（0050 CHECK 约束）；仅供本文件校验（SvelteKit 禁止 +page.server.ts 额外导出）。 */
const RULE_KINDS = ['check_in', 'task', 'reaction', 'post', 'comment', 'leaderboard'] as const;

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');

  const rulesResult = await getAuthed<{ items: ActivityTask[] }>(
    cookies,
    '/api/v1/admin/activity/tasks',
    requestId
  );
  if (!rulesResult.ok && rulesResult.status === 401) throw redirect(303, '/login');
  const rules = adminListState(rulesResult);

  // 奖励总闸只读展示；不可用（403/5xx）不阻断规则管理，降级提示。
  const configResult = await getAuthed<ActivityConfig>(
    cookies,
    '/api/v1/admin/activity/config',
    requestId
  );
  const masterSwitch = configResult.ok
    ? { enabled: configResult.data.rewards_enabled !== false, version: configResult.data.version ?? 0 }
    : { unavailable: true as const, message: configResult.message };

  return { rules, masterSwitch, error: null } satisfies AdminPointsRulesPageData;
};

/** 解析可空非负整数（空串 = 不带该字段）；返回判别式结果而非 ActionFailure。 */
function optionalInt(
  form: FormData,
  name: string
): { ok: true; value?: number } | { ok: false; message: string } {
  const raw = String(form.get(name) ?? '').trim();
  if (raw === '') return { ok: true };
  const n = Number(raw);
  if (!Number.isInteger(n) || n < 0) {
    return { ok: false, message: `${name} 须为非负整数（每日上限须 ≥ 1）` };
  }
  return { ok: true, value: n };
}

export const actions: Actions = {
  create: async ({ request, cookies }) => {
    const form = await request.formData();
    const kind = String(form.get('kind') ?? '').trim();
    const amountRaw = String(form.get('amount') ?? '').trim();
    const currency = String(form.get('currency') ?? 'coin').trim().toLowerCase();
    const reason = String(form.get('reason') ?? '').trim();
    const isEnabled = form.get('is_enabled') != null;

    if (!(RULE_KINDS as readonly string[]).includes(kind)) {
      return fail(422, { message: `无效规则类型：${kind}` });
    }
    const amount = Number(amountRaw === '' ? NaN : amountRaw);
    if (!Number.isInteger(amount) || amount < 0) {
      return fail(422, { message: '奖励数额须为非负整数' });
    }
    if (currency !== 'coin') {
      return fail(422, { message: '奖励币种仅支持 B币' });
    }
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' });

    const daily = optionalInt(form, 'daily_limit');
    if (!daily.ok) return fail(422, { message: daily.message });
    if (daily.value != null && daily.value < 1) {
      return fail(422, { message: '每日上限须 ≥ 1（留空 = 不限）' });
    }
    const cooldown = optionalInt(form, 'cooldown_seconds');
    if (!cooldown.ok) return fail(422, { message: cooldown.message });

    const body: Record<string, unknown> = {
      kind,
      amount,
      currency_id: currency,
      is_enabled: isEnabled,
      reason
    };
    if (daily.value != null) body.daily_limit = daily.value;
    if (cooldown.value != null) body.cooldown_seconds = cooldown.value;

    try {
      const result = await authedPost<unknown>(
        cookies,
        '/api/v1/admin/activity/tasks',
        body,
        request.headers.get('x-request-id')
      );
      if (result.ok) return { message: `规则已创建（${kind} +${amount} B币）` };
      return fail(result.status, { message: result.message, requestId: result.requestId });
    } catch {
      return fail(503, { message: '创建失败，请稍后重试' });
    }
  },

  update: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const version = Number(form.get('version') ?? 0);
    const reason = String(form.get('reason') ?? '').trim();
    const isEnabled = form.get('is_enabled') != null;
    if (!id) return fail(422, { message: '缺少规则标识' });
    if (!Number.isInteger(version) || version < 1) {
      return fail(422, { message: '规则版本缺失或无效，请刷新后重试' });
    }
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' });

    const body: Record<string, unknown> = { is_enabled: isEnabled, reason, currency_id: 'coin' };
    // 币种字段即使数额留空也必须是 B币，避免旧表单绕过币种限制。
    const currencyField = form.get('currency');
    const currency = currencyField == null || String(currencyField).trim() === ''
      ? 'coin'
      : String(currencyField).trim().toLowerCase();
    if (currency !== 'coin') {
      return fail(422, { message: '奖励币种仅支持 B币' });
    }
    // 数额留空 = 保持原值（后端 None = 沿用 current）。
    const amountRaw = String(form.get('amount') ?? '').trim();
    if (amountRaw !== '') {
      const amount = Number(amountRaw);
      if (!Number.isInteger(amount) || amount < 0) {
        return fail(422, { message: '奖励数额须为非负整数' });
      }
      body.amount = amount;
    }
    const dailyRaw = String(form.get('daily_limit') ?? '').trim();
    if (dailyRaw === '0') {
      body.daily_limit = null;
    } else if (dailyRaw !== '') {
      const daily = Number(dailyRaw);
      if (!Number.isInteger(daily) || daily < 1) {
        return fail(422, { message: '每日上限须 ≥ 1（输入 0 表示不限上限，留空保持原值）' });
      }
      body.daily_limit = daily;
    }
    const cooldown = optionalInt(form, 'cooldown_seconds');
    if (!cooldown.ok) return fail(422, { message: cooldown.message });
    if (cooldown.value != null) body.cooldown_seconds = cooldown.value;

    try {
      const result = await authedPatch<unknown>(
        cookies,
        `/api/v1/admin/activity/tasks/${encodeURIComponent(id)}`,
        body,
        { 'If-Match': String(version) },
        request.headers.get('x-request-id')
      );
      if (result.ok) return { message: `规则已更新（v${version + 1}）` };
      if (result.status === 409) {
        return fail(409, { message: `版本冲突：${result.message}（规则已被修改，请刷新后重试）` });
      }
      return fail(result.status, { message: result.message, requestId: result.requestId });
    } catch {
      return fail(503, { message: '保存失败，请稍后重试' });
    }
  }
};
