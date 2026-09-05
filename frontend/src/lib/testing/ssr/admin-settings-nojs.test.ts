// 原型对齐（prototype/pages/admin-settings.html）：系统设置页无 JS SSR 基线——
// app-card 三卡布局（功能开关+配置建议 / 站点信息 / 数据与导出）+ 页脚状态行与
// 「恢复默认 / 保存设置」操作行；保存按钮在 SSR（无 JS）时不得禁用，脏禁用仅
// hydration 后生效；公开源（public_source，0063）与原型逐字段对齐。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import AdminSettings from '../../../routes/admin/settings/+page.svelte';

const SETTINGS = {
  open_registration: true,
  email_verification: true,
  anonymous_replies: false,
  public_rss: true,
  maintenance_mode: false,
  site_name: 'BBLBB 社区',
  default_lang: 'zh-CN',
  public_source: 'https://bblbb.local',
  api_rate_limit: 30
};

describe('系统设置页 SSR（原型 app-card 布局对齐）', () => {
  it('ok 状态渲染三卡布局 + 页脚操作行', () => {
    const { body } = render(AdminSettings, {
      props: { data: { state: 'ok', settings: SETTINGS, version: 7, error: null }, form: null }
    });
    // 原型三卡结构（功能开关 / 站点信息 / 数据与导出）
    expect(body).toContain('功能开关');
    expect(body).toContain('站点信息');
    expect(body).toContain('数据与导出');
    expect(body).toContain('app-card__head');
    expect(body).toContain('app-card__foot');
    expect(body).toContain('sys-foot');
    expect(body).toContain('admin-action-row');
    // 五个开关（原型顺序与文案）
    for (const label of ['开放注册', '注册邮箱验证', '匿名回复', '公开 RSS / Atom', '维护模式']) {
      expect(body).toContain(label);
    }
    // 开关 hint 与原型逐字一致
    expect(body).toContain('关闭后仅管理员可邀请新用户');
    expect(body).toContain('为订阅工具提供只读 feed');
    // 站点信息表单（含公开源、审计原因与版本守卫）
    expect(body).toMatch(/<form[^>]*action="\?\/save"/);
    expect(body).toContain('name="version" value="7"');
    expect(body).toContain('name="site_name"');
    expect(body).toContain('name="default_lang"');
    expect(body).toContain('name="public_source"');
    expect(body).toContain('value="https://bblbb.local"');
    expect(body).toContain('name="api_rate_limit"');
    expect(body).toContain('name="reason"');
    // 原型字段文案：公开源必填 + 限流标签「次 / 分钟 / IP」+ 站点名称 ≤40
    expect(body).toContain('公开源（RSS / API）');
    expect(body).toContain('API 限流（次 / 分钟 / IP）');
    expect(body).toContain('maxlength="40"');
    // 页脚按钮位置：恢复默认（ghost）+ 保存设置（primary）
    expect(body).toContain('恢复默认');
    expect(body).toContain('保存设置');
    // 状态行初始文案（原型 data-sys-status 同款）
    expect(body).toContain('与已保存配置一致');
    // 初始态无脏标记、无内联错误、无配置建议（配置条件未触发）
    expect(body).not.toContain('is-dirty');
    expect(body).not.toContain('app-field-error');
    expect(body).not.toContain('配置建议');
  });

  it('SSR（无 JS）时保存按钮可提交——脏禁用仅 hydration 后启用', () => {
    const { body } = render(AdminSettings, {
      props: { data: { state: 'ok', settings: SETTINGS, version: 1, error: null }, form: null }
    });
    const submit = body.match(/<button[^>]*type="submit"[^>]*>/);
    expect(submit).not.toBeNull();
    expect(submit?.[0]).not.toContain('disabled');
  });

  it('维护模式开启时渲染原型 .app-notice 提示与配置建议', () => {
    const { body } = render(AdminSettings, {
      props: {
        data: {
          state: 'ok',
          settings: { ...SETTINGS, maintenance_mode: true },
          version: 2,
          error: null
        },
        form: null
      }
    });
    expect(body).toContain('app-notice');
    expect(body).toContain('维护模式已开启：前台用户会看到维护提示，仅管理员可访问后台。');
    // 配置建议（原型 data-sys-advisory）：维护模式 → 提示语
    expect(body).toContain('app-promo');
    expect(body).toContain('配置建议');
    expect(body).toContain('维护模式下前台访客将看到维护提示，仅管理员可操作');
  });

  it('开注册且关邮箱验证时渲染配置建议（原型 advisory 同逻辑）', () => {
    const { body } = render(AdminSettings, {
      props: {
        data: {
          state: 'ok',
          settings: { ...SETTINGS, email_verification: false },
          version: 3,
          error: null
        },
        form: null
      }
    });
    expect(body).toContain('app-promo');
    expect(body).toContain('注册开放且邮箱验证关闭，新账号注册后即可发帖，建议开启邮箱验证');
  });

  it('If-Match 冲突（409）渲染 .app-error 版本冲突横幅', () => {
    const { body } = render(AdminSettings, {
      props: {
        data: { state: 'ok', settings: SETTINGS, version: 3, error: null },
        form: { conflict: true, message: '版本冲突：version mismatch（设置已被其他人修改，请刷新后重试）' }
      }
    });
    expect(body).toContain('app-error');
    expect(body).toContain('保存失败（版本冲突）');
    expect(body).toContain('请刷新页面获取最新版本后再保存');
  });

  it('403 渲染无权限态；501 渲染开发中；error 渲染错误文案', () => {
    const forbidden = render(AdminSettings, {
      props: { data: { state: 'forbidden', settings: null, version: 0, error: null }, form: null }
    });
    expect(forbidden.body).toContain('无权限：该操作仅限管理员');

    const ni = render(AdminSettings, {
      props: { data: { state: 'not_implemented', settings: null, version: 0, error: null }, form: null }
    });
    expect(ni.body).toContain('系统设置接口开发中');

    const err = render(AdminSettings, {
      props: { data: { state: 'error', settings: null, version: 0, error: 'boom' }, form: null }
    });
    expect(err.body).toContain('boom');
  });
});
