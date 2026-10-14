import { describe, expect, it, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import SettingsNav from './SettingsNav.svelte';

describe('SettingsNav', () => {
  it('链接模式：默认渲染 7 个导航项链接，active 项高亮', () => {
    const { container } = render(SettingsNav, { props: { active: 'devices' } });
    const nav = container.querySelector('nav.app-settings-nav');
    expect(nav).not.toBeNull();

    const links = nav!.querySelectorAll('a');
    expect(links.length).toBe(7);

    const labels = Array.from(links).map((a) => a.textContent?.trim());
    expect(labels).toEqual([
      '个人资料',
      '外观与主题',
      '账号安全',
      '登录设备',
      '通知设置',
      'OAuth 授权',
      '隐私设置'
    ]);

    const activeLink = nav!.querySelector('a.is-active');
    expect(activeLink).not.toBeNull();
    expect(activeLink?.textContent?.trim()).toBe('登录设备');
    expect(activeLink?.getAttribute('href')).toBe('/me/security');
  });

  it('标签页模式：当传入 onSelectTab 时，同页 tab 渲染为 button，外部路由渲染为 a', async () => {
    const onSelectTab = vi.fn();
    const { container } = render(SettingsNav, {
      props: { active: 'profile', onSelectTab }
    });

    const tablist = container.querySelector('div.app-settings-nav[role="tablist"]');
    expect(tablist).not.toBeNull();

    const buttons = tablist!.querySelectorAll('button[role="tab"]');
    expect(buttons.length).toBe(5); // profile, appearance, security, notifications, oauth

    const links = tablist!.querySelectorAll('a');
    expect(links.length).toBe(2); // devices, privacy

    const profileBtn = buttons[0];
    expect(profileBtn.classList.contains('is-active')).toBe(true);
    expect(profileBtn.getAttribute('aria-selected')).toBe('true');

    await fireEvent.click(buttons[1]); // appearance
    expect(onSelectTab).toHaveBeenCalledWith('appearance');
  });

  it('移动端支持 Flarum/NoPJ 样式的顶栏下拉选择器', async () => {
    const onSelectTab = vi.fn();
    const { container } = render(SettingsNav, {
      props: { active: 'appearance', onSelectTab }
    });

    const titleBtn = container.querySelector('.settings-title-btn') as HTMLButtonElement | null;
    expect(titleBtn).not.toBeNull();
    expect(titleBtn?.textContent?.trim()).toContain('外观与主题');

    // 点击展开下拉
    await fireEvent.click(titleBtn!);
    const dropdownMenu = container.querySelector('.settings-dropdown-menu');
    expect(dropdownMenu).not.toBeNull();

    // 选中个人资料
    const profileItem = dropdownMenu?.querySelector('button') as HTMLButtonElement | null;
    expect(profileItem).not.toBeNull();
    await fireEvent.click(profileItem!);
    expect(onSelectTab).toHaveBeenCalledWith('profile');
  });
});
