// M02-UX-02：邮箱验证页无 JS 基线——SSR 输出原生 form[method=POST]
// 指向 ?/verify 与 ?/resend 命名 action，无 JS 时可提交；
// 未验证账号允许/禁止动作说明进入 SSR HTML。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import VerifyEmailPage from '../../../routes/verify-email/+page.svelte';

describe('无 JS：邮箱验证页（M02-UX-02）', () => {
  it('带 token：SSR 输出原生 form[action=?/verify] + 隐藏 token', () => {
    const { body } = render(VerifyEmailPage, {
      props: { data: { token: 'tok-ssr' }, form: undefined }
    });
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/verify"/);
    expect(body).toContain('name="token"');
    expect(body).toContain('完成邮箱验证');
  });

  it('SSR 输出原生 form[action=?/resend] + email 输入 + 提交按钮', () => {
    const { body } = render(VerifyEmailPage, {
      props: { data: { token: null }, form: undefined }
    });
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/resend"/);
    expect(body).toContain('name="email"');
    expect(body).toContain('重新发送验证邮件');
  });

  it('未验证账号允许/禁止动作说明进入 SSR HTML', () => {
    const { body } = render(VerifyEmailPage, {
      props: { data: { token: null }, form: undefined }
    });
    expect(body).toContain('不能');
    expect(body).toContain('发帖、回复、上传附件、参与交易或领取活动奖励');
    expect(body).toContain('可以');
  });

  it('验证成功态渲染成功面板（form.ok）', () => {
    const { body } = render(VerifyEmailPage, {
      props: { data: { token: 't' }, form: { ok: true } }
    });
    expect(body).toContain('验证成功');
    expect(body).toContain('前往首页');
  });
});
