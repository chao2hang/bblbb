// M12-UI-01/02/03：托管 Checkout 确认页 SSR——准确金额/余额变化/Scope/
// 授权期限、无 JS 原生表单（不含可篡改价格/用户/余额字段）、成功/失败/
// 处理中/重复/过期/request ID 状态。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import CheckoutPage from '../../../routes/marketplace/checkout/[id]/+page.svelte';
import type { MarketplaceCheckoutView } from '$lib/api/types';

function checkoutView(overrides: Partial<MarketplaceCheckoutView> = {}): MarketplaceCheckoutView {
  return {
    intent_id: 'int-1',
    interaction_id: 'int-1',
    version: 1,
    client_id: 'client-1',
    merchant_name: '测试商户',
    terms_url: 'https://merchant.example/terms',
    privacy_url: 'https://merchant.example/privacy',
    offer_id: 'offer-1',
    offer_title: '会员礼包',
    offer_description: '月度会员权益',
    offer_version: 2,
    quantity: 1,
    amount: 500,
    currency_id: 'coin',
    fee_bps: 100,
    fee_refundable: true,
    scopes: ['marketplace.checkout.create', 'marketplace.purchase'],
    balance: 1000,
    frozen_balance: 0,
    balance_after: 500,
    expires_at: Date.now() + 4 * 60 * 1000,
    status: 'pending',
    created_at: Date.now(),
    ...overrides
  };
}

describe('M12-UI-01/02 托管确认页 SSR', () => {
  it('显示商户、商品、数量、准确金额、余额变化、Scope 与授权期限', () => {
    const { body } = render(CheckoutPage, {
      props: { data: { checkout: checkoutView(), error: null }, form: null }
    });
    expect(body).toContain('测试商户');
    expect(body).toContain('会员礼包');
    expect(body).toContain('500');
    expect(body).toContain('COIN');
    expect(body).toContain('1000'); // 当前余额
    expect(body).toContain('500'); // 扣款后余额（balance_after）
    expect(body).toContain('marketplace.checkout.create');
    expect(body).toContain('marketplace.purchase');
    expect(body).toContain('剩余约');
  });

  it('余额不足 → 按钮禁用且文案为余额不足，不渲染可提交确认', () => {
    const { body } = render(CheckoutPage, {
      props: { data: { checkout: checkoutView({ balance: 100, balance_after: -400 }), error: null }, form: null }
    });
    expect(body).toContain('余额不足');
    expect(body).not.toContain('name="decision"');
  });

  it('确认表单是原生 form[method=POST] 且不包含可篡改的价格/用户/余额隐藏字段', () => {
    const { body } = render(CheckoutPage, {
      props: { data: { checkout: checkoutView(), error: null }, form: null }
    });
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/confirm"/);
    // 隐藏字段只含幂等键与乐观锁版本，绝不包含 amount/user/balance/currency。
    expect(body).toContain('name="client_request_id"');
    expect(body).toContain('name="expected_intent_version"');
    expect(body).not.toContain('name="amount"');
    expect(body).not.toContain('name="user_id"');
    expect(body).not.toContain('name="balance"');
    expect(body).not.toContain('name="currency_id"');
    // 有取消（deny）表单（无 JS 可用）。
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/deny"/);
  });

  it('失败 → 显示错误信息与 request ID，并给出不重复扣款提示', () => {
    const { body } = render(CheckoutPage, {
      props: {
        data: { checkout: checkoutView(), error: null },
        form: { message: 'insufficient funds', code: 'insufficient_funds', requestId: 'req-1' }
      }
    });
    expect(body).toContain('insufficient funds');
    expect(body).toContain('req-1');
    expect(body).toContain('余额不足');
    expect(body).toContain('本次未扣款');
  });

  it('处理中 → 提示请勿重复提交', () => {
    const { body } = render(CheckoutPage, {
      props: {
        data: { checkout: checkoutView(), error: null },
        form: { message: 'request already in progress', code: 'invalid_request' }
      }
    });
    expect(body).toContain('请求处理中');
    expect(body).toContain('重复提交不会重复扣款');
  });

  it('成功 → 展示金额与购买 ID（succeeded 状态）', () => {
    const { body } = render(CheckoutPage, {
      props: {
        data: { checkout: checkoutView(), error: null },
        form: {
          ok: true,
          purchase: {
            id: 'purchase-1',
            intent_id: 'int-1',
            client_id: 'client-1',
            user_id: 'u1',
            offer_id: 'offer-1',
            offer_version: 2,
            quantity: 1,
            amount: 500,
            fee_amount: 5,
            merchant_net: 495,
            currency_id: 'coin',
            status: 'succeeded',
            refunded_amount: 0,
            merchant_order_id: 'ord-1',
            created_at: 0,
            updated_at: 0
          }
        }
      }
    });
    expect(body).toContain('交易成功');
    expect(body).toContain('purchase-1');
    expect(body).toContain('succeeded');
  });
});

describe('M12-UI-03 加载错误状态 SSR', () => {
  it('过期 Intent → 显示过期提示', () => {
    const { body } = render(CheckoutPage, {
      props: {
        data: { checkout: null, error: { code: 'checkout_intent_expired', message: 'checkout intent expired' } },
        form: null
      }
    });
    expect(body).toContain('授权已过期');
    expect(body).toContain('重新发起购买');
  });

  it('已消费 Intent → 提示查询原 Purchase', () => {
    const { body } = render(CheckoutPage, {
      props: {
        data: { checkout: null, error: { code: 'checkout_intent_consumed', message: 'checkout intent already consumed' } },
        form: null
      }
    });
    expect(body).toContain('该请求已完成');
  });

  it('用户不一致 → 提示使用原账号', () => {
    const { body } = render(CheckoutPage, {
      props: {
        data: { checkout: null, error: { code: 'checkout_user_mismatch', message: 'mismatch' } },
        form: null
      }
    });
    expect(body).toContain('登录账号与发起购买时不一致');
  });
});
