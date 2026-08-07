// M07-UI-03/04：购买确认页与订单结果页 SSR——准确价格、余额变化、不可退款
// 说明、失败恢复（版本冲突）与订单状态（entitlement/补偿待处理）。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import ShopProductPage from '../../../routes/shop/[id]/+page.svelte';
import ShopOrderPage from '../../../routes/shop/orders/[id]/+page.svelte';
import type { ShopOrder, ShopProduct } from '$lib/api/types';

const product: ShopProduct = {
  id: 'p1',
  kind: 'cosmetic_nickname',
  status: 'published',
  slug: 'blue-name',
  title: '蓝色昵称',
  description_safe: '昵称显示为蓝色',
  icon_token: 'star',
  slot: 'nickname_color',
  currency: 'coin',
  unit_price: 50,
  quantity_limit: 1,
  stock_remaining: 10,
  required_level: 1,
  validity_seconds: null,
  refund_policy: 'non_refundable',
  version: 3,
  created_at: 0,
  updated_at: 0,
  purchasable: true
};

function confirmData(overrides: Partial<{ balance: number; level: number; ownedCount: number; product: ShopProduct | null; error: string | null }> = {}) {
  const balance = overrides.balance ?? 200;
  return {
    product: overrides.product ?? product,
    balance: { currency: 'coin', amount: balance },
    level: overrides.level ?? 5,
    ownedCount: overrides.ownedCount ?? 0,
    error: overrides.error ?? null
  };
}

describe('M07-UI-03 购买确认页 SSR', () => {
  it('显示准确价格、当前余额、购买后余额与不可退款说明', () => {
    const { body } = render(ShopProductPage, {
      props: { data: confirmData(), form: null }
    });
    expect(body).toContain('蓝色昵称');
    expect(body).toContain('50');
    expect(body).toContain('COIN');
    expect(body).toContain('200'); // 当前余额
    expect(body).toContain('150'); // 200 - 50 = 购买后余额
    expect(body).toContain('数字装扮默认不可退款');
    expect(body).toContain('确认购买');
  });

  it('余额不足 → 购买按钮禁用且文案为“余额不足”，不显示可提交按钮', () => {
    const { body } = render(ShopProductPage, {
      props: { data: confirmData({ balance: 10 }), form: null }
    });
    expect(body).toContain('余额不足');
    expect(body).not.toContain('确认购买');
  });

  it('等级门槛未达 → 锁定提示，购买表单不渲染', () => {
    const { body } = render(ShopProductPage, {
      props: { data: confirmData({ level: 2, product: { ...product, required_level: 3 } }), form: null }
    });
    expect(body).toContain('需要 LV.3');
    expect(body).toContain('你的当前等级是 LV.2');
    expect(body).not.toContain('确认购买');
  });

  it('售罄 → 售罄提示，不渲染购买表单', () => {
    const { body } = render(ShopProductPage, {
      props: { data: confirmData({ product: { ...product, stock_remaining: 0 } }), form: null }
    });
    expect(body).toContain('已售罄');
    expect(body).not.toContain('确认购买');
  });

  it('限购已满 → 提示达到购买上限', () => {
    const { body } = render(ShopProductPage, {
      props: { data: confirmData({ ownedCount: 1, product: { ...product, quantity_limit: 1 } }), form: null }
    });
    expect(body).toContain('已达到该商品购买上限');
  });

  it('购买表单为原生 form[method=POST] 且携带稳定幂等键（重试不重复扣款）', () => {
    const { body } = render(ShopProductPage, {
      props: { data: confirmData(), form: null }
    });
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/purchase"/);
    expect(body).toContain('name="client_request_id"');
    expect(body).toContain('name="expected_product_version"');
    expect(body).toContain('value="3"'); // product.version
  });

  it('action 失败 → 显示错误信息与恢复指引（版本冲突）', () => {
    const { body } = render(ShopProductPage, {
      props: {
        data: confirmData(),
        form: { message: '商品信息已更新，请重新确认', code: 'product_version_changed', requestId: 'r1' }
      }
    });
    expect(body).toContain('商品信息已更新');
    expect(body).toContain('刷新页面后重新确认商品信息');
  });
});

describe('M07-UI-04 订单结果页 SSR', () => {
  const order: ShopOrder = {
    id: 'o1',
    product_id: 'p1',
    product_version: 3,
    product_title: '蓝色昵称',
    quantity: 1,
    currency: 'coin',
    unit_price: 50,
    total_amount: 50,
    status: 'succeeded',
    entitlement_id: 'e1',
    created_at: 1700000000000,
    updated_at: 1700000000000
  };

  it('成功订单 → 显示金额快照与权益发放入口', () => {
    const { body } = render(ShopOrderPage, {
      props: { data: { order, balance: null, error: null } }
    });
    expect(body).toContain('交易成功');
    expect(body).toContain('50');
    expect(body).toContain('COIN');
    expect(body).toContain('权益已发放');
    expect(body).toContain('/me/wardrobe');
  });

  it('补偿待处理 → 显示处理中提示且说明重复提交不重复扣款', () => {
    const { body } = render(ShopOrderPage, {
      props: { data: { order: { ...order, entitlement_id: null, entitlement_status: 'pending' }, balance: null, error: null } }
    });
    expect(body).toContain('权益正在发放中');
    expect(body).toContain('重复提交不会重复扣款');
  });

  it('已退款订单 → 显示退款状态', () => {
    const { body } = render(ShopOrderPage, {
      props: { data: { order: { ...order, status: 'refunded' }, balance: null, error: null } }
    });
    expect(body).toContain('已退款');
  });

  it('错误 → 显示错误横幅，不渲染订单信息', () => {
    const { body } = render(ShopOrderPage, {
      props: { data: { order: null, balance: null, error: '订单不存在或无权查看' } }
    });
    expect(body).toContain('订单不存在');
    expect(body).not.toContain('交易成功');
  });
});
