import type { LayoutServerLoad } from './$types';
import { getAuthed, SESSION_COOKIE } from '$lib/api/server';
import type { Notification, User } from '$lib/api/types';
import type { ActiveThemeView } from '$lib/theme/projection';

// M00-FRONTEND-06：SSR/浏览器缓存边界。
//
// 站点包含 Session（navbar 用户态）、通知、设置等个人化内容，且帖子存在
// 非公开可见性；任何 HTML 一旦进入共享缓存都可能泄漏会话化片段。
// 因此根 layout 对所有 SSR 响应固定 `Cache-Control: private, no-store`：
//  - 浏览器/中间层不得缓存页面 HTML（始终回源渲染）；
//  - 静态哈希资源（public, immutable, max-age=31536000）由部署层另行配置，
//    与这里互不影响；
//  - 客户端 API 响应（/api/v1/*）由后端各自控制，不在此范围。

/** 全局壳·通知徽标状态（仅本人可见的私有数据，走 no-store）。 */
export interface LayoutNotifications {
  /** 未读数（Navbar 铃铛徽标；匿名/后端不可用为 0）。 */
  unreadCount: number;
  /** 最近几条速览（铃铛下拉；匿名/后端不可用为空）。 */
  recent: Notification[];
}

export const load: LayoutServerLoad = async ({ cookies, request, setHeaders }) => {
  // GAP-FIX 修复：页面 load 的内部 fetch（/api/v1/*）响应携带
  // Cache-Control: private, no-store 时，SvelteKit 会把它传播进页面响应头；
  // 布局与页面 load 并行执行，若页面先完成 fetch，此处再 setHeaders 会抛
  // '"Cache-Control" header is already set'（曾致 /messages 间歇 500）。
  // 传播值与本布局意图一致（private, no-store），冲突时直接沿用。
  try {
    setHeaders({
      'Cache-Control': 'private, no-store'
    });
  } catch {
    // 已由内部 fetch 传播设置，语义相同，无需覆盖。
  }
  const requestId = request.headers.get('x-request-id');

  // 并行获取当前生效主题（用户偏好优先，站点默认次之，内置 default 兜底）
  const themePromise = getAuthed<ActiveThemeView>(
    cookies,
    '/api/v1/themes/active',
    requestId
  ).catch(() => null);

  // 会话与通知徽标：登录用户（会话 Cookie 存在）时服务端取 /me（Navbar 用户态：
  // SSR 首帧即渲染真实登录态——hydration 后 navbar 不闪、无 JS 基线一致）+
  // unread_count 与最近 3 条速览（limit=3，避免整页拉全量；unread_count 是
  // 响应的独立字段）。两请求并行，互不阻塞。
  // 失败（含 401 过期会话/后端不可达）→ 对应空态，绝不阻塞整站渲染；
  // 浏览器端导航的增量刷新见 +layout.svelte 的会话刷新路径。
  let user: User | null = null;
  let notifications: LayoutNotifications = { unreadCount: 0, recent: [] };
  if (cookies.get(SESSION_COOKIE)) {
    const meResult = getAuthed<User>(cookies, '/api/v1/me', requestId).catch(
      () => null
    );
    const notifResult = getAuthed<{ items?: Notification[]; unread_count?: number }>(
      cookies,
      '/api/v1/notifications?limit=3',
      requestId
    ).catch(() => null);
    const [me, result] = await Promise.all([meResult, notifResult]);
    if (me?.ok) {
      user = me.data;
    }
    if (result?.ok) {
      notifications = {
        unreadCount: typeof result.data.unread_count === 'number' ? result.data.unread_count : 0,
        recent: Array.isArray(result.data.items) ? result.data.items.slice(0, 3) : []
      };
    }
  }

  const themeResult = await themePromise;
  const activeTheme: ActiveThemeView | null = themeResult?.ok && themeResult.data ? themeResult.data : null;

  const data: {
    notifications: LayoutNotifications;
    user: User | null;
    activeTheme?: ActiveThemeView | null;
  } = {
    notifications,
    user,
    activeTheme
  };

  return data;
};
