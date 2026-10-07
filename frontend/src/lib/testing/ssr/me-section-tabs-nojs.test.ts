// M02-UX-05：MeSectionTabs（个人中心分区 tag 栏）无 JS 基线。
// 每个 tag = 独立路由页面：账户资产 /me · 账号安全 /me/security ·
// 信任等级 /me/level；当前项由 URL 推导（aria-current='page'），
// SSR 直出，点击即整页切换，无页内锚点滚动。
import { describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import MeSectionTabs from '$lib/components/MeSectionTabs.svelte';

const mockState = vi.hoisted(() => ({ pathname: '/me' }));

vi.mock('$app/state', () => ({
  page: {
    url: {
      get pathname() {
        return mockState.pathname;
      },
      searchParams: new URLSearchParams()
    },
    params: {},
    data: {},
    route: { id: mockState.pathname }
  }
}));

function renderTabs() {
  return render(MeSectionTabs, { props: {} });
}

describe('无 JS：个人中心分区 tag 栏（tag = 独立路由页面）', () => {
  it('恒定渲染三个分区链接（/me · /me/security · /me/level）', () => {
    mockState.pathname = '/me';
    const { body } = renderTabs();
    expect(body).toMatch(/href="\/me"/);
    expect(body).toMatch(/href="\/me\/security"/);
    expect(body).toMatch(/href="\/me\/level"/);
    expect(body).toContain('账户资产');
    expect(body).toContain('账号安全');
    expect(body).toContain('信任等级');
  });

  it('当前页 tag 高亮：/me → 账户资产 aria-current=page', () => {
    mockState.pathname = '/me';
    const { body } = renderTabs();
    expect(body).toMatch(/aria-current="page"[^>]*>\s*账户资产/);
    expect(body).not.toMatch(/aria-current="page"[^>]*>\s*账号安全/);
  });

  it('/me/security → 账号安全高亮；/me/level → 信任等级高亮', () => {
    mockState.pathname = '/me/security';
    const security = renderTabs();
    expect(security.body).toMatch(/aria-current="page"[^>]*>\s*账号安全/);

    mockState.pathname = '/me/level';
    const level = renderTabs();
    expect(level.body).toMatch(/aria-current="page"[^>]*>\s*信任等级/);
  });
});
