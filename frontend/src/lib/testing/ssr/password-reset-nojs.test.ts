// M02-UX-04：忘记/重置密码页无 JS 基线——SSR 输出原生 form[method=POST]，
// 确认页带 token 时输出隐藏 token + 新密码表单，成功后输出“其他 Session
// 已撤销”提示；无 JS 可完成全流程。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import PasswordResetPage from '../../../routes/password-reset/+page.svelte';
import ConfirmPage from '../../../routes/password-reset/confirm/+page.svelte';

describe('无 JS：忘记密码页（M02-UX-04）', () => {
  it('SSR 输出原生 form[method=POST] + email 输入', () => {
    const { body } = render(PasswordResetPage, { props: { form: undefined } });
    expect(body).toMatch(/<form[^>]*method="POST"/);
    expect(body).toContain('name="email"');
    expect(body).toContain('发送重置链接');
  });

  it('统一 202 → sent 成功面板（不泄漏是否注册）', () => {
    const { body } = render(PasswordResetPage, {
      props: { form: { sent: true, email: 'alice@example.com' } }
    });
    expect(body).toContain('重置链接已发送');
    expect(body).toContain('如果该邮箱已注册');
    expect(body).toContain('/login');
  });
});

describe('无 JS：重置密码页（M02-UX-04）', () => {
  it('带 token：SSR 输出原生 form[method=POST] + 隐藏 token + 新密码/确认输入', () => {
    const { body } = render(ConfirmPage, {
      props: { data: { token: 'tok-ssr' }, form: undefined }
    });
    expect(body).toMatch(/<form[^>]*method="POST"/);
    expect(body).toContain('name="token"');
    expect(body).toContain('name="password"');
    expect(body).toContain('name="confirm"');
    expect(body).toContain('重置密码');
  });

  it('成功 → 成功面板明确提示其他 Session 已撤销', () => {
    const { body } = render(ConfirmPage, {
      props: { data: { token: 'tok-ssr' }, form: { ok: true } }
    });
    expect(body).toContain('密码已重置');
    expect(body).toContain('其他设备上的会话已全部撤销');
    expect(body).toContain('/login');
  });

  it('无 token：提示从邮件完整链接进入，不渲染密码表单', () => {
    const { body } = render(ConfirmPage, {
      props: { data: { token: null }, form: undefined }
    });
    expect(body).toContain('请从重置邮件中的完整链接进入本页');
    expect(body).not.toContain('name="password"');
  });
});
