// M07-UI-01：余额/等级/经验/签到 SSR——安全投影、签到按钮（原生 form + 幂等键）、
// 已签到态与 429 冷却提示。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import BalancePage from '../../../routes/me/balance/+page.svelte';

const summary = {
  level: 7,
  level_name: '资深成员',
  xp: 320,
  xp_to_next: 180,
  checked_in_today: true,
  streak_days: 4,
  today_earned: [{ currency: 'coin', amount: 10 }, { currency: 'exp', amount: 20 }],
  balances: [
    { currency: 'coin', amount: 1500 },
    { currency: 'exp', amount: 320 }
  ]
};

describe('M07-UI-01 积分页 SSR', () => {
  it('渲染余额/等级/经验/连续签到与今日奖励（安全投影）', () => {
    const { body } = render(BalancePage, {
      props: { data: { summary, error: null }, form: null }
    });
    expect(body).toContain('LV.7');
    expect(body).toContain('资深成员');
    expect(body).toContain('1500');
    expect(body).toContain('COIN');
    expect(body).toContain('+10');
    expect(body).toContain('连续签到 4 天');
  });

  it('已签到 → 按钮禁用且文案“今日已签到”', () => {
    const { body } = render(BalancePage, {
      props: { data: { summary, error: null }, form: null }
    });
    expect(body).toContain('今日已签到');
    expect(body).toContain('disabled');
  });

  it('未签到 → 原生 form ?/visit + 隐藏幂等键 + 立即签到按钮', () => {
    const { body } = render(BalancePage, {
      props: { data: { summary: { ...summary, checked_in_today: false }, error: null }, form: null }
    });
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/visit"/);
    expect(body).toContain('name="client_request_id"');
    expect(body).toContain('立即签到');
    expect(body).toContain('今日未签到');
  });

  it('429 → 显示冷却提示', () => {
    const { body } = render(BalancePage, {
      props: { data: { summary: { ...summary, checked_in_today: false }, error: null }, form: { message: 'x', retryAfterSecs: 60 } }
    });
    expect(body).toContain('约 60 秒');
  });

  it('load 错误 → 错误横幅，不渲染余额', () => {
    const { body } = render(BalancePage, {
      props: { data: { summary: null, error: '服务暂不可用' }, form: null }
    });
    expect(body).toContain('服务暂不可用');
    expect(body).not.toContain('B 币');
  });
});
