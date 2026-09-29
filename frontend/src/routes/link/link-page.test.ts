import { describe, expect, it, afterEach } from 'vitest';
import { render, cleanup } from '@testing-library/svelte';
import LinkPage from './+page.svelte';
import { load } from './+page.server';

afterEach(() => {
  cleanup();
});

describe('LinkPage (+page.svelte 跳转确认页)', () => {
  it('正确渲染截图对应的安全跳转确认界面', () => {
    const { getByText, getByRole, container } = render(LinkPage, {
      props: {
        data: {
          target: 'https://www.arsrna.cn'
        }
      }
    });

    // 1. 顶栏使用本项目默认品牌名，不冒用第三方品牌
    expect(getByText('BBLBB')).toBeInTheDocument();

    // 2. 主体提示文案与目标 URL
    expect(getByText('即将离开 BBLBB，请注意账号和财产安全')).toBeInTheDocument();
    expect(getByText('https://www.arsrna.cn/')).toBeInTheDocument();

    // 3. 继续访问按钮与属性
    const proceedBtn = getByRole('link', { name: '继续访问' });
    expect(proceedBtn).toBeInTheDocument();
    expect(proceedBtn).toHaveAttribute('href', 'https://www.arsrna.cn/');
    expect(proceedBtn).toHaveAttribute('target', '_blank');
    expect(proceedBtn).toHaveAttribute('rel', 'noopener noreferrer nofollow');
    expect(proceedBtn).toHaveAttribute('data-direct-jump', 'true');

    // 4. 等距立体 3D 插画视觉元素
    const visual = container.querySelector('.link-visual-icon');
    expect(visual).not.toBeNull();

    // Footer exposes only a real first-party link; no third-party copyright or fake anchors.
    expect(container.textContent).not.toContain('Tencent');
    expect(container.textContent).not.toContain('CNB');
    expect(container.querySelectorAll('a[href^="#"]')).toHaveLength(0);
    expect(getByRole('link', { name: '返回 BBLBB' })).toHaveAttribute('href', '/');
  });

  it('缺少目标链接时展示友好的错误指引与返回首页按钮', () => {
    const { getByText, getByRole } = render(LinkPage, {
      props: {
        data: {
          target: ''
        }
      }
    });

    expect(getByText('缺少跳转目标链接')).toBeInTheDocument();
    const homeBtn = getByRole('link', { name: '返回首页' });
    expect(homeBtn).toHaveAttribute('href', '/');
  });

  it('遇到危险或非法协议时阻止跳转并提示', () => {
    const { getByText, getByRole } = render(LinkPage, {
      props: {
        data: {
          target: 'javascript:alert(1)'
        }
      }
    });

    expect(getByText('不安全的链接协议')).toBeInTheDocument();
    const homeBtn = getByRole('link', { name: '返回首页' });
    expect(homeBtn).toHaveAttribute('href', '/');
  });

  it('load 同时支持 target 与旧 url 参数别名，target 优先', async () => {
    const fromLegacyAlias = (await load({ url: new URL('https://site.test/link?url=https%3A%2F%2Fexample.com') } as any)) as { target: string };
    expect(fromLegacyAlias.target).toBe('https://example.com');

    const targetWins = (await load({
      url: new URL('https://site.test/link?url=https%3A%2F%2Fold.test&target=https%3A%2F%2Fnew.test')
    } as any)) as { target: string };
    expect(targetWins.target).toBe('https://new.test');
  });

  it('自定义后台站点名称时，动态应用在标题和文案中', () => {
    const { getByText } = render(LinkPage, {
      props: {
        data: {
          target: 'https://example.com',
          site: {
            siteName: '极客社区',
            siteDescription: '技术讨论区',
            currencyName: '金币',
            loginEyebrow: '',
            loginTitle: '',
            loginSubtitle: '',
            registerEyebrow: '',
            registerTitle: '',
            registerSubtitle: '',
            maintenanceMode: false,
            googleLoginEnabled: false,
            githubLoginEnabled: false
          }
        }
      }
    });

    expect(getByText('极客社区')).toBeInTheDocument();
    expect(getByText('即将离开 极客社区，请注意账号和财产安全')).toBeInTheDocument();
  });
});
