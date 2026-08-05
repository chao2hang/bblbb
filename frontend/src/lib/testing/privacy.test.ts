// M00-FRONTEND-09：hydration payload、预取与客户端 store 的隐私守卫测试。
//
// 背景：SvelteKit 会把 server load 的输出序列化进 SSR HTML（hydration payload，
// 即 __data.json / data-sveltekit-fetched），`data-sveltekit-preload-data="hover"`
// 预取的是同一份 load 输出；客户端 store（User 会话投影、首页 $state）也只消费
// 这些投影。因此「load 输出 = hydration = 预取数据源」必须只含公开字段。
//
// 本测试用「对抗性后端响应」（多返回 email/password_hash/session_token/totp_secret/
// 隐藏正文等字段）验证白名单投影确实把它们挡在 SSR/客户端之外。
import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { load } from '../../routes/+page.server';
import Navbar from '$lib/components/Navbar.svelte';
import appHtml from '../../app.html?raw';

vi.mock('$app/state', () => ({
  page: { url: { pathname: '/' }, params: {}, data: {}, route: { id: '/' } }
}));
vi.mock('$app/navigation', () => ({
  goto: vi.fn(),
  invalidate: vi.fn(),
  invalidateAll: vi.fn()
}));

function jsonResponse(data: unknown): Response {
  return new Response(JSON.stringify(data), {
    status: 200,
    headers: { 'Content-Type': 'application/json' }
  });
}

/** 对抗性板块：白名单 6 字段之外混入邮箱/凭据/密钥。 */
const adversarialBoard = {
  id: 'b1',
  slug: 'general',
  name: '综合讨论',
  description: '日常闲聊',
  post_count: 3,
  is_active: true,
  email: 'owner@example.com',
  password_hash: '$argon2id$HASH-SECRET',
  session_token: 'TOKEN-SECRET',
  totp_secret: 'T0T0-SECRET',
  secret_field: 'BOARD-SECRET'
};

/** 对抗性标签。 */
const adversarialTag = {
  id: 't1',
  name: 'svelte',
  usage_count: 5,
  created_at: 0,
  api_key: 'AKIA-SECRET'
};

/** 对抗性帖子：除了私密字段，还带隐藏正文（非公开可见性不应流出）。 */
const adversarialPost = {
  id: 'p1',
  title: '你好 BBLBB',
  author_id: 'u1',
  reply_count: 2,
  view_count: 10,
  pinned: false,
  created_at: 0,
  last_reply_at: null,
  content: 'PUBLIC-BODY-SHOULD-NOT-LEAK',
  body_html: '<p>PUBLIC-BODY-SHOULD-NOT-LEAK</p>',
  hidden_body: '<p>HIDDEN-SECRET-BODY</p>',
  visibility: 'logged_in'
};

const FORBIDDEN = [
  'password_hash',
  'session_token',
  'totp_secret',
  'secret',
  'api_key',
  'PUBLIC-BODY-SHOULD-NOT-LEAK',
  'HIDDEN-SECRET-BODY',
  'owner@example.com',
  'AKIA-SECRET'
];

interface HomeLoadData {
  boards: Record<string, unknown>[];
  tags: Record<string, unknown>[];
  posts: Record<string, unknown>[];
}

async function runLoad(fetchMock: typeof fetch): Promise<HomeLoadData> {
  return (await load({ fetch: fetchMock } as never)) as HomeLoadData;
}

function adversarialFetch(): typeof fetch {
  return vi.fn(async (url: string | URL | Request) => {
    const u = String(url);
    if (u.includes('/api/v1/boards'))
      return jsonResponse({ items: [adversarialBoard], next_cursor: null, has_more: false });
    if (u.includes('/api/v1/tags'))
      return jsonResponse({ items: [adversarialTag], next_cursor: null, has_more: false });
    if (u.includes('/api/v1/search'))
      return jsonResponse({ items: [adversarialPost], next_cursor: null, has_more: false });
    return jsonResponse({});
  }) as typeof fetch;
}

