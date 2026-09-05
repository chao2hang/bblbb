/**
 * BBLBB 小程序全局配置
 *
 * 发布前请修改 API_BASE 为你的后端 HTTPS 域名，并在微信公众平台
 * 「开发管理 → 开发设置 → 服务器域名」中将 request 合法域名配置为
 * 同一 HTTPS 域名（见 README「部署对接」一节）。
 */
module.exports = {
  /**
   * 后端 API 根地址（不含 /api/v1 前缀）。
   *
   * - 开发者工具本地调试：http://127.0.0.1:18181（miniprogram/dev/start-all.sh
   *   启动的小程序专用联调后端：独立端口/独立 SQLite/已放行微信 Referer
   *   来源）。需勾选「详情 → 本地设置 → 不校验合法域名…」。
   *   注意：不要指向仓库主开发后端（8080）——其 ALLOWED_ORIGINS 未包含
   *   servicewechat.com，写请求会 400 origin_not_allowed。
   * - 真机 / 生产：必须为 HTTPS 且已登记 request 合法域名，
   *   例如 https://forum.example.com（服务端 BBLBB__ALLOWED_ORIGINS
   *   需包含 https://servicewechat.com）。
   */
  API_BASE: 'http://127.0.0.1:18181',

  /**
   * 手动 Cookie 管理开关（默认 false）。
   *
   * 微信 wx.request 默认自动处理 Set-Cookie / Cookie（运行时 cookie 引擎），
   * 正常情况保持 false 即可。若个别基础库/真机环境未自动携带 cookie，
   * 置为 true 启用本工具内置的 cookie jar（自行解析 Set-Cookie 并
   * 在后续请求显式携带 Cookie 头）作为兜底。
   */
  MANUAL_COOKIES: false,

  /** 请求超时（毫秒），与 app.json networkTimeout 保持一致 */
  TIMEOUT_MS: 15000,

  /** 分页默认条数 */
  PAGE_SIZE: 20,

  /**
   * 客户端标识（仅用于日志/审计识别流量来源，后端不消费）。
   * 微信 wx.request 的 User-Agent 由运行时固定携带（MicroMessenger），
   * 后端反爬与来源校验均以运行时行为为准。
   */
  CLIENT_TAG: 'bblbb-miniprogram/1.0',
};
