// M02-UX-03：登录页无 JS 基线——SSR 输出原生 form[method=POST]，
// 启用 TOTP 的账号第二步表单（challenge_token 隐藏域 + 验证码输入）也
// 是无 JS 可提交的原生表单；认证裁决始终在后端。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import LoginPage from '../../../routes/login/+page.svelte';

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
});
