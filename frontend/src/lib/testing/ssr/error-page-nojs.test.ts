// M02-UX-07：全局错误页（+error.svelte）SSR 基线——未处理错误渲染为
// 可访问（role=alert）且可恢复（按状态恢复动作）的 ProblemState；
// 404 渲染“内容未找到 + 返回首页”。
import { describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import ErrorPage from '../../../routes/+error.svelte';

vi.mock('$app/state', () => ({
  page: { error: { status: 404, message: 'Not Found' } }
}));

describe('无 JS：全局错误页（M02-UX-07）', () => {
  it('404 → role=alert 状态 + 返回首页链接', () => {
    const { body } = render(ErrorPage, {});
    expect(body).toContain('role="alert"');
    expect(body).toContain('内容未找到');
    expect(body).toContain('返回首页');
    expect(body).not.toContain('/login'); // 404 不出现无关的登录动作
  });

  it('不输出默认英文消息（Not Found 被过滤）', () => {
    const { body } = render(ErrorPage, {});
    expect(body).not.toContain('Not Found');
  });
});
