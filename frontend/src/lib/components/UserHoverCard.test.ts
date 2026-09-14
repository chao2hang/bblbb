// UserHoverCard 数据补齐与关注交互测试（M03-UI-03 / GAP-FIX 社交域）。
//
// 卡片打开后客户端拉取 GET /users/{username}（公开端点）补齐
// created_at / 社交统计 / presentation_tokens / equipped_achievements /
// is_following；本文件用 mock fetch 验证：
//   1. 拉取完成 → 等级 chip / 统计行 / 加入时间 / 关注按钮出现；
//   2. 佩戴徽章行 = 成就墙装备槽的真实成就（equipped_achievements，
//      仅 code/name；无装备/拉取前不渲染伪造行）；
//   3. 点击关注 → 调用 follow API，按钮翻转、粉丝数更新；
//   4. follow 接口 401 → 按钮退化为「登录后关注」引导链接。
// 隐私契约（只渲染公开字段）由 privacy.test.ts / usercard-nojs.test.ts 守卫。
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import UserHoverCard from './UserHoverCard.svelte';

const CREATED_MS = Date.parse('2025-06-01T00:00:00Z');

function jsonResponse(data: unknown, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: async () => data,
    headers: new Headers()
  } as unknown as Response;
}

const profileData = {
  id: 'u1',
  username: 'chaos',
  display_name: 'Chaos',
  level: 4,
  bio: null,
  avatar_attachment_id: null,
  cover_attachment_id: null,
  signature: '人生苦短，再来一碗',
  created_at: CREATED_MS,
  post_count: 128,
  followers: 42,
  following: 17,
  is_following: false,
  presentation_tokens: null,
  equipped_achievements: [
    { code: 'first_post', name: '首发帖' },
    { code: 'community_elder', name: '社区元老' }
  ]
};

/** follow 端点状态可按用例切换（200 / 401）。 */
let followStatus = 200;

const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
  const url = String(input);
  if (url.endsWith('/me')) {
    return jsonResponse({ id: 'me1', username: 'viewer', level: 9 });
  }
  if (url.endsWith('/users/chaos')) return jsonResponse(profileData);
  if (url.endsWith('/users/chaos/follow')) {
    return followStatus === 200
      ? jsonResponse({ following: true, followers: 43 })
      : jsonResponse({ status: followStatus, detail: 'unauthorized' }, followStatus);
  }
  if (url.endsWith('/auth/csrf')) return jsonResponse({ token: 'test-csrf' });
  return jsonResponse({ status: 404, detail: 'not found' }, 404);
});

function renderCard() {
  return render(UserHoverCard, {
    props: { user: { username: 'chaos', display_name: 'Chaos' } }
  });
}

beforeEach(() => {
  followStatus = 200;
  vi.stubGlobal('fetch', fetchMock as unknown as typeof fetch);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe('UserHoverCard 数据补齐与关注交互', () => {
  it('打开后拉取公开资料：补齐等级 chip / 统计行 / 加入时间 / 关注按钮', async () => {
    renderCard();
    // 触发点数据立即可见（拉取前不渲染伪造行）。
    expect(screen.getByText('Chaos')).toBeTruthy();
    expect(screen.queryByText('帖子')).toBeNull();
    // 拉取前不渲染伪造徽章行（成就来自公开资料补齐）。
    expect(screen.queryByText('首发帖')).toBeNull();
    // 拉取完成 → 统计行 / 等级 chip（来自 profile）/ 加入时间出现。
    expect(await screen.findByText('128')).toBeTruthy();
    expect(screen.getByText('帖子')).toBeTruthy();
    expect(screen.getByText('粉丝')).toBeTruthy();
    expect(screen.getByText('关注')).toBeTruthy();
    expect(screen.getByText('TL4')).toBeTruthy();
    expect(screen.getByText(/加入于/)).toBeTruthy();
    // 登录视角 → 渲染真实关注按钮（而非登录引导链接）。
    expect(screen.getByRole('button', { name: '+ 关注' })).toBeTruthy();
  });

  it('悬浮卡渲染头像框与用户名称', async () => {
    render(UserHoverCard, {
      props: {
        user: { username: 'chaos', display_name: 'Chaos' },
        presentation: {
          presentation_tokens: {
            avatar_frame: 'gold_ring'
          }
        }
      }
    });

    await screen.findByText('128');
    expect(document.querySelector('.cosmetic-avatar__fallback-frame')).not.toBeNull();
    expect(document.querySelector('.user-hover-name')?.textContent?.trim()).toBe('Chaos');
  });

  it('佩戴徽章行渲染成就墙装备槽的真实成就（equipped_achievements）', async () => {
    renderCard();
    const tags = await screen.findByLabelText('佩戴的成就徽章');
    expect(tags.textContent).toContain('首发帖');
    expect(tags.textContent).toContain('社区元老');
    // 只渲染 code/name 公开字段映射出的成就名，无商城装扮 Token。
    expect(tags.textContent).not.toContain('first_post');
  });

  it('无已装备成就 → 徽章行不渲染（缺省行不伪造）', async () => {
    const fetchMockEmpty = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith('/me')) {
        return jsonResponse({ id: 'me1', username: 'viewer', level: 9 });
      }
      if (url.endsWith('/users/chaos')) {
        return jsonResponse({ ...profileData, equipped_achievements: [] });
      }
      return jsonResponse({ status: 404, detail: 'not found' }, 404);
    });
    vi.stubGlobal('fetch', fetchMockEmpty as unknown as typeof fetch);
    render(UserHoverCard, {
      props: { user: { username: 'chaos', display_name: 'Chaos' } }
    });
    await screen.findByText('128');
    expect(screen.queryByLabelText('佩戴的成就徽章')).toBeNull();
  });

  it('点击关注 → 调用 follow API，按钮翻转且粉丝数更新', async () => {
    renderCard();
    const btn = await screen.findByRole('button', { name: '+ 关注' });
    await fireEvent.click(btn);
    expect(await screen.findByRole('button', { name: '已关注 · 取消' })).toBeTruthy();
    expect(screen.getByText('43')).toBeTruthy();
  });

  it('关注接口 401 → 按钮退化为登录引导链接', async () => {
    followStatus = 401;
    renderCard();
    const btn = await screen.findByRole('button', { name: '+ 关注' });
    await fireEvent.click(btn);
    const loginLink = await screen.findByRole('link', { name: '登录后关注' });
    expect(loginLink.getAttribute('href')).toBe('/login?next=%2Fusers%2Fchaos');
  });

  it('统计 → 对应页面：帖子/粉丝/关注渲染为链接且指向正确路由', async () => {
    renderCard();
    await screen.findByText('128');
    // 帖子 → 用户主页内容 tab；粉丝/关注 → 关系列表页（与用户主页 meta 一致）。
    expect(screen.getByText('帖子').closest('a')?.getAttribute('href')).toBe('/users/chaos?tab=posts');
    expect(screen.getByText('粉丝').closest('a')?.getAttribute('href')).toBe('/users/chaos/followers');
    expect(screen.getByText('关注').closest('a')?.getAttribute('href')).toBe('/users/chaos/following');
  });

  it('装扮试穿（preview）模式统计保持非链接，不做导航', async () => {
    render(UserHoverCard, {
      props: {
        user: { username: 'chaos', display_name: 'Chaos' },
        preview: true
      }
    });
    await screen.findByText('128');
    expect(screen.getByText('帖子').closest('a')).toBeNull();
    expect(screen.getByText('粉丝').closest('a')).toBeNull();
    expect(screen.getByText('关注').closest('a')).toBeNull();
  });
});
