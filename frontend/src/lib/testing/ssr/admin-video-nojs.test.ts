// M10-UI-06 & M18-ADMIN-VIDEO：管理端视频页 SSR 快照。
//
// - not_implemented → 开发中态（后端未实现时核心论坛不受影响）；
// - ok → 逐 Provider 只读卡（审计信息：策略版本/更新时间、停用徽章）+
//   「保存策略/测试/恢复默认」触发按钮（写操作 = 按钮 → Dialog，无 JS 不渲染）；
// - 转码队列为只读 Mock 投影（无写端点）→ 无选择列（无死 UI）；
// - 隐私守卫：对抗性 Provider（Secret/内部字段）不进入 SSR HTML。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import AdminVideoPage from '../../../routes/admin/video/+page.svelte';
import type { AdminVideoPageData } from '../../../routes/admin/video/+page.server';

const clientRequestId = 'req-key-0000000000000002';

const okData: AdminVideoPageData = {
  state: 'ok',
  clientRequestId,
  error: null,
  policies: {
    enabled: true,
    version: 3,
    items: [
      {
        provider: 'direct',
        enabled: true,
        allowed_hosts: ['media.example.com'],
        embed_hosts: [],
        allowed_media_types: ['video/mp4', 'video/webm'],
        max_duration_seconds: 1800,
        timeout_ms: 15000,
        policy_version: 2,
        updated_at: 1700000000000
      },
      {
        provider: 'xigua',
        enabled: false,
        allowed_hosts: ['www.xigua.example'],
        embed_hosts: ['player.xigua.example'],
        allowed_media_types: [],
        policy_version: 4,
        updated_at: 1700000000000
      }
    ]
  }
};

const notImplementedData: AdminVideoPageData = {
  state: 'not_implemented',
  clientRequestId,
  error: 'not implemented',
  policies: null
};

describe('M10-UI-06 管理端视频 SSR', () => {
  it('not_implemented → 开发中态（不影响核心论坛）', () => {
    const { body } = render(AdminVideoPage, { props: { data: notImplementedData, form: null } });
    expect(body).toContain('视频管理接口开发中');
    expect(body).toContain('核心论坛功能不受影响');
  });

  it('ok → 逐 Provider 只读卡：审计版本、停用徽章、按钮+弹层触发按钮', () => {
    const { body } = render(AdminVideoPage, { props: { data: okData, form: null } });
    expect(body).toContain('直链视频（direct）');
    expect(body).toContain('西瓜视频（xigua）');
    expect(body).toContain('已停用');
    // 新契约：每个 Provider 一组「保存策略 / 测试 / 恢复默认」触发按钮（弹层无 JS 不渲染）
    const saveButtons = body.match(/<span>保存策略<\/span>/g);
    expect(saveButtons?.length).toBe(2);
    expect(body).toContain('<span>测试</span>');
    expect(body).toContain('<span>恢复默认</span>');
    // 行数据审计要素（If-Match 版本）仍按行展示
    expect(body).toContain('审计：策略版本 v2');
    expect(body).toContain('审计：策略版本 v4');
  });

  it('ok → 测试按钮文案、审计信息展示策略版本与更新时间', () => {
    const { body } = render(AdminVideoPage, { props: { data: okData, form: null } });
    expect(body).toContain('测试');
    expect(body).toContain('审计：策略版本 v2');
    expect(body).toContain('审计：策略版本 v4');
    expect(body).toContain('更新于');
    expect(body).toContain('服务端写入审计');
  });

  it('隐私守卫：对抗性 Provider（Secret/内部字段）不进入 HTML', () => {
    const adversarial = {
      state: 'ok',
      clientRequestId,
      error: null,
      policies: {
        items: [
          {
            provider: 'direct',
            enabled: true,
            allowed_hosts: ['media.example.com'],
            embed_hosts: [],
            allowed_media_types: [],
            policy_version: 1,
            provider_secret: 'ADMIN-VIDEO-SSR-KEY',
            s3_signing_secret: 'ADMIN-VIDEO-SSR-SECRET',
            signed_url_template: 'https://internal.example/s?={SSR-SIG}'
          }
        ]
      }
    } as unknown as AdminVideoPageData;
    const { body } = render(AdminVideoPage, { props: { data: adversarial, form: null } });
    expect(body).not.toContain('ADMIN-VIDEO-SSR-KEY');
    expect(body).not.toContain('ADMIN-VIDEO-SSR-SECRET');
    expect(body).not.toContain('internal.example');
  });

  it('403 → 无权限态；站点功能关闭 → 说明', () => {
    const forbidden = render(AdminVideoPage, {
      props: { data: { state: 'forbidden', clientRequestId, error: 'forbidden', policies: null }, form: null }
    });
    expect(forbidden.body).toContain('没有权限访问视频管理');

    const disabled = render(AdminVideoPage, {
      props: {
        data: { state: 'ok', clientRequestId, error: null, policies: { items: [], enabled: false } },
        form: null
      }
    });
    expect(disabled.body).toContain('视频功能未开放（Feature Flag 默认关闭）');
  });

  it('P1-06 & P1-07: 白名单改为按钮+弹层，转码任务多元状态渲染且无死选择列', () => {
    const { body } = render(AdminVideoPage, {
      props: {
        data: {
          ...okData,
          whitelistConfig: {
            enableEmbed: true,
            domainWhitelist: 'surprise-test.example, bilibili.com',
            strictMode: 'strict',
            fallbackMode: 'safe_link'
          }
        },
        form: null
      }
    });
    // 新契约：白名单写操作 = 「编辑白名单与安全」按钮 → Dialog（?/save-whitelist），
    // 弹层本体与输入值无 JS 不渲染
    expect(body).toContain('编辑白名单与安全');
    expect(body).not.toContain('action="?/save-whitelist"');
    // 转码队列：P0 整改移除 mock 投影 → 空队列表格 + 真实空态文案
    expect(body).not.toContain('全选当前列表');
    expect(body).not.toContain('项已选');
    expect(body).not.toContain('已转码');
    expect(body).toContain('当前筛选下没有转码任务');
  });
});
