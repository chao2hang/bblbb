// M03-UI-02：/settings 页无 JS SSR 基线——资料编辑原生表单可读、隐藏版本
// 字段存在、冲突态提示、成功态投影刷新、不输出任何会话/私有凭据字段。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import SettingsPage from '../../../routes/settings/+page.svelte';
import type { User } from '$lib/api/types';

const user: User = {
  id: 'u1',
  username: 'alice',
  email: 'alice@example.com',
  email_verified: true,
  status: 'active',
  display_name: '爱丽丝',
  bio: '公开简介',
  signature: '公开签名',
  timezone: 'UTC',
  theme_name: null,
  email_visible_to: 'nobody',
  profile_visible_to: 'everyone',
  level: 7,
  roles: ['member'],
  mfa_enabled: false,
  version: 3
};

const updatedUser = { ...user, display_name: '新昵称', signature: '新签名', version: 4 };

describe('M03-UI-02 /settings 无 JS SSR 基线', () => {
  it('SSR 输出原生资料编辑表单（POST ?/profile）与隐藏版本字段', () => {
    const { body } = render(SettingsPage, { props: { data: { user, error: null }, form: undefined } });
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/profile"/);
    expect(body).toContain('name="version"');
    expect(body).toContain('value="3"');
    expect(body).toContain('name="avatar_attachment_id"');
    expect(body).toContain('个人头像');
    expect(body).toContain('id="set-display-name"');
    // 资料表单现为「签名」单字段（bio 已随页面迭代移除，公开投影不再含简介）。
    expect(body).toContain('id="set-signature"');
    expect(body).toContain('公开签名');
    expect(body).not.toContain('id="set-bio"');
    expect(body).toContain('爱丽丝');
  });

  it('版本冲突 → 提示横幅与“加载最新资料”入口，且保留表单字段值', () => {
    const { body } = render(SettingsPage, {
      props: { data: { user, error: null }, form: { conflict: true, message: '资料已在其他窗口被修改，请刷新后重新编辑' } }
    });
    expect(body).toContain('版本冲突');
    expect(body).toContain('加载最新资料');
    expect(body).toMatch(/<a[^>]*href="\/settings"[^>]*>加载最新资料/);
    expect(body).toContain('name="version"');
    expect(body).toContain('value="3"');
  });

  it('保存成功 → 渲染更新后投影（新昵称/新签名/新版本）', () => {
    const { body } = render(SettingsPage, {
      props: { data: { user, error: null }, form: { ok: true, user: updatedUser } }
    });
    expect(body).toContain('新昵称');
    expect(body).toContain('新签名');
    expect(body).toContain('value="4"'); // 隐藏版本已刷新为新版本
    expect(body).toContain('当前公开投影');
  });

  it('隐私守卫：对抗性 user（混入会话/凭据字段）不进入 HTML', () => {
    const adversarial = {
      ...user,
      password_hash: 'SETTINGS-PAGE-HASH',
      session_token: 'SETTINGS-PAGE-TOKEN'
    };
    const { body } = render(SettingsPage, { props: { data: { user: adversarial, error: null }, form: undefined } });
    expect(body).not.toContain('SETTINGS-PAGE-HASH');
    expect(body).not.toContain('SETTINGS-PAGE-TOKEN');
    // 表单只输出 version 一个隐藏字段（无 session_id/token 等）。
    expect(body).not.toContain('name="session_id"');
  });

  it('load 失败 → 错误横幅，不渲染表单', () => {
    const { body } = render(SettingsPage, { props: { data: { user: null, error: '服务暂不可用' }, form: undefined } });
    expect(body).toContain('服务暂不可用');
    expect(body).not.toMatch(/<form[^>]*action="\?\/profile"/);
  });

  it('账号安全区域包含修改密码、两步验证（2FA）与 Passkey 设置', () => {
    const { body } = render(SettingsPage, {
      props: {
        data: {
          user,
          error: null,
          passkeyEnabled: true,
          passkeys: [
            {
              id: 'pk-1',
              name: '我的指纹钥匙',
              aaguid: null,
              backup_eligible: true,
              backed_up: true,
              created_at: 1700000000000,
              last_used_at: 1700001000000
            }
          ]
        },
        form: undefined
      }
    });
    // 包含修改密码表单
    expect(body).toContain('修改密码');
    expect(body).toContain('id="set-current-password"');
    // 包含两步验证（2FA）卡片与入口
    expect(body).toContain('两步验证（2FA / TOTP）');
    expect(body).toContain('未启用');
    expect(body).toContain('立即开启两步验证');
    expect(body).toContain('/mfa');
    // 包含 Passkey（通行密钥）卡片与已绑定列表
    expect(body).toContain('Passkey（通行密钥）');
    expect(body).toContain('已绑定 1 把密钥');
    expect(body).toContain('我的指纹钥匙');
    expect(body).toContain('已云同步');
    expect(body).toContain('action="?/passkeyRevoke"');
  });

  it('当用户已启用 MFA 时，两步验证卡片显示已启用状态及管理入口', () => {
    const userWithMfa = { ...user, mfa_enabled: true };
    const { body } = render(SettingsPage, {
      props: {
        data: {
          user: userWithMfa,
          error: null,
          passkeyEnabled: false
        },
        form: undefined
      }
    });
    expect(body).toContain('两步验证（2FA / TOTP）');
    expect(body).toContain('已启用');
    expect(body).toContain('管理两步验证与恢复码');
    expect(body).toContain('服务端尚未配置 Passkey');
  });
});
