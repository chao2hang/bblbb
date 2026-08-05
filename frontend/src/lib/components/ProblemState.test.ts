// M02-UX-07：ProblemState 可访问 + 可恢复状态——role=alert 播报、
// 各状态码默认恢复动作、429 Retry-After、request ID 可复制、
// children 覆盖默认动作。
import { describe, expect, it, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import ProblemState from './ProblemState.svelte';
import { expectSrAnnouncement } from '$lib/testing/a11y';

describe('ProblemState（可访问性）', () => {
  it('以 role=alert 播报错误状态（assertive live region）', () => {
    const { container } = render(ProblemState, { status: 503, desc: '服务暂不可用' });
    const region = container.querySelector('[role="alert"]');
    expect(region).not.toBeNull();
    expectSrAnnouncement(container, '服务暂不可用', 'alert');
  });

  it('按状态显示标题与文案映射', () => {
    const { container } = render(ProblemState, { status: 401 });
    expect(container.textContent).toContain('需要登录');
    const { container: c2 } = render(ProblemState, { status: 429 });
    expect(c2.textContent).toContain('操作太频繁');
  });

  it('request ID 以 code 展示并带 aria-label（可复制）', () => {
    const { container } = render(ProblemState, {
      status: 500,
      problem: { status: 500, request_id: 'rid-abc' }
    });
    const code = container.querySelector('code.problem-request-id');
    expect(code).not.toBeNull();
    expect(code).toHaveTextContent('rid-abc');
    expect(code).toHaveAttribute('aria-label', '服务端请求号：rid-abc');
  });
});

describe('ProblemState（可恢复动作）', () => {
  it('401 → 去登录 + 返回首页链接', () => {
    const { container } = render(ProblemState, { status: 401 });
    const links = Array.from(container.querySelectorAll('a')).map((a) => a.textContent?.trim());
    expect(links).toContain('去登录');
    expect(container.querySelector('a[href="/login"]')).not.toBeNull();
    expect(container.querySelector('a[href="/"]')).not.toBeNull();
  });

  it('403 → 返回首页链接 + 返回上一页按钮', () => {
    const { container } = render(ProblemState, { status: 403 });
    expect(container.querySelector('a[href="/"]')).not.toBeNull();
    expect(container.textContent).toContain('返回上一页');
  });

  it('404 → 返回首页链接', () => {
    const { container } = render(ProblemState, { status: 404 });
    expect(container.querySelector('a[href="/"]')).not.toBeNull();
  });

  it('429 → 显示 Retry-After 秒数 + 刷新按钮', () => {
    const { container } = render(ProblemState, {
      status: 429,
      problem: { status: 429, retry_after: 42 }
    });
    expect(container.textContent).toContain('请在 42 秒后重试');
    expect(container.textContent).toContain('刷新页面');
  });

  it('503 → 稍后重试（reload）+ 返回首页', () => {
    const { container } = render(ProblemState, { status: 503 });
    expect(container.textContent).toContain('稍后重试');
    expect(container.querySelector('a[href="/"]')).not.toBeNull();
  });

  it('刷新按钮触发 window.location.reload（可恢复）', () => {
    const reload = vi.fn();
    vi.stubGlobal('location', { reload });
    const { container } = render(ProblemState, { status: 503 });
    const button = Array.from(container.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('稍后重试')
    );
    expect(button).not.toBeUndefined();
    button!.click();
    expect(reload).toHaveBeenCalledOnce();
    vi.unstubAllGlobals();
  });

  it('children 覆盖默认恢复动作', () => {
    const custom = createRawSnippet(() => ({
      render: () => '<span>自定义动作</span>'
    }));
    const { container } = render(ProblemState, { status: 404, children: custom });
    expect(container.textContent).toContain('自定义动作');
    expect(container.querySelector('a[href="/"]')).toBeNull();
  });
});
