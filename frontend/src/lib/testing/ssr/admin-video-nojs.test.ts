// M10-UI-06：管理端视频页 SSR 快照。
//
// - not_implemented → 开发中态（后端未实现时核心论坛不受影响）；
// - ok → 逐 Provider 原生表单（If-Match 版本 + reason 必填 + test formaction）、
//   审计信息（策略版本/更新时间）、停用状态徽章；
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

  it('ok → 逐 Provider 原生表单：If-Match 版本、reason 必填、停用徽章', () => {
    const { body } = render(AdminVideoPage, { props: { data: okData, form: null } });
    expect(body).toContain('直链视频（direct）');
    expect(body).toContain('西瓜视频（xigua）');
    expect(body).toContain('已停用');
    // 每个 Provider 一张 save 表单 + 隐藏 provider/expected_version + reason
    const forms = body.match(/<form[^>]*action="\?\/save"[^>]*>/g);
    expect(forms?.length).toBe(2);
    expect(body).toContain('name="provider"');
    expect(body).toContain('name="expected_version"');
    expect(body).toContain('value="2"');
    expect(body).toContain('value="4"');
    expect(body).toContain('name="reason"');
    expect(body).toContain('必填（写审计）');
  });

  it('ok → 测试按钮走 formaction="?/test"，审计信息展示策略版本与更新时间', () => {
    const { body } = render(AdminVideoPage, { props: { data: okData, form: null } });
    expect(body).toMatch(/formaction="\?\/test"/);
    expect(body).toContain('测试此 Provider');
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
});
