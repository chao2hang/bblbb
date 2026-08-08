import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [sveltekit()],
  server: {
    proxy: {
      // 显式对象形式（M14-A11Y-01）：字符串简写在部分 Vite 7.x 版本下
      // 代理异常，改为 target + changeOrigin 保证 /api 转发到内部后端。
      '/api': { target: 'http://127.0.0.1:8080', changeOrigin: false },
      '/healthz': { target: 'http://127.0.0.1:8080', changeOrigin: false },
      '/readyz': { target: 'http://127.0.0.1:8080', changeOrigin: false }
    }
  }
});
