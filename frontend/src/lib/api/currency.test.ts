// 货币展示标签单元测试：商城卡片曾直接渲染 currency_id（UUID 形态）导致
// 价格行出现整段 UUID 并换行撑破布局（M07-UI-02 布局回归）。
// 语义（与 client.ts 一致）：name（库内中文名）→ coin 代号映射 →
// 短 code 大写；未知 UUID 形态的悬空引用不原样外露（返回空串）。
import { describe, expect, it } from 'vitest';
import { currencyLabel, formatMoney } from './client';

describe('currencyLabel（货币展示标签）', () => {
  it('优先 currency_name（库内中文名）', () => {
    expect(currencyLabel({ id: '01911fd5-0047-0000-0000-000000000002', code: 'coin', name: '金币' })).toBe('金币');
  });

  it('缺 name 时用已知 coin 代号或短 code 大写', () => {
    expect(currencyLabel({ id: '01911fd5-0047-0000-0000-000000000002', code: 'coin' })).toBe('COIN');
    expect(currencyLabel({ code: 'gems' })).toBe('GEMS');
  });

  it('只有 id：已知 coin 映射命中，未知 UUID 形态返回空串', () => {
    expect(currencyLabel({ id: 'coin' })).toBe('COIN');
    expect(currencyLabel({ id: '01911fd5-0047-0000-0000-000000000001' })).toBe('');
    expect(currencyLabel({ id: '098765ab-12cd-0000-0000-000000000099' })).toBe('');
  });

  it('字符串入参与空输入', () => {
    expect(currencyLabel('coin')).toBe('COIN');
    expect(currencyLabel(null)).toBe('');
    expect(currencyLabel({})).toBe('');
  });
});

describe('formatMoney（金额 + 货币标签）', () => {
  it('有标签 → “100 COIN”', () => {
    expect(formatMoney(100, { code: 'coin' })).toBe('100 COIN');
  });

  it('free 选项：0 金额渲染“免费”', () => {
    expect(formatMoney(0, { code: 'coin' }, { free: true })).toBe('免费');
  });

  it('未知 UUID 引用 → 只渲染金额，不留悬空空格', () => {
    expect(formatMoney(0, { id: '098765ab-12cd-0000-0000-000000000099' })).toBe('0');
  });
});
