import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

// 远程测试与本地 HTTPS 支持：
// 后端会话/CSRF cookie 是 __Host- 前缀（强制 Secure，M02-SESSION-02），
// 非 localhost 的明文 http 无法存储该 cookie（浏览器与 curl 均拒绝），
// 远程或非 localhost 访问必须经 https。默认开发证书按本配置文件定位，
// 不受 npm run dev 的当前工作目录影响；环境变量仍可覆盖证书路径。
const defaultCert = fileURLToPath(new URL('../dev/certs/dev.crt', import.meta.url));
const defaultKey = fileURLToPath(new URL('../dev/certs/dev.key', import.meta.url));
const resolveTlsPath = (value: string): string => resolve(process.cwd(), value);
const tlsCert = process.env.BBLBB_DEV_TLS_CERT
  ? resolveTlsPath(process.env.BBLBB_DEV_TLS_CERT)
  : existsSync(defaultCert)
    ? defaultCert
    : undefined;
const tlsKey = process.env.BBLBB_DEV_TLS_KEY
  ? resolveTlsPath(process.env.BBLBB_DEV_TLS_KEY)
  : existsSync(defaultKey)
    ? defaultKey
    : undefined;

// 自签名证书环境下，允许 Node.js SSR 内部请求信任该证书
if (tlsCert && tlsKey && process.env.NODE_TLS_REJECT_UNAUTHORIZED === undefined) {
  process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0';
}

// /api 代理目标可用 E2E_API_TARGET 覆盖（视觉检测等多实例并行时避免与
// 8080 上的既有后端冲突）；默认不变。
const apiTarget = process.env.E2E_API_TARGET ?? 'http://127.0.0.1:8080';

export default defineConfig({
  plugins: [sveltekit()],
  server: {
    // 远程浏览器需要访问 10.10.10.10:5173；开发服务器默认绑定所有接口，
    // 可用 BBLBB_DEV_HOST=127.0.0.1 恢复仅本机监听。
    host: process.env.BBLBB_DEV_HOST ?? '0.0.0.0',
    hmr: { overlay: false },
    ...(tlsCert && tlsKey
      ? { https: { cert: readFileSync(tlsCert), key: readFileSync(tlsKey) } }
      : {}),
    proxy: {
      // 显式对象形式（M14-A11Y-01）：字符串简写在部分 Vite 7.x 版本下
      // 代理异常，改为 target + changeOrigin 保证 /api 转发到内部后端。
      // 注意：必须用 ^/api/ 正则而非 '/api' 前缀——vite 代理键是路径前缀
      // 匹配，'/api' 会把前端路由 /apikeys 一并代理到后端（GAP-FIX 修复）。
      '^/api/': { target: apiTarget, changeOrigin: false },
      '/healthz': { target: apiTarget, changeOrigin: false },
      '/readyz': { target: apiTarget, changeOrigin: false }
    }
  }
});
