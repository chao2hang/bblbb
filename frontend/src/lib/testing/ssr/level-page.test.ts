import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { LINUXDO_TRUST_LEVELS } from '$lib/api/types';
import LevelPage from '../../../routes/me/level/+page.svelte';
import BalancePage from '../../../routes/me/balance/+page.svelte';

describe('全站统一社区信任等级体系（TL0–TL4）', () => {
  describe('LINUXDO_TRUST_LEVELS 5 级信任标准元数据', () => {
    it('包含且仅包含 5 个等级：TL0 新用户 至 TL4 领导者', () => {
      expect(LINUXDO_TRUST_LEVELS).toHaveLength(5);
      expect(LINUXDO_TRUST_LEVELS.map((t) => t.code)).toEqual(['TL0', 'TL1', 'TL2', 'TL3', 'TL4']);
      expect(LINUXDO_TRUST_LEVELS.map((t) => t.name)).toEqual([
        '新用户',
        '基本用户',
        '成员',
        '活跃用户',
        '领导者'
      ]);
    });

    it('TL4 明确规定为人工审核授予，TL3 具备滚动考核说明', () => {
      const tl4 = LINUXDO_TRUST_LEVELS[4];
      expect(tl4.promotion).toContain('工作人员人工审核手动授予');
      expect(tl4.perks).toContain('置顶/关闭/归档社区话题特权');

      const tl3 = LINUXDO_TRUST_LEVELS[3];
      expect(tl3.summary).toContain('滚动窗口考核');
      expect(tl3.promotion).toContain('100天滚动窗口');
    });
  });

  describe('独立等级页 /me/level SSR 渲染', () => {
    it('正确渲染社区信任等级中心与 TL0–TL4 阶梯表', () => {
      const trust = {
        level: 1,
        name: '基本用户',
        summary: '愿意阅读即可达到的正式用户；解除新用户发帖频次限制与编辑时间限制。',
        updated_at: 1722816000000,
        grace_until: null,
        window: null,
        next_level: {
          level: 2,
          name: '成员',
          summary: '持续活跃并积极参与讨论的社区成员。',
          manual_only: false,
          eligible: false,
          requirements: [
            {
              key: 'days_visited',
              label: '访问天数',
              current: 6,
              required: 15,
              met: false
            },
            {
              key: 'posts_read',
              label: '阅读楼层数',
              current: 100,
              required: 100,
              met: true
            }
          ]
        }
      };

      const { body } = render(LevelPage, {
        props: {
          data: {
            trust,
            error: null
          }
        }
      });

      // 验证标题与信任体系说明
      expect(body).toContain('社区信任等级');
      expect(body).toContain('社区行为标准');
      expect(body).toContain('TL1 · 基本用户');
      expect(body).toContain('当前生效');

      // 验证下一级晋升要求卡片
      expect(body).toContain('升至 TL2（成员） 晋升要求');
      expect(body).toContain('访问天数');
      expect(body).toContain('阅读楼层数');
      expect(body).toContain('已达标');
      expect(body).toContain('未达标');

      // 验证 Tab 分页导航设计（解决长页面堆叠）
      expect(body).toContain('我的进度与下一级要求');
      expect(body).toContain('全站信任等级阶梯表');
      expect(body).toContain('信任体系规则解读');

      // 验证当前等级特权卡片
      expect(body).toContain('TL1 · 基本用户 享有的核心特权');
      expect(body).toContain('生效中');
    });

    it('缺省信任数据时安全降级为 TL0 新用户兜底', () => {
      const { body } = render(LevelPage, {
        props: {
          data: {
            trust: null,
            error: null
          }
        }
      });

      expect(body).toContain('社区信任等级');
      expect(body).toContain('TL0 · 新用户');
      expect(body).toContain('全站信任等级阶梯表');
    });
  });

  describe('积分页 /me/balance 与信任等级对齐', () => {
    it('积分页展示社区信任等级入口，不再渲染混淆的 10 级经验阶梯', () => {
      const summary = {
        level: { level_id: 'tl1', name: '基本用户', sort_order: 1 },
        checked_in_today: true,
        streak_days: 6,
        balances: [{ currency: 'coin', amount: 50 }]
      };

      const { body } = render(BalancePage, {
        props: {
          data: { summary, error: null },
          form: null
        }
      });

      expect(body).toContain('社区信任等级');
      expect(body).toContain('行为信任标准体系');
      expect(body).toContain('/me/level');
      expect(body).not.toContain('全站经验等级阶梯表');
      expect(body).not.toContain('还需 38 经验升至 LV.2');
    });
  });
});
