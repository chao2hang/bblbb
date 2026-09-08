// M13-THEME：主题页面结构预设（layout.mode）SSR 投影测试。
//
// 根布局把服务端解析的结构预设写入 .app-shell[data-theme-layout]：
// - SSR 首帧即带结构属性（无闪烁，无 JS 也有正确结构语义）；
// - 合法预设直通（classic/sidebar/wide）；缺失/非法/危险值回退 classic；
// - 对抗性 token 值不进入任何 HTML 属性（闭集防御）。
import { describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import { createRawSnippet } from 'svelte';
import RootLayout from '../../../routes/+layout.svelte';
import type { ActiveThemeView } from '$lib/theme/projection';

// Navbar 的 isActive 读取 $app/state page.url.pathname；根布局渲染需要假 page。
vi.mock('$app/state', () => ({
  page: { url: { pathname: '/', searchParams: new URLSearchParams() }, data: {} }
}));

const children = createRawSnippet(() => ({ render: () => '<div id="main-content" />' }));

function renderShell(activeTheme: ActiveThemeView | null): string {
  return render(RootLayout, {
    props: {
      children,
      data: { user: null, notifications: { unreadCount: 0, recent: [] }, activeTheme }
    }
  }).body;
}

describe('M13-THEME 主题结构预设 SSR 投影', () => {
  it('声明 sidebar 的活跃主题 → 首帧输出 data-theme-layout="sidebar"', () => {
    const body = renderShell({
      name: 'chinese-elegance',
      revision: 3,
      source: 'site_default',
      tokens: { 'layout.mode': 'sidebar' }
    });
    expect(body).toContain('class="app-shell"');
    expect(body).toContain('data-theme-layout="sidebar"');
  });

  it('声明 wide 的活跃主题 → 首帧输出 data-theme-layout="wide"', () => {
    const body = renderShell({
      name: 'paper',
      revision: 1,
      source: 'site_default',
      tokens: { 'layout.mode': 'wide' }
    });
    expect(body).toContain('data-theme-layout="wide"');
  });

  it('未声明 layout.mode 的旧主题 → 回退 classic', () => {
    const body = renderShell({
      name: 'legacy-v1',
      revision: 1,
      source: 'site_default',
      tokens: { 'color.background': '#0f172a' }
    });
    expect(body).toContain('data-theme-layout="classic"');
  });

  it('后端不可达（activeTheme=null）→ classic 兜底', () => {
    const body = renderShell(null);
    expect(body).toContain('data-theme-layout="classic"');
  });

  it('对抗性结构值（未注册预设/注入尝试）不进入 HTML 属性', () => {
    const body = renderShell({
      name: 'evil-theme',
      revision: 1,
      source: 'site_default',
      tokens: {
        'layout.mode': '"><script>alert(1)</script>',
        secret: 'LEAK-LAYOUT-SSR'
      }
    });
    // 非法值被闭集回退为 classic，绝不原样进入属性
    expect(body).toContain('data-theme-layout="classic"');
    expect(body).not.toContain('alert(1)');
    expect(body).not.toContain('LEAK-LAYOUT-SSR');
  });

  it('主题声明的其他合法 token 不破坏结构属性（共存）', () => {
    const body = renderShell({
      name: 'midnight',
      revision: 2,
      source: 'site_default',
      tokens: {
        'color.background': '#0f172a',
        'layout.mode': 'wide'
      }
    });
    expect(body).toContain('data-theme-layout="wide"');
  });
});
