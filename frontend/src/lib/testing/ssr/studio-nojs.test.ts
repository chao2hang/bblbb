// M07-SHOP-STUDIO-UX：装扮工作台精简改版为直接选品定价上架
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import StudioPage from '../../../routes/admin/shop/studio/+page.svelte';

describe('/admin/shop/studio 简装重定向壳', () => {
  it('SSR 渲染重定向至商城管理页面的跳转提示', () => {
    const { body } = render(StudioPage);
    expect(body).toContain('商城管理');
    expect(body).toContain('进入商城管理');
  });
});
