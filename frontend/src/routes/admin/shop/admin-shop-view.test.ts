import { describe, expect, it, afterEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/svelte';
import AdminShopPage from './+page.svelte';
import type { ShopProduct, CosmeticDef } from '$lib/api/types';

afterEach(() => {
  cleanup();
});

const mockCosmetics: CosmeticDef[] = [
  {
    id: 'frame_1',
    kind: 'avatar_frame',
    name: '雷霆动效头像框',
    style: { mode: 'steam_frame', image: 'frame.png' },
    status: 'active',
    updatedAt: 1726600000000
  },
  {
    id: 'bg_1',
    kind: 'profile_effect',
    name: '赛博夜景背景',
    style: { mode: 'profile', image: 'bg.jpg' },
    status: 'active',
    updatedAt: 1726600000000
  },
  {
    id: 'nick_1',
    kind: 'nickname_color',
    name: '极光渐变',
    style: { mode: 'gradient', colors: ['#ff0080', '#7928ca'] },
    status: 'active',
    updatedAt: 1726600000000
  }
];

const mockProducts: ShopProduct[] = [
  {
    id: 'p_frame_1',
    kind: 'cosmetic_avatar',
    status: 'published',
    slug: 'lightning-frame',
    title: '雷霆动效头像框',
    description_safe: '动效头像框',
    icon_token: 'award',
    slot: 'avatar_frame',
    currency_id: 'coin',
    currency_code: 'COIN',
    currency_name: '金币',
    unit_price: 150,
    quantity_limit: 1,
    stock_remaining: 99,
    required_level: 1,
    validity_seconds: null,
    refund_policy: 'non_refundable',
    version: 1,
    created_at: 1726600000000,
    updated_at: 1726600000000,
    purchasable: true,
    presentation_tokens: ['avatar.frame.frame_1']
  },
  {
    id: 'p_bg_1',
    kind: 'profile_effect',
    status: 'published',
    slug: 'cyber-night',
    title: '赛博夜景背景',
    description_safe: '个人资料动态背景',
    icon_token: 'sparkles',
    slot: 'profile_effect',
    currency_id: 'coin',
    currency_code: 'COIN',
    currency_name: '金币',
    unit_price: 300,
    quantity_limit: 1,
    stock_remaining: 50,
    required_level: 2,
    validity_seconds: null,
    refund_policy: 'non_refundable',
    version: 1,
    created_at: 1726600000000,
    updated_at: 1726600000000,
    purchasable: true,
    presentation_tokens: ['profile.effect.bg_1']
  },
  {
    id: 'p_nick_1',
    kind: 'cosmetic_nickname',
    status: 'disabled',
    slug: 'aurora-nick',
    title: '极光渐变',
    description_safe: '彩色渐变昵称',
    icon_token: 'wand-2',
    slot: 'nickname_color',
    currency_id: 'coin',
    currency_code: 'COIN',
    currency_name: '金币',
    unit_price: 200,
    quantity_limit: 1,
    stock_remaining: null,
    required_level: 1,
    validity_seconds: null,
    refund_policy: 'non_refundable',
    version: 2,
    created_at: 1726600000000,
    updated_at: 1726600000000,
    purchasable: false,
    presentation_tokens: ['nickname.color.nick_1']
  }
];

function createPageData() {
  return {
    config: {
      state: 'ok' as const,
      data: {
        enabled: true,
        currency_id: 'coin',
        default_refund_policy: 'non_refundable'
      }
    },
    products: {
      state: 'ok' as const,
      items: mockProducts
    },
    orders: {
      state: 'ok' as const,
      items: []
    },
    cosmetics: mockCosmetics
  };
}

describe('商城管理 卡片展示 / 列表展示 切换与分类 Tag 筛选', () => {
  it('渲染分类 Tag 标签栏与展示视图切换按钮', () => {
    const { container } = render(AdminShopPage, {
      props: { data: createPageData() as any, form: null }
    });

    // 视图切换按钮存在
    const viewButtons = container.querySelectorAll('.view-mode-btn');
    expect(viewButtons.length).toBe(2);
    expect(container.textContent).toContain('卡片展示');
    expect(container.textContent).toContain('列表展示');

    // 类型筛选 Tag 标签存在
    const tags = container.querySelectorAll('.shop-filter-tag');
    expect(tags.length).toBeGreaterThanOrEqual(4);
    expect(container.textContent).toContain('全部');
    expect(container.textContent).toContain('Steam 动效头像框');
    expect(container.textContent).toContain('个人资料背景');
    expect(container.textContent).toContain('彩色昵称');
  });

  it('默认卡片展示（grid 视图）下渲染商品卡片网格与卡片信息', () => {
    const { container } = render(AdminShopPage, {
      props: { data: createPageData() as any, form: null }
    });

    const activePanel = container.querySelector('.shop-tab-panel:not(.is-hidden)');
    expect(activePanel).toBeTruthy();

    const cardGrids = activePanel?.querySelectorAll('.shop-admin-card-grid');
    expect(cardGrids?.length).toBeGreaterThan(0);

    const cards = activePanel?.querySelectorAll('.shop-admin-card');
    expect(cards?.length).toBe(3); // 默认商品货架 Tab 下渲染 3 件商品

    // 检查卡片内信息
    expect(activePanel?.textContent).toContain('雷霆动效头像框');
    expect(activePanel?.textContent).toContain('赛博夜景背景');
    expect(activePanel?.textContent).toContain('极光渐变');
    expect(activePanel?.textContent).toContain('在售');
    expect(activePanel?.textContent).toContain('已停售');
  });

  it('点击「列表展示」切换为表格行列表展示', async () => {
    const { container } = render(AdminShopPage, {
      props: { data: createPageData() as any, form: null }
    });

    const listBtn = Array.from(container.querySelectorAll('.view-mode-btn')).find((b) =>
      b.textContent?.includes('列表展示')
    ) as HTMLButtonElement;
    expect(listBtn).toBeTruthy();

    await fireEvent.click(listBtn);

    const activePanel = container.querySelector('.shop-tab-panel:not(.is-hidden)');
    // 切换后渲染 post-row
    const postRows = activePanel?.querySelectorAll('.post-row');
    expect(postRows?.length).toBe(3); // 3 件商品行
    expect(activePanel?.querySelectorAll('.shop-admin-card-grid').length).toBe(0);

    // 再切回卡片展示
    const gridBtn = Array.from(container.querySelectorAll('.view-mode-btn')).find((b) =>
      b.textContent?.includes('卡片展示')
    ) as HTMLButtonElement;
    await fireEvent.click(gridBtn);
    expect(activePanel?.querySelectorAll('.shop-admin-card-grid').length).toBeGreaterThan(0);
  });

  it('点击主业务 Tab 可在商品货架、装扮样式库与订单记录间无冲突切换', async () => {
    const { container } = render(AdminShopPage, {
      props: { data: createPageData() as any, form: null }
    });

    const productPanel = container.querySelector('#panel-products');
    const cosmeticPanel = container.querySelector('#panel-cosmetics');
    const orderPanel = container.querySelector('#panel-orders');

    // 默认在「商品货架」激活，装扮样式库与订单被隐藏
    expect(productPanel?.classList.contains('is-hidden')).toBe(false);
    expect(cosmeticPanel?.classList.contains('is-hidden')).toBe(true);
    expect(orderPanel?.classList.contains('is-hidden')).toBe(true);
    expect(productPanel?.textContent).toContain('商品列表');

    // 切换到「装扮样式库」
    const cosmeticTab = Array.from(container.querySelectorAll('.shop-main-tabs .tab')).find((t) =>
      t.textContent?.includes('装扮样式库')
    ) as HTMLButtonElement;
    expect(cosmeticTab).toBeTruthy();
    await fireEvent.click(cosmeticTab);

    // 当前视图展示装扮样式库，商品货架被隐藏
    expect(productPanel?.classList.contains('is-hidden')).toBe(true);
    expect(cosmeticPanel?.classList.contains('is-hidden')).toBe(false);
    expect(orderPanel?.classList.contains('is-hidden')).toBe(true);
    expect(cosmeticPanel?.textContent).toContain('已生效装扮样式库');
    const cards = cosmeticPanel?.querySelectorAll('.shop-admin-card');
    expect(cards?.length).toBe(3); // 3 个样式定义

    // 切换到「订单记录」
    const orderTab = Array.from(container.querySelectorAll('.shop-main-tabs .tab')).find((t) =>
      t.textContent?.includes('订单记录')
    ) as HTMLButtonElement;
    expect(orderTab).toBeTruthy();
    await fireEvent.click(orderTab);

    expect(productPanel?.classList.contains('is-hidden')).toBe(true);
    expect(cosmeticPanel?.classList.contains('is-hidden')).toBe(true);
    expect(orderPanel?.classList.contains('is-hidden')).toBe(false);
    expect(orderPanel?.textContent).toContain('订单');
  });

  it('点击类型 Tag 切换可按类型精准过滤商品与样式', async () => {
    const { container } = render(AdminShopPage, {
      props: { data: createPageData() as any, form: null }
    });

    // 找到「Steam 动效头像框」Tag
    const avatarTag = Array.from(container.querySelectorAll('.shop-filter-tag')).find((t) =>
      t.textContent?.includes('Steam 动效头像框')
    ) as HTMLButtonElement;
    expect(avatarTag).toBeTruthy();

    await fireEvent.click(avatarTag);

    // 过滤后只显示头像框商品
    expect(container.textContent).toContain('雷霆动效头像框');
    expect(container.textContent).not.toContain('赛博夜景背景');
    expect(container.textContent).not.toContain('极光渐变');

    // 切换至「个人资料背景」
    const spaceTag = Array.from(container.querySelectorAll('.shop-filter-tag')).find((t) =>
      t.textContent?.includes('个人资料背景')
    ) as HTMLButtonElement;
    await fireEvent.click(spaceTag!);

    expect(container.textContent).toContain('赛博夜景背景');
    expect(container.textContent).not.toContain('雷霆动效头像框');

    // 切换回「全部」
    const allTag = Array.from(container.querySelectorAll('.shop-filter-tag')).find((t) =>
      t.textContent?.includes('全部')
    ) as HTMLButtonElement;
    await fireEvent.click(allTag!);

    expect(container.textContent).toContain('雷霆动效头像框');
    expect(container.textContent).toContain('赛博夜景背景');
    expect(container.textContent).toContain('极光渐变');
  });

  it('支持搜索输入实时过滤商品', async () => {
    const { container } = render(AdminShopPage, {
      props: { data: createPageData() as any, form: null }
    });

    const searchInput = container.querySelector('.shop-search-input') as HTMLInputElement;
    expect(searchInput).toBeTruthy();

    await fireEvent.input(searchInput, { target: { value: '赛博' } });

    expect(container.textContent).toContain('赛博夜景背景');
    expect(container.textContent).not.toContain('雷霆动效头像框');
  });
});
