import { defineConfig } from 'vitest/config';
import { sveltekit } from '@sveltejs/kit/vite';

// M00-FRONTEND-07/08/09：前端单测配置。
//
// 两个项目：
//  - dom：jsdom + @testing-library/svelte（mount 需要 svelte 的 client 构建，
//    因此 resolve.conditions 含 browser）。
//  - ssr：node 环境 + svelte/server 渲染（验证无 JS 时 SSR HTML 可读；
//    组件按 server 编译，svelte 解析到 index-server.js）。
export default defineConfig({
  plugins: [sveltekit()],
  test: {
    projects: [
      {
        extends: true,
        resolve: { conditions: ['browser'] },
        test: {
          name: 'dom',
          environment: 'jsdom',
          include: ['src/**/*.{test,spec}.{js,ts}'],
          exclude: ['src/lib/testing/ssr/**'],
          setupFiles: ['./src/test/setup.ts'],
          restoreMocks: true
        }
      },
      {
        extends: true,
        resolve: { conditions: ['module', 'node', 'development'] },
        test: {
          name: 'ssr',
          environment: 'node',
          include: ['src/lib/testing/ssr/**'],
          restoreMocks: true
        }
      }
    ]
  }
});