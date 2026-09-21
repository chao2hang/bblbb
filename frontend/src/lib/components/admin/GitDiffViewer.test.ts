import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/svelte';
import GitDiffViewer from './GitDiffViewer.svelte';

describe('GitDiffViewer Component', () => {
  it('renders side-by-side split mode by default', () => {
    const { container } = render(GitDiffViewer, {
      beforeBody: 'Hello World',
      afterBody: 'Hello Git World',
      fromVersion: 1,
      toVersion: 2,
      reason: 'Feature commit'
    });

    expect(container.textContent).toContain('v1 → v2');
    expect(container.textContent).toContain('+1');
    expect(container.textContent).toContain('-1');
    expect(container.textContent).toContain('修改前');
    expect(container.textContent).toContain('修改后');
    expect(container.textContent).toContain('Feature commit');
  });

  it('renders new file mode when beforeBody is null', () => {
    const { container } = render(GitDiffViewer, {
      beforeBody: null,
      afterBody: 'Brand new article line 1\nLine 2',
      fromVersion: null,
      toVersion: 1
    });

    expect(container.textContent).toContain('首次提交');
    expect(container.textContent).toContain('新增文章（首次提交）');
    expect(container.textContent).toContain('+2');
  });
});
