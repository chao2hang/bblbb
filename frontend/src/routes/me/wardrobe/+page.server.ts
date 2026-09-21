// M07-UI-05：衣柜——装备/卸下/徽章（关联成就页徽章系统与商城装扮）/装饰预览。
//
// - load：GET /me/presentation + GET /me/entitlements + 成就系统（listAchievements + listMyAchievements）；
//   401 → 登录。
// - equip/unequip action：
//   1. 若提交 achievement_code：PUT/DELETE /api/v1/me/achievements/{code}/equip（成就徽章佩戴/卸下，最多 3 枚）；
//   2. 若提交 entitlement_id：POST /me/entitlements/{id}/equip|unequip，body 含 expected_presentation_version。
// - 乐观并发 + use:enhance 刷新投影。

import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, getAuthed } from '$lib/api/server';
import {
  equipAchievement,
  listAchievements,
  listMyAchievements,
  unequipAchievement
} from '$lib/api/client';
import type {
  AchievementDef,
  CosmeticDef,
  Entitlement,
  MyAchievementItem,
  Presentation,
  User
} from '$lib/api/types';
import { problemMessage, type Problem } from '$lib/errors';

export interface WardrobeAchievementBadge {
  code: string;
  name: string;
  description: string;
  category: string;
  iconUrl: string | null;
  unlocked: boolean;
  unlockedAt: number | null;
  equipped: boolean;
}

export interface WardrobePageData {
  presentation: Presentation | null;
  user?: User | null;
  entitlements: Entitlement[];
  /** 装扮样式库定义（M07-SHOP-UI-10）：自定义样式的名称标签解析；可缺省。 */
  cosmetics: CosmeticDef[];
  /** 成就系统徽章池（包含已解锁与佩戴状态） */
  achievementBadges?: WardrobeAchievementBadge[];
  /** 成就徽章统计（已解锁/总数/已佩戴/上限） */
  achievementStats?: {
    unlocked: number;
    total: number;
    equipped: number;
    maxSlots: number;
  };
  error: string | null;
}

export interface WardrobeActionData {
  ok?: boolean;
  message?: string;
  code?: string | null;
  requestId?: string | null;
}

export const load: PageServerLoad = async ({ cookies, request, fetch }) => {
  const requestId = request.headers.get('x-request-id');
  const presResult = await getAuthed<Presentation>(cookies, '/api/v1/me/presentation', requestId);
  if (!presResult.ok && presResult.status === 401) throw redirect(303, '/login');
  if (!presResult.ok) {
    return {
      presentation: null,
      user: null,
      entitlements: [],
      cosmetics: [],
      achievementBadges: [],
      achievementStats: { unlocked: 0, total: 0, equipped: 0, maxSlots: 3 },
      error: presResult.message
    } satisfies WardrobePageData;
  }
  const entResult = await getAuthed<{ entitlements?: Entitlement[]; items?: Entitlement[] }>(cookies, '/api/v1/me/entitlements', requestId);
  if (!entResult.ok) {
    return {
      presentation: presResult.data,
      user: null,
      entitlements: [],
      cosmetics: [],
      achievementBadges: [],
      achievementStats: { unlocked: 0, total: 0, equipped: 0, maxSlots: 3 },
      error: entResult.message
    } satisfies WardrobePageData;
  }
  const meResult = await getAuthed<User>(cookies, '/api/v1/me', requestId);
  // 样式库（自定义装扮名称解析）：nice-to-have，放最后取；失败降级为空。
  const cosResult = await getAuthed<{ cosmetics: CosmeticDef[] }>(cookies, '/api/v1/shop/cosmetics', requestId);

  // 获取成就定义与我的成就进度，构建徽章池（关联成就页）
  let achievementBadges: WardrobeAchievementBadge[] = [];
  let achievementStats = { unlocked: 0, total: 0, equipped: 0, maxSlots: 3 };
  try {
    const [defsPage, myPage] = await Promise.all([
      listAchievements(fetch).catch(() => ({ items: [] as AchievementDef[] })),
      listMyAchievements(fetch).catch(() => ({
        items: [] as MyAchievementItem[],
        stats: { unlocked: 0, total: 0, equipped: 0, max_slots: 3 }
      }))
    ]);
    const defs = defsPage.items ?? [];
    const mine = myPage.items ?? [];
    const byCode = new Map(mine.map((m) => [m.code, m]));

    achievementBadges = defs.map((def) => {
      const my = byCode.get(def.code);
      const unlocked = typeof my?.unlocked_at === 'number' && my.unlocked_at > 0;
      return {
        code: def.code,
        name: def.name,
        description: def.description,
        category: def.category,
        iconUrl: def.icon_url ?? null,
        unlocked,
        unlockedAt: unlocked ? (my?.unlocked_at ?? null) : null,
        equipped: my?.equipped === true
      };
    });

    achievementStats = {
      unlocked: myPage.stats?.unlocked ?? achievementBadges.filter((b) => b.unlocked).length,
      total: myPage.stats?.total ?? defs.length,
      equipped: myPage.stats?.equipped ?? achievementBadges.filter((b) => b.equipped).length,
      maxSlots: myPage.stats?.max_slots ?? 3
    };
  } catch {
    // 成就服务故障时安全降级为空列表
  }

  return {
    presentation: presResult.data,
    user: meResult?.ok ? meResult.data : null,
    entitlements: entResult.data.entitlements ?? entResult.data.items ?? [],
    cosmetics: cosResult.ok ? cosResult.data.cosmetics ?? [] : [],
    achievementBadges,
    achievementStats,
    error: null
  } satisfies WardrobePageData;
};

