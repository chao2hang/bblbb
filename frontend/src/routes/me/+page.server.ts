// M02-UX-05：/me 个人主页（概览枢纽）服务端 load。
//
// 2026-09 功能拆分：本页收敛为「概览 + 入口」——
// - 会话设备管理（revoke / logoutall）与两步验证（TOTP/Passkey）拆至
//   /me/security（设备撤销等写 action 一并平移）；
// - 两步验证的完整管理在专属页 /mfa（M18-MFA-01）；
// - 本页 load 仍取设备列表，仅用于资料卡「登录设备 N 台」概览计数，
//   不再承载任何写操作。
//
// - load：转发浏览器会话 Cookie → GET /api/v1/me（安全投影，仅渲染自身
//   账号可见字段，不输出任何会话 token）；401 → 跳登录；
// - GAP-FIX 增强数据（非致命，失败只降级对应区块）：GET /activity/summary
//   （账户卡：等级/经验/B币）与 GET /me/sanctions（我的处罚，失败 → 空列表）；
//   M20-TRUST：GET /me/trust-level（失败降级 null，卡片隐藏）。

import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import type { ActivitySummary, CosmeticDef, Entitlement, Presentation, SanctionItem, TrustLevelProgress, User } from '$lib/api/types';
import type { DeviceSession } from '$lib/api/generated/v1';

export interface MePageData {
  user: User | null;
  sessions: DeviceSession[];
  error: string | null;
  /** GAP-FIX 账户卡：等级/经验/B币（GET /activity/summary，失败降级 null）。
   *  可选：旧 fixture/渐进迁移下允许缺失（页面按 null 处理）。 */
  activity?: ActivitySummary | null;
  /** GAP-FIX 我的处罚（GET /me/sanctions；失败时页面按空列表处理）。
   *  可选：同上。 */
  sanctions?: SanctionItem[];
  /** M20-TRUST 信任等级进度（GET /me/trust-level；失败降级 null，卡片隐藏）。 */
  trust?: TrustLevelProgress | null;
  presentation?: Presentation | null;
  cosmetics?: CosmeticDef[];
  entitlements?: Entitlement[];
  /** 个人资料封面（GET /api/v1/users/{id}/profile-cover，失败降级 null）。 */
  cover?: {
    attachment_id: string;
    alt_text?: string;
    position?: string;
    content_url?: string;
  } | null;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const meResult = await getAuthed<User>(cookies, '/api/v1/me', requestId);
  if (meResult.ok === false) {
    if (meResult.status === 401) throw redirect(303, '/login');
    return {
      user: null,
      sessions: [],
      error: meResult.message,
      activity: null,
      sanctions: []
    } satisfies MePageData;
  }
  const sessionsResult = await getAuthed<DeviceSession[]>(
    cookies,
    '/api/v1/auth/sessions',
    requestId
  );
  if (sessionsResult.ok === false) {
    if (sessionsResult.status === 401) throw redirect(303, '/login');
    return {
      user: meResult.data,
      sessions: [],
      error: sessionsResult.message,
      activity: null,
      sanctions: []
    } satisfies MePageData;
  }

  // GAP-FIX 账户卡 + 我的处罚（增强数据，非致命——失败只降级对应区块）：
  // - GET /activity/summary：等级/经验/B币（economy.rs 已落地）；
  // - GET /me/sanctions：本人处罚记录；失败时降级为空列表。
  // M20-TRUST：GET /me/trust-level 信任等级进度（失败降级 null，卡片隐藏）。
  const [activityResult, sanctionsResult, trustResult, presentationResult, cosmeticsResult, entitlementsResult, coverResult] = await Promise.all([
    getAuthed<ActivitySummary>(cookies, '/api/v1/activity/summary', requestId),
    getAuthed<{ items?: SanctionItem[] }>(cookies, '/api/v1/me/sanctions', requestId),
    getAuthed<TrustLevelProgress>(cookies, '/api/v1/me/trust-level', requestId),
    getAuthed<Presentation>(cookies, '/api/v1/me/presentation', requestId),
    getAuthed<{ cosmetics?: CosmeticDef[] }>(cookies, '/api/v1/shop/cosmetics', requestId),
    getAuthed<Entitlement[] | { entitlements?: Entitlement[]; items?: Entitlement[] }>(
      cookies,
      '/api/v1/me/entitlements',
      requestId
    ),
    meResult.data?.id
      ? getAuthed<{
          attachment_id: string;
          alt_text?: string;
          position?: string;
          content_url?: string;
        }>(cookies, `/api/v1/users/${meResult.data.id}/profile-cover`, requestId)
      : Promise.resolve({ ok: false as const, status: 404, message: '', requestId: null, retryAfterSecs: null, code: null })
  ]);
  const activity = activityResult.ok ? activityResult.data : null;
  const sanctions =
    sanctionsResult.ok && Array.isArray(sanctionsResult.data.items) ? sanctionsResult.data.items : [];
  const trust = trustResult.ok ? trustResult.data : null;
  const presentation = presentationResult.ok ? presentationResult.data : null;
  const cosmetics = cosmeticsResult.ok && Array.isArray(cosmeticsResult.data?.cosmetics) ? cosmeticsResult.data.cosmetics : [];
  const entitlements = entitlementsResult.ok
    ? Array.isArray(entitlementsResult.data)
      ? entitlementsResult.data
      : Array.isArray(entitlementsResult.data.entitlements)
        ? entitlementsResult.data.entitlements
        : Array.isArray(entitlementsResult.data.items)
          ? entitlementsResult.data.items
          : []
    : [];
  const cover = coverResult.ok && coverResult.data?.attachment_id ? coverResult.data : null;

  const sessions = sessionsResult.data;
  return {
    user: meResult.data,
    sessions,
    error: null,
    activity,
    sanctions,
    trust,
    presentation,
    cosmetics,
    entitlements,
    cover
  } satisfies MePageData;
};
