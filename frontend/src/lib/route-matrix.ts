// M03-UI-09：原型 → 生产路由矩阵（唯一数据源，文档与结构测试共用）。
//
// 原型（prototype/js/router.js，47 路由：42 静态 + 5 动态）仅作设计回归，生产前端绝不导入其
// mock/store 数据源（M14-ROUTES-07）。本矩阵把每个原型路由映射到生产路由：
//   - shipped：已存在 +page 路由（svelte-check/build 强制）；
//   - planned：契约/页面随对应里程碑交付（milestone 字段标注）。
// 结构测试 route-matrix.test.ts 据此断言：shipped 路由的 +page.svelte 存在、
// 生产源码无 prototype 导入、矩阵覆盖全部原型路由。

export interface PrototypeRouteEntry {
  /** 原型路径（router.js 原始 pattern）。 */
  prototype: string;
  /** 生产路由（SvelteKit 路径，[] 为动态段）。 */
  production: string;
  status: 'shipped' | 'planned';
  milestone: string;
  note: string;
}

export const PROTOTYPE_ROUTE_MATRIX: PrototypeRouteEntry[] = [
  // ── 公开浏览 ─────────────────────────────────────────────────────────────
  { prototype: '/', production: '/', status: 'shipped', milestone: 'M00/M03', note: '首页（板块/标签/最新讨论 SSR）' },
  { prototype: '/discover', production: '/discover', status: 'shipped', milestone: 'GAP-FIX', note: '发现页（热门标签/活跃内容/板块导航）' },
  { prototype: '/messages', production: '/messages', status: 'shipped', milestone: 'GAP-FIX', note: '私信（会话列表/消息线程/发送/已读）' },
  { prototype: '/achievements', production: '/achievements', status: 'shipped', milestone: 'GAP-FIX', note: '成就墙（总览/目录/隐藏成就/装备槽）' },
  { prototype: '/apikeys', production: '/apikeys', status: 'shipped', milestone: 'GAP-FIX', note: 'API 密钥（创建一次性展示/撤销）' },
  { prototype: '/market', production: '/marketplace', status: 'shipped', milestone: 'GAP-FIX', note: '应用市场（交易摘要/精选应用/购买入口）' },
  { prototype: '/billing', production: '/me/billing', status: 'shipped', milestone: 'GAP-FIX', note: '下载账单（统计/流水/重新下载）' },
  { prototype: '/boards', production: '/boards', status: 'shipped', milestone: 'M3', note: '板块总览（板块树）' },
  { prototype: '/boards/{slug}', production: '/boards/[slug]', status: 'shipped', milestone: 'M3', note: '板块详情 + 帖子 + 权限提示' },
  { prototype: '/tags', production: '/tags', status: 'shipped', milestone: 'M3', note: '标签分组展示' },
  { prototype: '/tags/{name}', production: '/tags/[slug]', status: 'shipped', milestone: 'GAP-FIX', note: '标签聚合页（该标签帖子列表/空态/分页）' },
  { prototype: '/topics/{id}', production: '/posts/[id]', status: 'shipped', milestone: 'M14', note: '帖子详情（M14 起与实际路由一致）' },
  { prototype: '/users/{name}', production: '/users/[username]', status: 'shipped', milestone: 'M3', note: '用户主页 SSR（公开投影）' },
  { prototype: '/search', production: '/search', status: 'shipped', milestone: 'M3', note: '搜索页 + 标签筛选 chip' },
  { prototype: '/publish', production: '/editor', status: 'shipped', milestone: 'M14', note: '发帖编辑器（M14 起与实际路由一致）' },
  { prototype: '/notifications', production: '/notifications', status: 'shipped', milestone: 'M5', note: '通知列表/已读/偏好/失效态随 M05-UI 落地' },
  { prototype: '/favorites', production: '/favorites', status: 'shipped', milestone: 'GAP-FIX', note: '收藏列表（取消收藏/加载更多/空态）' },
  { prototype: '/shop', production: '/shop', status: 'shipped', milestone: 'M7', note: '积分商城列表（价格/库存/等级门槛/限购）随 M07-SHOP 落地' },
  { prototype: '/activity', production: '/me/balance', status: 'shipped', milestone: 'M7', note: '余额/等级/经验/签到随 M07-LEVELS 落地（/me/balance）' },
  { prototype: '/me/closet', production: '/me/wardrobe', status: 'shipped', milestone: 'M7', note: '装扮衣橱（装备/徽章/预览）随 M07-SHOP 落地' },

  // ── 身份 ─────────────────────────────────────────────────────────────────
  { prototype: '/login', production: '/login', status: 'shipped', milestone: 'M2', note: '登录（含 TOTP 两步）' },
  { prototype: '/register', production: '/register', status: 'shipped', milestone: 'M2', note: '注册' },
  { prototype: '/forgot-password', production: '/password-reset', status: 'shipped', milestone: 'M2', note: '忘记密码（含 /confirm）' },
  { prototype: '/settings', production: '/settings', status: 'shipped', milestone: 'M3', note: '资料编辑（If-Match 并发）' },
  { prototype: '/mfa', production: '/mfa', status: 'shipped', milestone: 'M18', note: '独立两步验证页（M18-MFA-01：TOTP secret / 验证确认 / 恢复码）' },
  { prototype: '/403', production: '+error.svelte', status: 'shipped', milestone: 'M2', note: '错误页统一渲染' },
  { prototype: '/404', production: '+error.svelte', status: 'shipped', milestone: 'M2', note: '错误页统一渲染' },
  { prototype: '/429', production: '+error.svelte', status: 'shipped', milestone: 'M2', note: '错误页统一渲染' },

  // ── 管理后台（M13-ADMIN/M13-UI 落地；已交付页见 status） ───────────────────
  { prototype: '/admin', production: '/admin', status: 'shipped', milestone: 'M3', note: '后台入口卡片（M13 全量导航）' },
  { prototype: '/admin/users', production: '/admin/users', status: 'shipped', milestone: 'M13', note: '用户管理（user.manage；If-Match + reason + recent-auth）' },
  { prototype: '/admin/roles', production: '/admin/roles', status: 'shipped', milestone: 'M3', note: '角色列表（后端裁决，501→开发中态）' },
  { prototype: '/admin/content', production: '/admin/content', status: 'shipped', milestone: 'M18', note: '内容审核（M18-ADMIN-CONTENT：待审列表 + 通过/驳回 + 审计）' },
  { prototype: '/admin/posts', production: '/admin/posts', status: 'shipped', milestone: 'M13', note: '帖子管理概览（权限门 + 板块/审核链接）' },
  { prototype: '/admin/boards', production: '/admin/boards', status: 'shipped', milestone: 'M3', note: '板块管理（列表 501 + 创建表单可用）' },
  { prototype: '/admin/tags', production: '/admin/tags', status: 'shipped', milestone: 'M3', note: '标签管理（列表 501 + 创建表单可用）' },
  { prototype: '/admin/attachments', production: '/admin/attachments', status: 'shipped', milestone: 'M13', note: '附件管理（存储脱敏视图 + 下载计费入口）' },
  { prototype: '/admin/download-billing', production: '/admin/download-billing', status: 'shipped', milestone: 'M13', note: '下载计费（脱敏策略视图）' },
  { prototype: '/admin/ai', production: '/admin/ai', status: 'shipped', milestone: 'M9', note: 'AI 管理（Provider 脱敏状态/预算/任务重试取消/Flag 配置）随 M09-UI 落地' },
  { prototype: '/admin/video', production: '/admin/video', status: 'shipped', milestone: 'M10', note: '视频管理（Provider 策略配置/测试/停用/审计展示）随 M10-UI 落地' },
  { prototype: '/admin/storage', production: '/admin/storage', status: 'shipped', milestone: 'M6', note: '存储管理（local/S3 配置/测试/脱敏状态）随 M06-UI 落地' },
  { prototype: '/admin/notifications', production: '/admin/notifications', status: 'shipped', milestone: 'M13', note: '通知概览（权限门 + 活跃/商城配置链接）' },
  { prototype: '/admin/audit', production: '/admin/audit', status: 'shipped', milestone: 'M13', note: '审计概览（权限门 + 文档入口）' },
  { prototype: '/admin/reports', production: '/moderation/report', status: 'shipped', milestone: 'M5', note: '举报页（M05-UI，无 JS 退化）' },
  { prototype: '/admin/reports/{id}', production: '/moderation/appeals/[id]', status: 'shipped', milestone: 'M5', note: '申诉详情/撤回（M05-UI）' },
  { prototype: '/admin/points', production: '/admin/points', status: 'shipped', milestone: 'M13', note: '积分/活跃配置（只读；禁止直接改余额或流水）' },
  { prototype: '/admin/levels', production: '/admin/levels', status: 'shipped', milestone: 'M13', note: '等级/附件配额（M06-QUOTA 脱敏视图）' },
  { prototype: '/admin/themes', production: '/admin/themes', status: 'shipped', milestone: 'M13', note: '主题管理（上传/预览/默认/Token 编辑/版本冲突）' },
  { prototype: '/admin/plugins', production: '/admin/plugins', status: 'shipped', milestone: 'M13', note: '插件管理（v1 配置型：能力白名单/安装/启停/设置）' },
  { prototype: '/admin/oauth', production: '/admin/oauth', status: 'shipped', milestone: 'M13', note: 'OIDC 管理（OAuth Client 脱敏列表）' },
  { prototype: '/admin/marketplace', production: '/admin/marketplace', status: 'shipped', milestone: 'M12', note: 'Marketplace 管理（Client/Scope/Offer/余额/Webhook/对账/紧急停用）随 M12-UI 落地' },
  { prototype: '/admin/shop', production: '/admin/shop', status: 'shipped', milestone: 'M7', note: '商城管理（商品/订单/退款）随 M07-UI 落地' },
  { prototype: '/admin/activity', production: '/admin/activity', status: 'shipped', milestone: 'M7', note: '活跃管理（签到/任务配置）随 M07-UI 落地' },
  { prototype: '/admin/settings', production: '/admin/settings', status: 'shipped', milestone: 'GAP-FIX', note: '站点设置（功能开关/站点信息/If-Match 保存）' },
  { prototype: '/admin-achievements', production: '/admin/achievements', status: 'shipped', milestone: 'GAP-FIX', note: '成就管理（目录 CRUD/手工授予/启停）' },
  { prototype: '/admin-bi', production: '/admin/bi', status: 'shipped', milestone: 'GAP-FIX', note: 'BI 数据看板（周期切换/KPI 指标卡/环比）' }
];
