import { describe, expect, it } from 'vitest';
import { resolveSiteCopy } from './copy';

const rawSite = {
  site_name: '示例社区',
  site_description: '分享与讨论',
  currency_name: 'B币',
  login_eyebrow: 'WELCOME',
  login_title: '登录示例社区',
  login_subtitle: '欢迎回来',
  register_eyebrow: 'JOIN',
  register_title: '创建账号',
  register_subtitle: '加入社区',
  maintenance_mode: false,
  google_login_enabled: false,
  github_login_enabled: false,
  version: 1
};

describe('resolveSiteCopy', () => {
  it('resolves raw API fields', () => {
    const resolved = resolveSiteCopy(rawSite);
    expect(resolved.siteName).toBe('示例社区');
    expect(resolved.currencyName).toBe('B币');
  });

  it('is idempotent for an already-resolved view', () => {
    const resolved = resolveSiteCopy(rawSite);
    expect(resolveSiteCopy(resolved)).toEqual(resolved);
  });

  it('uses stable defaults when the API result is missing', () => {
    expect(resolveSiteCopy(null)).toMatchObject({ siteName: 'BBLBB', currencyName: '金币' });
  });
});
