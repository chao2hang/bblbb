// M18-MFA-01：/mfa 页无 JS 基线——enroll 流程各态均为原生 form[method=POST]，
// 注册二维码以 <img data-URL> 输出（SSR/无 JS 可直接扫码），二维码缺失时降级
// 为手工录入提示；密钥/otpauth 文本恒定保留。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import MfaPage from '../../../routes/mfa/+page.svelte';

const user = {
  id: 'u-1',
  username: 'alice',
  email: 'alice@example.com',
  email_verified: true,
  status: 'active',
  display_name: null,
  level: 3,
  roles: ['member'],
  mfa_enabled: false,
  version: 3
};

function renderPage(form?: unknown): string {
  const { body } = render(MfaPage, {
    props: {
      data: { user, error: null },
      ...(form === undefined ? {} : { form })
    }
  });
  return body;
}

describe('无 JS：/mfa 页（M18-MFA-01）', () => {
  it('未启用：SSR 输出 ?/enroll 原生表单 + 未启用徽标', () => {
    const body = renderPage(undefined);
    expect(body).toContain('未启用');
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/enroll"/);
    expect(body).not.toContain('?/disable');
  });

  it('enroll-challenge：SSR 输出二维码 <img> + 密钥降级 + ?/confirm 与 ?/cancel', () => {
    const body = renderPage({
      mfa: {
        kind: 'enroll-challenge',
        otpauth_uri: 'otpauth://totp/BBLBB:alice@example.com',
        secret_base32: 'JBSWY3DPEHPK3PXP',
        qr_data_url: 'data:image/svg+xml;base64,TESTQR'
      }
    });
    // 二维码以 <img data-URL> 输出（无 JS 可直接扫码）
    expect(body).toContain('data:image/svg+xml;base64,TESTQR');
    expect(body).toMatch(/<img[^>]*alt="两步验证注册二维码/);
    // 手工录入降级（details 折叠不影响 SSR 输出）
    expect(body).toContain('JBSWY3DPEHPK3PXP');
    expect(body).toContain('otpauth://totp/');
    // 确认/取消均为原生表单
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/confirm"/);
    expect(body).toContain('name="code"');
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/cancel"/);
  });

  it('enroll-challenge 无二维码 → 降级提示，不输出 <img>', () => {
    const body = renderPage({
      mfa: {
        kind: 'enroll-challenge',
        otpauth_uri: 'otpauth://totp/BBLBB:alice@example.com',
        secret_base32: 'JBSWY3DPEHPK3PXP',
        qr_data_url: null
      }
    });
    expect(body).toContain('二维码生成失败');
    expect(body).toContain('JBSWY3DPEHPK3PXP');
    expect(body).not.toMatch(/<img[^>]*otp-qr/);
  });

  it('已启用：SSR 输出 ?/recovery 与 ?/disable 原生表单', () => {
    const { body } = render(MfaPage, {
      props: {
        data: { user: { ...user, mfa_enabled: true }, error: null }
      }
    });
    expect(body).toContain('已启用');
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/recovery"/);
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/disable"/);
  });
});
