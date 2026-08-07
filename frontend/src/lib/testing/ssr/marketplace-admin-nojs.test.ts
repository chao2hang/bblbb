// M12-UI-05/06：管理员 Marketplace 控制台 SSR——Client/Scope/限额/余额/
// Webhook/对账/紧急停用表单、高风险操作 reason 必填、权限越界（无权限态）、
// 敏感账务字段脱敏。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import AdminMarketplacePage from '../../../routes/admin/marketplace/+page.svelte';
import type { MarketplaceClientView } from '$lib/api/types';

function client(overrides: Partial<MarketplaceClientView> = {}): MarketplaceClientView {
  return {
    id: 'mc-1',
    client_id: 'oauth-client-1',
    owner_user_id: 'owner-1',
    name: '测试商户',
    status: 'active',
    terms_url: 'https://merchant.example/terms',
    privacy_url: 'https://merchant.example/privacy',
    webhook_url: 'https://merchant.example/hook',
    webhook_secret_version: 3,
    redirect_uris: ['https://merchant.example/cb'],
    fee_bps: 100,
    version: 4,
    approval_history: [{ action: 'approve', at: 0 }],
    created_at: 0,
    updated_at: 0,
    scopes: [
      { scope: 'marketplace.checkout.create', status: 'approved', limits: { max_amount_per_transaction: 5000 }, version: 1, effective_at: 0 },
      { scope: 'marketplace.refund', status: 'approved', limits: {}, version: 1, effective_at: 0 }
    ],
    balance: { client_id: 'mc-1', currency_id: 'coin', available_balance: 100, pending_balance: 200, frozen_balance: 0, total: 300, status: 'active', version: 1 },
    ...overrides
  };
}

const data = {
  clients: { state: 'ok' as const, items: [client()] },
  offers: { state: 'ok' as const, items: [] },
  deliveries: { state: 'ok' as const, items: [] },
  balances: []
};

describe('M12-UI-05 管理控制台 SSR', () => {
  it('显示 Client、Scope 审批、限额、余额与紧急停用入口', () => {
    const { body } = render(AdminMarketplacePage, { props: { data, form: null } });
    expect(body).toContain('测试商户');
    expect(body).toContain('marketplace.checkout.create');
    expect(body).toContain('已批准');
    expect(body).toContain('紧急停用');
    expect(body).toContain('100'); // 可用余额
    expect(body).toContain('200'); // 待结算
    expect(body).toContain('轮换 Webhook Secret');
  });

  it('Client 表单带 If-Match 版本与必填 reason', () => {
    const { body } = render(AdminMarketplacePage, { props: { data, form: null } });
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/upsertClient"/);
    expect(body).toContain('name="version"');
    expect(body).toContain('value="4"');
    expect(body).toContain('操作原因（必填）');
    expect(body).toContain('name="reason"');
  });

  it('权限越界 → 显示无权限态', () => {
    const { body } = render(AdminMarketplacePage, {
      props: { data: { ...data, clients: { state: 'forbidden', message: '该操作仅限管理员' } }, form: null }
    });
    expect(body).toContain('无权限');
    expect(body).toContain('该操作仅限管理员');
  });

  it('敏感字段脱敏：不渲染 webhook_secret_hash / client_secret', () => {
    const { body } = render(AdminMarketplacePage, { props: { data, form: null } });
    expect(body).not.toContain('webhook_secret_hash');
    expect(body).not.toContain('client_secret');
  });
});

describe('M12-UI-06 高风险操作 SSR', () => {
  it('紧急停用/对账/退款重试表单均含 reason 输入', () => {
    const { body } = render(AdminMarketplacePage, { props: { data, form: null } });
    expect(body).toContain('紧急停用');
    expect(body).toContain('对账');
    // 对账表单带 client_id 与原因。
    expect(body).toContain('name="after_cursor"');
    expect(body).toContain('name="reason"');
  });

  it('secret 轮换成功 → 一次性显示新 Secret', () => {
    const { body } = render(AdminMarketplacePage, {
      props: { data, form: { message: 'Webhook Secret 已轮换', secret: 's3cr3t-value' } }
    });
    expect(body).toContain('仅显示一次');
    expect(body).toContain('s3cr3t-value');
  });

  it('版本冲突 → 显示冲突提示', () => {
    const { body } = render(AdminMarketplacePage, {
      props: { data, form: { message: '版本冲突：resource version conflict', code: 'version_conflict' } }
    });
    expect(body).toContain('版本冲突');
  });
});
