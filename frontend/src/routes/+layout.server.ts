import type { LayoutServerLoad } from './$types';

// M00-FRONTEND-06：SSR/浏览器缓存边界。
//
// 站点包含 Session（navbar 用户态）、通知、设置等个人化内容，且帖子存在
// 非公开可见性；任何 HTML 一旦进入共享缓存都可能泄漏会话化片段。
// 因此根 layout 对所有 SSR 响应固定 `Cache-Control: private, no-store`：
//  - 浏览器/中间层不得缓存页面 HTML（始终回源渲染）；
//  - 静态哈希资源（public, immutable, max-age=31536000）由部署层另行配置，
//    与这里互不影响；
//  - 客户端 API 响应（/api/v1/*）由后端各自控制，不在此范围。
export const load: LayoutServerLoad = ({ setHeaders }) => {
  setHeaders({
    'Cache-Control': 'private, no-store'
  });
  return {};
};