async function runEquip(
  cookies: Parameters<Actions['equip']>[0]['cookies'],
  entitlementId: string,
  expectedVersion: number,
  requestId: string | null,
  kind: 'equip' | 'unequip'
): Promise<ReturnType<typeof fail> | WardrobeActionData> {
  if (!entitlementId) {
    return fail(422, { message: '缺少权益标识' } satisfies WardrobeActionData);
  }
  if (!Number.isInteger(expectedVersion) || expectedVersion < 1) {
    return fail(422, { message: '展示版本缺失或无效，请刷新页面后重试' } satisfies WardrobeActionData);
  }
  try {
    const result = await authedPost<Presentation>(
      cookies,
      `/api/v1/me/entitlements/${encodeURIComponent(entitlementId)}/${kind}`,
      { expected_presentation_version: expectedVersion },
      requestId,
      { 'Idempotency-Key': `${kind}-${entitlementId}-${expectedVersion}` }
    );
    if (result.ok) {
      return { ok: true, message: kind === 'equip' ? '已装备' : '已卸下' } satisfies WardrobeActionData;
    }
    if (result.status === 409) {
      return fail(409, {
        message: result.message,
        code: result.code
      } satisfies WardrobeActionData);
    }
    return fail(result.status, { message: result.message, requestId: result.requestId } satisfies WardrobeActionData);
  } catch (e) {
    if (isRedirect(e)) throw e;
    return fail(503, { message: '衣柜服务暂不可用，请稍后重试' } satisfies WardrobeActionData);
  }
}

export const actions: Actions = {
  equip: async ({ request, cookies, fetch }) => {
    const form = await request.formData();
    // 优先检查是否为成就徽章装备
    const achievementCode = String(form.get('achievement_code') ?? '').trim();
    if (achievementCode) {
      try {
        await equipAchievement(fetch, achievementCode);
        return { ok: true, message: '成就徽章已佩戴' } satisfies WardrobeActionData;
      } catch (e) {
        const p = e && typeof e === 'object' ? (e as Problem) : null;
        if (p?.status === 409) {
          return fail(409, { message: '徽章槽位已满（最多 3 枚），请先卸下一枚徽章' } satisfies WardrobeActionData);
        }
        return fail(400, { message: problemMessage(p) || '佩戴成就徽章失败' } satisfies WardrobeActionData);
      }
    }

    // 否则按普通装扮权益处理
    return runEquip(
      cookies,
      String(form.get('entitlement_id') ?? ''),
      Number(form.get('expected_presentation_version') ?? 0),
      request.headers.get('x-request-id'),
      'equip'
    );
  },
  unequip: async ({ request, cookies, fetch }) => {
    const form = await request.formData();
    // 优先检查是否为成就徽章卸下
    const achievementCode = String(form.get('achievement_code') ?? '').trim();
    if (achievementCode) {
      try {
        await unequipAchievement(fetch, achievementCode);
        return { ok: true, message: '成就徽章已卸下' } satisfies WardrobeActionData;
      } catch (e) {
        const p = e && typeof e === 'object' ? (e as Problem) : null;
        return fail(400, { message: problemMessage(p) || '卸下成就徽章失败' } satisfies WardrobeActionData);
      }
    }

    // 否则按普通装扮权益处理
    return runEquip(
      cookies,
      String(form.get('entitlement_id') ?? ''),
      Number(form.get('expected_presentation_version') ?? 0),
      request.headers.get('x-request-id'),
      'unequip'
    );
  }
};
