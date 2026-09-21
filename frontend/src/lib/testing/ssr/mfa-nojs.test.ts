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

// M02-MFA-PK：服务端未配置 Passkey 的默认形态（整块隐藏）
const noPasskey = { passkeyEnabled: false, passkeys: [], passkeysError: null };

function renderPage(form?: unknown): string {
  const { body } = render(MfaPage, {
    props: {
      data: { user, error: null, ...noPasskey },
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
        data: { user: { ...user, mfa_enabled: true }, error: null, ...noPasskey }
      }
    });
    expect(body).toContain('已启用');
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/recovery"/);
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/disable"/);
  });

  it('recovery-codes：SSR 一次展示恢复码并提示只能使用一次（M02-UX-SEC 平移）', () => {
    const body = renderPage({
      mfa: { kind: 'recovery-codes', codes: ['ABCDEFGHIJKLMNOP', 'QRSTUVWXYZ234567'] }
    });
    expect(body).toContain('ABCDEFGHIJKLMNOP');
    expect(body).toContain('QRSTUVWXYZ234567');
    expect(body).toContain('每个只能使用一次');
  });

  it('Passkey：启用时输出管理卡片与 ?/passkeyRevoke 原生表单', () => {
    const { body } = render(MfaPage, {
      props: {
        data: {
          user: { ...user, mfa_enabled: true },
          error: null,
          passkeyEnabled: true,
          passkeys: [
            {
              id: 'pk-1',
              name: 'MacBook 指纹',
              aaguid: null,
              backup_eligible: true,
              backed_up: true,
              created_at: 1700000000000,
              last_used_at: 1700000001000
            }
          ],
          passkeysError: null
        }
      }
    });
    expect(body).toContain('Passkey');
    expect(body).toContain('MacBook 指纹');
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/passkeyRevoke"/);
    expect(body).toContain('name="id" value="pk-1"');
    // 已云同步徽标（backup 状态随断言更新）
    expect(body).toContain('已云同步');
  });

  it('Passkey：未配置时整块隐藏', () => {
    const body = renderPage(undefined);
    expect(body).not.toContain('passkeyRevoke');
  });
});

// M02-UX-SEC：step-up 重认证各态自 /me 页平移（M02-MFA-07）——recovery /
// disable 遇 403 step_up_required 时，本页输出 ?/re-auth 密码确认表单；
// reauth-done 后按 intent 引导重试原操作。
describe('无 JS：/mfa step-up 重认证（M02-MFA-07 平移）', () => {
  it('step-up：SSR 输出 ?/re-auth 原生表单 + intent 隐藏域', () => {
    const body = renderPage({ mfa: { kind: 'step-up', intent: 'disable' } });
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/re-auth"/);
    expect(body).toContain('name="password"');
    expect(body).toContain('name="intent"');
    expect(body).toContain('验证身份');
  });

  it('reauth-done（intent=disable）：SSR 输出重试 ?/disable 表单', () => {
    const body = renderPage({ mfa: { kind: 'reauth-done', intent: 'disable' } });
    expect(body).toContain('身份已验证');
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/disable"/);
  });

  it('reauth-done（intent=recovery）：重试表单指向 ?/recovery', () => {
    const body = renderPage({ mfa: { kind: 'reauth-done', intent: 'recovery' } });
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/recovery"/);
  });
});