describe('M00-FRONTEND-09 隐私守卫：hydration payload / 预取数据源', () => {
  it('load 输出只含公开字段白名单（对抗性后端响应被拦截）', async () => {
    const data = await runLoad(adversarialFetch());
    const serialized = JSON.stringify(data);

    // hydration/预取序列化后不得出现任何凭据、令牌、密钥、邮箱或正文内容。
    for (const forbidden of FORBIDDEN) {
      expect(serialized).not.toContain(forbidden);
    }

    // 数组元素只保留白名单字段（不得出现混入的私密键）。
    expect(Object.keys(data.boards[0]).sort()).toEqual(
      ['description', 'id', 'is_active', 'name', 'post_count', 'slug'].sort()
    );
    expect(Object.keys(data.tags[0]).sort()).toEqual(['id', 'name', 'slug', 'usage_count'].sort());
    expect(Object.keys(data.posts[0]).sort()).toEqual(
      ['author_id', 'created_at', 'id', 'last_reply_at', 'pinned', 'reply_count', 'title', 'view_count'].sort()
    );
  });

  it('白名单字段的值完整保留（公开数据不丢失）', async () => {
    const data = await runLoad(adversarialFetch());
    expect(data.boards[0]).toMatchObject({
      id: 'b1',
      slug: 'general',
      name: '综合讨论',
      description: '日常闲聊',
      post_count: 3,
      is_active: true
    });
    expect(data.tags[0]).toMatchObject({ id: 't1', name: 'svelte', usage_count: 5 });
    expect(data.posts[0]).toMatchObject({ id: 'p1', title: '你好 BBLBB', reply_count: 2 });
  });

  it('app.html 启用 hover 预取，且预取载荷（load 输出）不含私密字段', async () => {
    // 预取提示本身只是「hover 时预取」的开关，不携带任何数据。
    expect(appHtml).toContain('data-sveltekit-preload-data="hover"');
    // 预取拉取的是各路由 load 输出（__data.json），其隐私由上面的 load 测试保证。
    const data = await runLoad(adversarialFetch());
    expect(JSON.stringify(data)).not.toMatch(/password|token|secret|body_html|hidden/);
  });
});

describe('M00-FRONTEND-09 隐私守卫：客户端 store（会话投影渲染）', () => {
  it('Navbar 用户态只渲染公开字段，不渲染邮箱/凭据值', async () => {
    // 混合了私密字段的用户对象：Navbar 的公开 prop 类型本就拒绝这些字段，
    // 这里用 as never 模拟「运行时混入」的对抗场景。
    const navUser = {
      username: 'alice',
      display_name: '爱丽丝',
      level: 7,
      roles: ['member'],
      email: 'alice@example.com',
      password_hash: 'CLIENT-STORE-HASH',
      session_token: 'CLIENT-STORE-TOKEN',
      totp_secret: 'CLIENT-STORE-TOTP'
    } as never;

    render(Navbar, {
      props: {
        user: navUser,
        unread: 0
      }
    });

    // 头像以 display_name 为 aria-label/title（公开字段），不暴露邮箱或凭据。
    expect(screen.getByRole('img', { name: '爱丽丝' })).toBeTruthy();

    // 展开用户菜单：显示名称与等级，仍不出现邮箱/凭据值。
    await userEvent.click(screen.getByRole('button', { name: '用户菜单' }));
    expect(screen.getByText('爱丽丝')).toBeTruthy();
    expect(screen.getByText('LV.7')).toBeTruthy();

    expect(document.body.textContent).not.toContain('alice@example.com');
    expect(document.body.textContent).not.toContain('CLIENT-STORE-HASH');
    expect(document.body.textContent).not.toContain('CLIENT-STORE-TOKEN');
    expect(document.body.textContent).not.toContain('CLIENT-STORE-TOTP');
  });
});