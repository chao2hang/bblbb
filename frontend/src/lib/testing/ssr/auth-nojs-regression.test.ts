// M02-UX-08：无 JavaScript 认证流程的合理退化审计。
//
// 断言各认证页面（注册/登录/邮箱验证/重发/找回密码/重置密码）在 SSR 阶段
// 输出的 HTML：
//  1. 原生 `form[method=POST]`（服务端 form action），变更一律 POST 而非 GET；
//  2. 不渲染会话 Cookie / CSRF token / 密码等敏感值（认证裁决在后端，
//     前端不持有 token 明文）；
//  3. 无浏览器端认证逻辑工件（localStorage/sessionStorage 读取认证态、
//     内联 fetch 到认证 API）——use:enhance 仅渐进增强同一 action；
//  4. 根布局输出 `<noscript>` 降级提示。
import { describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import { createRawSnippet } from 'svelte';
import RegisterPage from '../../../routes/register/+page.svelte';
import LoginPage from '../../../routes/login/+page.svelte';
import VerifyEmailPage from '../../../routes/verify-email/+page.svelte';
import PasswordResetPage from '../../../routes/password-reset/+page.svelte';
import ConfirmResetPage from '../../../routes/password-reset/confirm/+page.svelte';
import MePage from '../../../routes/me/+page.svelte';
import RootLayout from '../../../routes/+layout.svelte';

// Navbar 的 isActive 读取 $app/state page.url.pathname；隔离渲染需提供假 page。
vi.mock('$app/state', () => ({
  page: { url: { pathname: '/' }, data: {} }
}));

const user = {
  id: 'u-1',
  username: 'alice',
  email: 'alice@example.com',
  email_verified: true,
  status: 'active',
  display_name: null,
  level: 1,
  roles: ['member'],
  mfa_enabled: false
};

const meData = {
  user,
  sessions: [],
  currentSessionId: null,
  error: null
};

interface PageCase {
  name: string;
  render: () => string;
}

const pages: PageCase[] = [
  { name: '注册', render: () => render(RegisterPage, { props: { form: undefined } }).body },
  { name: '登录', render: () => render(LoginPage, { props: { form: undefined } }).body },
  { name: '登录（MFA 步）', render: () => render(LoginPage, { props: { form: { mfa_required: true, challenge_token: 'ch-ssr' } } }).body },
  { name: '邮箱验证', render: () => render(VerifyEmailPage, { props: { data: { token: 'tok-ssr' }, form: undefined } }).body },
  { name: '找回密码', render: () => render(PasswordResetPage, { props: { form: undefined } }).body },
  { name: '重置密码', render: () => render(ConfirmResetPage, { props: { data: { token: 'tok-ssr' }, form: undefined } }).body },
  { name: '我的主页', render: () => render(MePage, { props: { data: meData, form: undefined } }).body }
];

describe('无 JS 认证退化：变更一律走服务端 form action（M02-UX-08）', () => {
  for (const page of pages) {
    it(`${page.name}：SSR 输出原生 form[method=POST]，无 GET 变更表单`, () => {
      const body = page.render();
      expect(body).toMatch(/<form[^>]*method="POST"/);
      expect(body).not.toMatch(/<form[^>]*method="get"/);
    });
  }
});

describe('无 JS 认证退化：认证裁决不在浏览器（M02-UX-08）', () => {
  for (const page of pages) {
    it(`${page.name}：不渲染会话/CSRF/密码敏感值，无浏览器端认证逻辑`, () => {
      const body = page.render();
      // 会话 Cookie / CSRF token 明文不得进入 SSR HTML
      expect(body).not.toContain('__Host-bblbb_session=');
      expect(body).not.toContain('__Host-bblbb_csrf=');
      expect(body).not.toContain('X-CSRF-Token');
      // 浏览器端认证态工件
      expect(body).not.toContain('localStorage');
      expect(body).not.toContain('sessionStorage');
      // use:enhance 是渐进增强：内联增强不携带认证数据（不 fetch 认证 API）
      expect(body).not.toContain('fetch("/api/v1/auth');
      expect(body).not.toContain("fetch('/api/v1/auth");
    });
  }

  it('MFA 挑战 token 仅作隐藏表单值，不进入可见文本', () => {
    const body = pages.find((p) => p.name === '登录（MFA 步）')!.render();
    // 隐藏域存在（无 JS 提交必需）
    expect(body).toContain('name="challenge_token"');
    // 但值不应以明文可见文本形式渲染
    expect(body).not.toContain('>ch-ssr<');
  });
});

describe('无 JS 认证退化：根布局输出降级提示（M02-UX-08）', () => {
  it('+layout.svelte SSR 输出 <noscript> 需要启用 JavaScript 提示', () => {
    const children = createRawSnippet(() => ({ render: () => '<div />' }));
    const { body } = render(RootLayout, { props: { children } });
    expect(body).toContain('<noscript>');
    expect(body).toContain('需要启用 JavaScript');
  });
});
