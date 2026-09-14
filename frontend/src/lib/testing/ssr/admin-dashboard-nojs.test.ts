// M19·值班台优化：/admin 仪表盘无 JS SSR 基线——
// 周期 Tab（链接式）+ MetricGroup 统计条（去盒、mono 值）+ 运营趋势 +
// 双栏值班区（最近管理员操作 / 关键入口 + 生产健康提示）。
// 断言：演示遗留「切换普通成员」不出现；待审 > 0 呈 warning 信号；
// 未收录审计码以 mono 原始码回退；403 无权限态不泄漏统计 DOM。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import AdminDashboard from '../../../routes/admin/+page.svelte';
import type { AdminDashboardPageData } from '../../../routes/admin/+page.server';
import type { AdminStats, AdminStatsTrend } from '../../../lib/api/types';

const NOW = Date.parse('2026-09-11T12:00:00+08:00');

const STATS: AdminStats = {
  members: 1284,
  members_delta_7d: 42,
  posts_today: 56,
  posts_today_delta: -8,
  posts_yesterday: 64,
  reports_pending: 3,
  active_today: 421,
  recent_admin_actions: [
    { action: 'admin.points.adjust', actor_username: 'Chaos', created_at: NOW },
    { action: 'attachment.complete', actor_username: 'Chaos', created_at: NOW }
  ]
};

const TREND: AdminStatsTrend = {
  period: 'day',
  bucket_ms: 3_600_000,
  buckets: Array.from({ length: 8 }, (_, i) => ({
    start: NOW - (8 - i) * 3_600_000,
    end: NOW - (7 - i) * 3_600_000,
    posts: i,
    comments: 0,
    active_users: i * 2,
    reports: 0
  }))
};

function okData(overrides: Partial<AdminDashboardPageData> = {}): AdminDashboardPageData {
  return { state: 'ok', stats: STATS, trend: TREND, period: 'day', error: null, ...overrides };
}

describe('管理仪表盘 SSR（M19 值班台）', () => {
  it('ok 状态渲染统计条/趋势/值班区，且无演示遗留入口', () => {
    const { body } = render(AdminDashboard, { props: { data: okData() } });
    expect(body).toContain('metric-group');
    expect(body).toContain('今日新帖');
    expect(body).toContain('1,284');
    expect(body).toContain('运营趋势');
    expect(body).toContain('最近管理员操作');
    expect(body).toContain('admin-timeline');
    expect(body).toContain('关键入口');
    expect(body).toContain('生产健康提示');
    // 周期 Tab 链接式（无 JS 可切）
    expect(body).toMatch(/href="\/admin\?period=week"/);
    // 演示遗留「切换普通成员」已由「系统设置」替代
    expect(body).not.toContain('切换普通成员');
    expect(body).toMatch(/href="\/admin\/settings"/);
    // 待审举报 > 0 → warning 信号值 + 「需要处理」注记
    expect(body).toContain('metric__value--warning');
    expect(body).toContain('需要处理');
    // 时间链入口
    expect(body).toMatch(/href="\/admin\/audit"/);
  });

  it('已收录审计码显示产品文案；未收录回退 mono 原始码', () => {
    const mapped = render(AdminDashboard, { props: { data: okData() } });
    expect(mapped.body).toContain('积分调整');
    expect(mapped.body).not.toContain('>admin.points.adjust<');
    expect(mapped.body).toContain('附件就绪');
    const unknown = render(AdminDashboard, {
      props: {
        data: okData({
          stats: {
            ...STATS,
            recent_admin_actions: [{ action: 'future.unknown_code', actor_username: 'Chaos', created_at: NOW }]
          }
        })
      }
    });
    expect(unknown.body).toContain('future.unknown_code');
    expect(unknown.body).toContain('dash-code');
  });

  it('全零趋势与 trend=null 渲染可行动空态', () => {
    const empty = render(AdminDashboard, {
      props: { data: okData({ trend: { ...TREND, buckets: TREND.buckets.map((b) => ({ ...b, posts: 0, active_users: 0, reports: 0 })) } }) }
    });
    expect(empty.body).toContain('当前周期内暂无');
    const noTrend = render(AdminDashboard, { props: { data: okData({ trend: null }) } });
    expect(noTrend.body).toContain('趋势数据暂不可用');
  });

  it('forbidden 状态渲染无权限文案，不泄漏统计数值', () => {
    const { body } = render(AdminDashboard, {
      props: { data: { state: 'forbidden', stats: null, trend: null, period: 'day', error: 'forbidden' } }
    });
    expect(body).toContain('无权限');
    expect(body).not.toContain('1,284');
    expect(body).not.toContain('metric-group');
  });
});
