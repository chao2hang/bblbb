// GAP-FIX（社交域·成就，achievements.rs）：/achievements——成就墙。
//
// - load：未登录 → /login；listAchievements（全部定义，隐藏成就后端已
//   脱敏）+ listMyAchievements（进度/装备态）合并渲染；
// - equip/unequip action：PUT/DELETE /me/achievements/{code}/equip（超
//   max_slots 409）→ 页面 toast + invalidateAll；
// - 失败态：返回 Problem（ProblemState 渲染），不抛 500。
import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import {
  equipAchievement,
  listAchievements,
  listMyAchievements,
  unequipAchievement
} from '$lib/api/client';
import type { AchievementDef, MyAchievementItem } from '$lib/api/types';
import { problemMessage, type Problem } from '$lib/errors';

/** 合并后的成就卡片（隐藏且未解锁 → 名称/描述显示 ???）。 */
export interface AchievementCard {
  code: string;
  name: string;
  description: string;
  category: string;
  rewardCoin: number;
  isHidden: boolean;
  /** 后台上传的成就图标（本地磁盘，不走 S3）；null = 未上传。 */
  iconUrl: string | null;
  unlocked: boolean;
  unlockedAt: number | null;
  /** 进行中进度（已解锁 = target 满条）。 */
  progress: number;
  /** 0 = 后端无进度行（不渲染进度条）。 */
  target: number;
  equipped: boolean;
}

export interface AchievementStats {
  unlocked: number;
  total: number;
  equipped: number;
  maxSlots: number;
}

export interface AchievementsPageData {
  cards: AchievementCard[];
  stats: AchievementStats;
  problem: Problem | null;
  error: string | null;
}

export interface AchievementsActionData {
  ok?: boolean;
  message?: string;
}

function asProblem(e: unknown): Problem | null {
  return e && typeof e === 'object' ? (e as Problem) : null;
}

export const load: PageServerLoad = async ({ fetch }) => {
  try {
    const [defsPage, myPage] = await Promise.all([listAchievements(fetch), listMyAchievements(fetch)]);
    const defs: AchievementDef[] = defsPage.items ?? [];
    const mine: MyAchievementItem[] = myPage.items ?? [];
    const byCode = new Map(mine.map((m) => [m.code, m]));

    const cards: AchievementCard[] = defs.map((def) => {
      const my = byCode.get(def.code);
      const unlocked = typeof my?.unlocked_at === 'number' && my.unlocked_at > 0;
      const hiddenLocked = def.is_hidden && !unlocked;
      const target = typeof my?.target === 'number' ? my.target : 0;
      return {
        code: def.code,
        name: hiddenLocked ? '???' : def.name,
        description: hiddenLocked ? '隐藏成就：达成条件保密，解锁后揭晓' : def.description,
        category: def.category,
        rewardCoin: def.reward_coin,
        isHidden: def.is_hidden,
        iconUrl: def.icon_url ?? null,
        unlocked,
        unlockedAt: unlocked ? (my?.unlocked_at ?? null) : null,
        progress: unlocked ? target : (my?.progress ?? 0),
        target,
        equipped: my?.equipped === true
      };
    });

    const stats: AchievementStats = {
      unlocked: myPage.stats?.unlocked ?? cards.filter((c) => c.unlocked).length,
      total: myPage.stats?.total ?? cards.length,
      equipped: myPage.stats?.equipped ?? mine.filter((m) => m.equipped).length,
      maxSlots: myPage.stats?.max_slots ?? 3
    };

    return { cards, stats, problem: null, error: null } satisfies AchievementsPageData;
  } catch (e) {
    const p = asProblem(e);
    if (p?.status === 401) throw redirect(303, '/login');
    return {
      cards: [],
      stats: { unlocked: 0, total: 0, equipped: 0, maxSlots: 3 },
      problem: p,
      error: problemMessage(p)
    } satisfies AchievementsPageData;
  }
};

export const actions: Actions = {
  equip: async ({ request, fetch }) => {
    const form = await request.formData();
    const code = String(form.get('code') ?? '').trim();
    if (!code) {
      return fail(422, { message: '缺少成就标识，请刷新后重试' } satisfies AchievementsActionData);
    }
    try {
      await equipAchievement(fetch, code);
      return { ok: true, message: '徽章已装备' } satisfies AchievementsActionData;
    } catch (e) {
      const p = asProblem(e);
      if (p?.status === 401) {
        return fail(401, { message: '登录已过期，请重新登录' } satisfies AchievementsActionData);
      }
      const status = typeof p?.status === 'number' && p.status >= 400 && p.status <= 599 ? p.status : 503;
      return fail(status, { message: problemMessage(p) } satisfies AchievementsActionData);
    }
  },
  unequip: async ({ request, fetch }) => {
    const form = await request.formData();
    const code = String(form.get('code') ?? '').trim();
    if (!code) {
      return fail(422, { message: '缺少成就标识，请刷新后重试' } satisfies AchievementsActionData);
    }
    try {
      await unequipAchievement(fetch, code);
      return { ok: true, message: '徽章已卸下' } satisfies AchievementsActionData;
    } catch (e) {
      const p = asProblem(e);
      if (p?.status === 401) {
        return fail(401, { message: '登录已过期，请重新登录' } satisfies AchievementsActionData);
      }
      const status = typeof p?.status === 'number' && p.status >= 400 && p.status <= 599 ? p.status : 503;
      return fail(status, { message: problemMessage(p) } satisfies AchievementsActionData);
    }
  }
};
