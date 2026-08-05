import { defineConfig } from 'vitest/config';
import { sveltekit } from '@sveltejs/kit/vite';

// M00-FRONTEND-07/08/09：前端单测配置。
// 复用 SvelteKit 插件以获得 $lib 别名与 .svelte 编译；环境固定 jsdom。
// conditions 含 browser：svelte 包按 browser 条件解析到 index-client.js，
// 否则默认解析到服务端构建（mount 不可用）。
export default defineConfig({
  plugins: [sveltekit()],
  resolve: {
    conditions: ['browser']
  },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.{test,spec}.{js,ts}'],
    setupFiles: ['./src/test/setup.ts'],
    restoreMocks: true
  }
});