// M18-ADMIN-POINTS-03 补证：签到页优化后 SSR 基线（运行概览 + 配置表单 + 规则摘要移交）。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import AdminActivityPage from '../../../routes/admin/activity/+page.svelte';
import type { AdminActivityPageData } from '../../../routes/admin/activity/+page.server';
import type { ActivityConfig, ActivityTask } from '$lib/api/types';

const config: ActivityConfig = {
  site_timezone: 'Asia/Shanghai',
  check_in_enabled: true,
  auto_check_in_enabled: true,
  day_reset_hour: 4,
  check_in_reward: { currency: 'coin', amount: 10 },
  check_in_daily_limit: 1,
  rewards_enabled: true,
  version: 5
};

const checkInRule: ActivityTask = {
  id: 'rule-checkin',
  kind: 'check_in',
  currency: 'coin',
  amount: 10,
  daily_limit: 1,
  cooldown_seconds: null,
  is_enabled: true,
  version: 3,
  updated_at: 1700000000000
};

const postRule: ActivityTask = {
  id: 'rule-post',
  kind: 'post',
  currency: 'coin',
  amount: 10,
  daily_limit: 5,
  cooldown_seconds: null,
  is_enabled: false,
  version: 1,
  updated_at: 1700000001000
};

const okData: AdminActivityPageData = {
  config: { state: 'ok', data: config },
  tasks: { state: 'ok', items: [checkInRule, postRule] }
};

describe('M07-UI-08 签到页优化 SSR', () => {
  it('运行概览：状态磁贴 + 奖励摘要 + 跨天/时区 + 快捷入口', () => {
    const { body } = render(AdminActivityPage, { props: { data: okData, form: null } });
    expect(body).toContain('运行概览');
    expect(body).toContain('签到功能运行中');
    expect(body).toContain('已开启');
    expect(body).toContain('发放中');
    expect(body).toContain('+10');
    expect(body).toContain('B币');
    expect(body).toContain('每日≤1 次');
    expect(body).toContain('跨天刷新 04:00');
    expect(body).toContain('href="/admin/points/rules"');
    expect(body).toContain('href="/admin/audit"');
  });

  it('签到配置 = 「全局配置」按钮 + 弹层（SSR 不渲染写表单），概览与版本号保留', () => {
    const { body } = render(AdminActivityPage, { props: { data: okData, form: null } });
    // 写操作入口 = 按钮；?/save-config 写表单收进弹层，不进 SSR HTML（约定 A）
    expect(body).toContain('全局配置');
    expect(body).not.toContain('action="?/save-config"');
    expect(body).not.toContain('name="reason"');
    // 配置版本号仍在只读概览中展示
    expect(body).toContain('配置 v5');
    // step-up / 审计提示保留在卡片说明里
    expect(body).toContain('写审计');
  });

  it('其他活跃规则：只读摘要计数 + 移交说明，不再有编辑/创建表单', () => {
    const { body } = render(AdminActivityPage, { props: { data: okData, form: null } });
    expect(body).toContain('其他活跃规则');
    expect(body).toContain('统一在积分规则配置页管理');
    expect(body).toContain('0/1');
    // 编辑面已移交：本页不应再有任务创建/更新表单
    expect(body).not.toContain('action="?/create-task"');
    expect(body).not.toContain('action="?/update-task"');
    // 签到规则行（check_in）不计入摘要卡种类
    expect(body).toContain('发帖奖励');
  });

  it('奖励总闸关闭 → 概览告警文案', () => {
    const off: AdminActivityPageData = {
      config: { state: 'ok', data: { ...config, rewards_enabled: false } },
      tasks: { state: 'ok', items: [] }
    };
    const { body } = render(AdminActivityPage, { props: { data: off, form: null } });
    expect(body).toContain('已暂停');
  });

  it('配置 403 → 错误提示，不渲染概览与写操作入口', () => {
    const forbidden: AdminActivityPageData = {
      config: { state: 'forbidden', message: '没有 activity.manage 权限' },
      tasks: { state: 'forbidden', message: '没有 activity.manage 权限' }
    };
    const { body } = render(AdminActivityPage, { props: { data: forbidden, form: null } });
    expect(body).toContain('没有 activity.manage 权限');
    expect(body).not.toContain('action="?/save-config"');
    expect(body).not.toContain('运行概览');
    expect(body).not.toContain('全局配置');
  });
});
