// M00-FRONTEND-07：vitest 全局夹具。
// - 注册 jest-dom 匹配器（toBeInTheDocument/toHaveFocus/toHaveAttribute…）。
// - 每个用例前安装 matchMedia mock（jsdom 未实现，减少动效/偏好查询测试依赖它）。
// - 每个用例后清理渲染容器。
import '@testing-library/jest-dom/vitest';
import { cleanup } from '@testing-library/svelte';
import { afterEach, beforeEach } from 'vitest';
import { installMatchMedia } from '../lib/testing/a11y';

beforeEach(() => {
  installMatchMedia();
  document.body.innerHTML = '';
});

afterEach(() => {
  cleanup();
  document.body.innerHTML = '';
});