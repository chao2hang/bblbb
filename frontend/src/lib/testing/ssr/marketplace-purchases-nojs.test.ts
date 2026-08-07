// M12-UI-04：用户 Purchase 查询页 SSR——只显示本人交易、退款状态展示、
// 敏感账务字段脱敏（不显示其他用户/商户数据）。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import PurchasesPage from '../../../routes/marketplace/purchases/+page.svelte';
import type { MarketplacePurchaseView } from '$lib/api/types';

function purchase(overrides: Partial<MarketplacePurchaseView> = {}): MarketplacePurchaseView {
  return {
    id: 'p-1',
    intent_id: 'i-1',
    client_id: 'client-1',
    user_id: 'u-1',
    offer_id: 'offer-1',
    offer_version: 2,
    quantity: 1,
    amount: 300,
    fee_amount: 3,
    merchant_net: 297,
    currency_id: 'coin',
    status: 'succeeded',
    refunded_amount: 0,
    merchant_order_id: 'ord-1',
    created_at: 1700000000000,
    updated_at: 1700000000000,
    refunds: [],
    ...overrides
  };
}

describe('M12-UI-04 购买记录页 SSR', () => {
  it('显示本人交易金额/状态/商户订单号', () => {
    const { body } = render(PurchasesPage, {
      props: { data: { purchases: [purchase()], error: null }, form: null }
    });
    expect(body).toContain('300');
    expect(body).toContain('COIN');
    expect(body).toContain('交易成功');
    expect(body).toContain('ord-1');
  });

  it('空列表 → 说明只有本人确认的交易会显示', () => {
    const { body } = render(PurchasesPage, {
      props: { data: { purchases: [], error: null }, form: null }
    });
    expect(body).toContain('暂无 Marketplace 购买记录');
  });

  it('已退款 → 显示退款状态与金额；requested 显示待处理说明', () => {
    const { body } = render(PurchasesPage, {
      props: {
        data: {
          purchases: [
            purchase({
              status: 'refunded',
              refunded_amount: 300,
              refunds: [{ id: 'r-1', purchase_id: 'p-1', client_id: 'client-1', amount: 300, status: 'processed', reason_code: 'customer_request', merchant_refund_id: 'mr-1', refunded_by: 'client-1', refunded_by_type: 'client', created_at: 0 }]
            }),
            purchase({
              id: 'p-2',
              merchant_order_id: 'ord-2',
              refunds: [{ id: 'r-2', purchase_id: 'p-2', client_id: 'client-1', amount: 50, status: 'requested', reason_code: 'merchant', merchant_refund_id: 'mr-2', refunded_by: 'client-1', refunded_by_type: 'client', created_at: 0 }]
            })
          ],
          error: null
        },
        form: null
      }
    });
    expect(body).toContain('已退款');
    expect(body).toContain('待商户资金到位后由平台处理');
  });

  it('敏感字段脱敏：不渲染其他用户/商户内部标识之外的数据', () => {
    const { body } = render(PurchasesPage, {
      props: { data: { purchases: [purchase({ user_id: 'u-secret', client_id: 'c-secret' })], error: null }, form: null }
    });
    // 不显示内部 user_id / client 内部 id（只显示商户订单号与金额快照）。
    expect(body).not.toContain('u-secret');
    expect(body).not.toContain('c-secret');
    expect(body).not.toContain('fee_amount');
    expect(body).not.toContain('merchant_net');
  });
});
