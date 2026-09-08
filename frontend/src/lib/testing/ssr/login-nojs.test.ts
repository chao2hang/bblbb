// M02-UX-03：登录页无 JS 基线——SSR 输出原生 form[method=POST]，
// 启用 TOTP 的账号第二步表单（challenge_token 隐藏域 + 验证码输入）也
// 是无 JS 可提交的原生表单；认证裁决始终在后端。
import { describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import LoginPage from '../../../routes/login/+page.svelte';

// 登录页的回跳目标（next 隐藏域）读取 $app/state page.url.searchParams；
// 隔离渲染需提供假 page（含 searchParams）。
vi.mock('$app/state', () => ({
  page: {
    url: { pathname: '/login', searchParams: new URLSearchParams() },
    data: {}
  }
}));

describe('无 JS：登录页（M02-UX-03）', () => {
  it('密码步：SSR 输出原生 form[method=POST] + 字段', () => {
    const { body } = render(LoginPage, { props: { form: undefined } });
    expect(body).toMatch(/<form[^>]*method="POST"/);
    expect(body).toContain('name="identifier"');
    expect(body).toContain('name="password"');
    expect(body).toContain('登录');
  });

  it('MFA 步：form[method=POST][action=?/mfa] + 隐藏 challenge_token + 验证码输入', () => {
    const { body } = render(LoginPage, {
      props: { form: { mfa_required: true, challenge_token: 'ch-ssr' } }
    });
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/mfa"/);
    expect(body).toContain('name="challenge_token"');
    expect(body).toContain('name="totp_code"');
    expect(body).toContain('验证并登录');
  });

  it('密码步初始不渲染 MFA 表单（无 mfa_required）', () => {
    const { body } = render(LoginPage, { props: { form: undefined } });
    expect(body).not.toContain('action="?/mfa"');
  });

  it('启用第三方登录时渲染 Google 与 GitHub 登录入口', () => {
    const { body } = render(LoginPage, {
      props: {
        data: {
          site: {
            siteName: 'BBLBB',
            siteDescription: 'BBLBB 社区论坛',
            loginEyebrow: 'WELCOME BACK',
            loginTitle: '登录 BBLBB',
            loginSubtitle: 'BBLBB 社区论坛',
            registerEyebrow: 'JOIN BBLBB',
            registerTitle: '创建账号',
            registerSubtitle: 'BBLBB 社区论坛',
            maintenanceMode: false,
            googleLoginEnabled: true,
            githubLoginEnabled: true,
          }
        },
        form: undefined
      }
    });
    expect(body).toContain('/api/v1/auth/oauth/google/start');
    expect(body).toContain('/api/v1/auth/oauth/github/start');
    expect(body).toContain('Google 账号登录');
    expect(body).toContain('GitHub 账号登录');
  });

  it('未启用第三方登录时不渲染入口', () => {
    const { body } = render(LoginPage, {
      props: {
        data: {
          site: {
            siteName: 'BBLBB',
            siteDescription: 'BBLBB 社区论坛',
            loginEyebrow: 'WELCOME BACK',
            loginTitle: '登录 BBLBB',
            loginSubtitle: 'BBLBB 社区论坛',
            registerEyebrow: 'JOIN BBLBB',
            registerTitle: '创建账号',
            registerSubtitle: 'BBLBB 社区论坛',
            maintenanceMode: false,
            googleLoginEnabled: false,
            githubLoginEnabled: false,
          }
        },
        form: undefined
      }
    });
    expect(body).not.toContain('/api/v1/auth/oauth/google/start');
    expect(body).not.toContain('/api/v1/auth/oauth/github/start');
  });
});
