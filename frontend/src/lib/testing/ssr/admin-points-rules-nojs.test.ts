// M18-ADMIN-POINTS-03：积分规则配置页 SSR 快照（动作→奖励矩阵 + 无 JS 退化）。
// 约定 A 更新：既有规则行内编辑表单 → 行「编辑」按钮 + Dialog（?/update）；
// 「新增/一键补齐」→ 按钮 + Dialog（?/create，预填动作类型）。
// 约定 D 更新：规则行「编辑」按钮 → 「⋮」三点菜单（单项「编辑规则」，RowActionsMenu），
// 点菜单项打开既有编辑 Dialog；菜单关闭态只渲染触发按钮（行语义 aria-label）。
// 无 JS（SSR）基线：矩阵/规则行/总闸状态照常渲染；写表单收进弹层，不进 SSR HTML。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import AdminPointsRulesPage from '../../../routes/admin/points/rules/+page.svelte';
import type { AdminPointsRulesPageData } from '../../../routes/admin/points/rules/+page.server';
import type { ActivityTask } from '$lib/api/types';

const checkInRule: ActivityTask = {
  id: 'rule-checkin-1',
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
  id: 'rule-post-1',
  kind: 'post',
  currency: 'exp',
  amount: 10,
  daily_limit: 5,
  cooldown_seconds: 60,
  is_enabled: false,
  version: 1,
  updated_at: 1700000001000
};

const okData: AdminPointsRulesPageData = {
  rules: { state: 'ok', items: [checkInRule, postRule] },
  masterSwitch: { enabled: true, version: 5 },
  error: null
};

describe('M18-ADMIN-POINTS-03 积分规则配置 SSR', () => {
  it('矩阵渲染六类动作卡 + 规则行只读数据 + 总闸运行中', () => {
    const { body } = render(AdminPointsRulesPage, { props: { data: okData, form: null } });
    for (const label of ['每日签到', '发布主题', '发表回复', '内容表态', '自定义任务', '榜单奖励']) {
      expect(body).toContain(label);
    }
    // 既有规则行：数额/币种/每日上限/冷却/版本只读展示（数据仍在 SSR）
    expect(body).toContain('+10');
    expect(body).toContain('B币');
    expect(body).not.toContain('经验 EXP');
    expect(body).not.toContain('经验值');
    expect(body).toContain('每日上限：1');
    expect(body).toContain('每日上限：5');
    expect(body).toContain('冷却：60 秒');
    expect(body).toContain('v3');
    expect(body).toContain('v1');
    // 启停徽标
    expect(body).toContain('启用');
    expect(body).toContain('停用');
    // 总闸状态（rewards_enabled=true → 运行中文案，不出现关闭告警）
    expect(body).toContain('全站奖励发放运行中');
    expect(body).not.toContain('全站奖励发放已关闭');
  });

  it('写操作 = 三点菜单/按钮 + 弹层：规则行「⋮」菜单与「配置/追加规则」按钮渲染，写表单不进 SSR HTML', () => {
    const { body } = render(AdminPointsRulesPage, { props: { data: okData, form: null } });
    // 规则行触发入口（约定 D）：「⋮」三点菜单，行语义 aria-label
    expect(body).toContain('aria-label="更多操作：积分规则 每日签到"');
    expect(body).toContain('aria-label="更多操作：积分规则 发布主题"');
    // 动作卡头部的 create 入口按钮在 SSR 渲染
    expect(body).toContain('配置「自定义任务」奖励');
    expect(body).toContain('配置「榜单奖励」奖励');
    expect(body).toContain('追加规则');
    // 新契约：菜单关闭态不渲染操作列表（「编辑规则」项），旧「编辑」文字按钮不再出现
    expect(body).not.toContain('row-actions__menu');
    expect(body).not.toContain('>编辑</span>');
    expect(body).not.toContain('>编辑规则<');
    // 新契约：行内编辑/创建表单移入弹层，SSR 不再出现写表单与审计输入
    expect(body).not.toContain('action="?/update"');
    expect(body).not.toContain('action="?/create"');
    expect(body).not.toContain('name="reason"');
    expect(body).not.toContain('name="version"');
    expect(body).not.toContain('name="kind"');
  });

  it('未配置动作 → 「未配置」徽标 + 空态 + 预填入口按钮', () => {
    const { body } = render(AdminPointsRulesPage, { props: { data: okData, form: null } });
    expect(body).toContain('未配置');
    expect(body).toContain('尚未配置该动作的奖励');
    // 未配置种类卡给「配置「X」奖励」按钮（打开 create 弹层并预填 kind）
    expect(body).toContain('配置「自定义任务」奖励');
    expect(body).toContain('配置「内容表态」奖励');
  });

  it('奖励总闸关闭 → 告警文案 + 指向 /admin/activity', () => {
    const off: AdminPointsRulesPageData = { ...okData, masterSwitch: { enabled: false, version: 6 } };
    const { body } = render(AdminPointsRulesPage, { props: { data: off, form: null } });
    expect(body).toContain('全站奖励发放已关闭');
    expect(body).toContain('href="/admin/activity"');
  });

  it('总闸接口不可用 → 降级提示，不中断规则矩阵', () => {
    const degraded: AdminPointsRulesPageData = {
      ...okData,
      masterSwitch: { unavailable: true, message: '没有权限' }
    };
    const { body } = render(AdminPointsRulesPage, { props: { data: degraded, form: null } });
    expect(body).toContain('奖励总闸状态未知');
    expect(body).toContain('发布主题');
  });

  it('403 → 错误提示，不渲染矩阵与写操作入口', () => {
    const forbidden: AdminPointsRulesPageData = {
      rules: { state: 'forbidden', message: '没有 activity.manage 权限' },
      masterSwitch: null,
      error: null
    };
    const { body } = render(AdminPointsRulesPage, { props: { data: forbidden, form: null } });
    expect(body).toContain('没有 activity.manage 权限');
    expect(body).not.toContain('action="?/create"');
    expect(body).not.toContain('发布主题');
    expect(body).not.toContain('>编辑</span>');
    expect(body).not.toContain('更多操作：积分规则');
  });

  it('空规则 → 全部动作显示未配置态', () => {
    const empty: AdminPointsRulesPageData = {
      rules: { state: 'ok', items: [] },
      masterSwitch: { enabled: true, version: 1 },
      error: null
    };
    const { body } = render(AdminPointsRulesPage, { props: { data: empty, form: null } });
    expect(body).toContain('规则总数');
    expect(body.match(/未配置/g)?.length).toBeGreaterThanOrEqual(6);
  });
});
