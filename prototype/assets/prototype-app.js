/* BBLBB prototype completion runtime
 * One-entry hash SPA: routes, mock resources, state transitions and verification affordances.
 */
(function () {
  'use strict';

  var APP_KEY = 'bblbb-prototype-state';
  var PAGE_IDS = ['articles', 'boards', 'board', 'tags', 'tag', 'topic', 'publish', 'user', 'favorites', 'settings', 'register', 'forgot-password', 'billing', 'appeals', 'market', 'checkout', 'purchases', 'apikeys', 'mfa', '403', '404', '429', 'error', 'messages'];
  var ADMIN_ITEMS = [
    ['#admin', '仪表盘', 'layout-dashboard'],
    ['#admin-reports', '举报与审核', 'flag'],
    ['#admin-points', '积分与货币', 'coins'],
    ['#admin-levels', '等级管理', 'award'],
    ['#admin-achievements', '成就管理', 'trophy'],
    ['#admin-themes', '主题管理', 'palette'],
    ['#admin-plugins', '插件管理', 'puzzle'],
    ['#admin-oauth', 'OAuth 客户端', 'shield'],
    ['#admin-storage', '文件存储', 'server'],
    ['#admin-download-billing', '下载计费', 'download'],
    ['#admin-ai', '大模型设置', 'sparkles'],
    ['#admin-video', '视频插件', 'video'],
    ['#admin-marketplace', '市场与交易', 'shopping-cart'],
    ['#admin-bi', 'BI 看板', 'bar-chart'],
    ['#admin-users', '用户管理', 'users'],
    ['#admin-roles', '角色与权限', 'shield'],
    ['#admin-boards', '板块管理', 'workflow'],
    ['#admin-posts', '内容管理', 'file-text'],
    ['#admin-tags', '标签管理', 'tag'],
    ['#admin-attachments', '附件管理', 'paperclip'],
    ['#admin-notifications', '通知与邮件', 'bell'],
    ['#admin-audit', '审计日志', 'file-text'],
    ['#admin-settings', '系统设置', 'settings']
  ];
  var ADMIN_GROUPS = [
    ['概览', ['#admin']],
    ['内容', ['#admin-posts', '#admin-boards', '#admin-tags', '#admin-attachments']],
    ['用户与权限', ['#admin-users', '#admin-roles', '#admin-reports']],
    ['激励与交易', ['#admin-achievements', '#admin-points', '#admin-levels', '#admin-marketplace', '#admin-download-billing']],
    ['扩展与集成', ['#admin-themes', '#admin-plugins', '#admin-oauth', '#admin-ai', '#admin-video']],
    ['系统', ['#admin-storage', '#admin-notifications', '#admin-audit', '#admin-settings', '#admin-bi']]
  ];
  var BOARDS = [
    { slug: 'tech-essay', name: '技术随笔', desc: '深入记录工程实践、性能优化与架构取舍。', count: '86 个主题', today: '14 今日', mod: 'Nina' },
    { slug: 'rust', name: 'Rust', desc: 'Rust 工程、异步运行时与生态实践。', count: '64 个主题', today: '9 今日', mod: 'Chaos' },
    { slug: 'web-dev', name: 'Web 开发', desc: 'SvelteKit、前端体验与全栈交付。', count: '52 个主题', today: '8 今日', mod: 'Alice' },
    { slug: 'opensource', name: '开源项目', desc: '项目协作、维护与社区治理。', count: '38 个主题', today: '5 今日', mod: 'Yuwen' },
    { slug: 'chat', name: '闲聊', desc: '技术之外，聊聊正在发生的事。', count: '27 个主题', today: '4 今日', mod: 'Mark' },
    { slug: 'meta', name: '站务', desc: '社区公告、反馈与使用指南。', count: '18 个主题', today: '2 今日', mod: 'Yuwen' }
  ];
  var ARTICLES = [
    { id: '101', title: 'Rust 小机器上的 SQLite 并发实践', summary: '在资源受限的小型服务器上跑 SQLite，需要从连接池、checkpoint、WAL 三处同时入手。', author: 'Chaos', board: 'tech-essay', tags: 'Rust / SQLite / 性能优化', views: '3.2k' },
    { id: '102', title: '自建 OIDC Provider 的最小集', summary: '从 discovery、JWKS、userinfo、revocation 开始，整理自托管场景下的最小方案。', author: 'Yuwen', board: 'opensource', tags: 'OAuth / 自托管', views: '1.8k' },
    { id: '103', title: 'Markdown 渲染沙箱与 CSP 实战', summary: '论坛型站点的 XSS 风险集中在 Markdown 渲染管线与外链图片。', author: 'Mark', board: 'web-dev', tags: '安全 / Markdown', views: '2.4k' },
    { id: '104', title: 'SQLite 在 512MB 小机器上的取舍', summary: 'WAL、连接池和 checkpoint 在生产环境中的取舍与验证。', author: 'Alice', board: 'tech-essay', tags: 'SQLite / 性能优化', views: '1.5k' }
  ];
  var TOPICS = {
    '101': { id: '101', type: 'article', title: ARTICLES[0].title, author: 'Chaos', board: 'tech-essay', boardName: '技术随笔', tags: ['Rust', 'SQLite', '性能优化'], views: '3.2k', restricted: 'level', locked: '达到 Lv.3 或回复后可见', body: '在资源受限的小型服务器上跑 SQLite，需要从连接池、checkpoint、WAL 三处同时入手。本文结合 BBLBB 的真实部署约束，给出可验证的并发配置与回滚策略。' },
    'rules': { id: 'rules', type: 'topic', title: '社区发布规范（v1.0）', author: 'Yuwen', board: 'meta', boardName: '站务', tags: ['社区'], views: '3.2k', body: '1. 内容对版：技术内容请发到对应板块，跨板块内容请说明理由。2. 禁止站外商业推广（违反者按举报处理 §3）。3. 友善交流，不进行人身攻击；争议请基于事实与论据。4. 付费内容须明确标注解锁价格，发布前建议先阅读板块发帖规则。5. 定时发布与草稿发布在等级变化后会重新校验可见性。' },
    '102': { id: '102', type: 'topic', title: '自建 OIDC Provider 的最小集', author: 'Yuwen', board: 'opensource', boardName: '开源项目', tags: ['OAuth', '自托管'], views: '1.8k', body: '从 discovery、JWKS、userinfo、revocation 开始，整理自托管场景下的最小方案。' },
    '103': { id: '103', type: 'topic', title: 'Markdown 渲染沙箱与 CSP 实战', author: 'Mark', board: 'web-dev', boardName: 'Web 开发', tags: ['安全', 'Markdown'], views: '2.4k', body: '论坛型站点的 XSS 风险集中在 Markdown 渲染管线与外链图片。' },
    '104': { id: '104', type: 'article', title: 'SQLite 在 512MB 小机器上的取舍', author: 'Alice', board: 'tech-essay', boardName: '技术随笔', tags: ['SQLite', '性能优化'], views: '1.5k', body: 'WAL、连接池和 checkpoint 在生产环境中的取舍与验证。' },
    '201': { id: '201', type: 'topic', title: '使用 SvelteKit 构建博客与轻量论坛是否合理？', author: 'Chaos', board: 'web-dev', boardName: 'Web 开发', tags: ['SvelteKit', '自托管'], views: '2.1k', restricted: 'reply', locked: '回复本主题后可见', body: '这个月用 SvelteKit 写完 BBLBB 的核心页面，包括 SSR、SEO、CSRF、表单与 OIDC 接入。整体体验比想象中好，但踩了几个坑。' },
    '202': { id: '202', type: 'topic', title: 'OAuth Provider 应该自研到什么程度？', author: 'Alice', board: 'opensource', boardName: '开源项目', tags: ['OAuth', '自托管'], views: '958', restricted: 'paid', locked: '支付 10 B币永久解锁', body: '社区小到一定规模就会面临这个问题：是直接接 Keycloak / Authentik，还是自己实现一个 OIDC Provider？本文尝试给出一个决策树。' },
    welcome: { id: 'welcome', type: 'topic', title: '欢迎来到 BBLBB 社区', author: 'Lin', board: 'meta', boardName: '站务', tags: ['社区'], views: '1.2k', restricted: 'reply', locked: '回复本主题后可见', body: '这是一个以技术交流、产品共创和高质量讨论为核心的社区。分享你的想法，认识志同道合的朋友。' },
    'product-update': { id: 'product-update', type: 'topic', title: '本周产品更新：主题与通知中心', author: 'Mark', board: 'meta', boardName: '站务', tags: ['产品'], views: '642', body: '我们刚完成主题切换和通知中心的第一版，欢迎体验后留下你的建议。' },
    performance: { id: 'performance', type: 'article', title: '分享一个简洁的 SvelteKit 页面性能优化方案', author: 'Nina', board: 'tech-essay', boardName: '技术随笔', tags: ['SvelteKit', '性能优化'], views: '1.8k', body: '从路由级代码分割、图片策略到数据预加载，整理了日常项目中最有效的几个优化点。' },
    'build-tools': { id: 'build-tools', type: 'article', title: '2026 前端构建工具选型回顾', author: 'Lin', board: 'tech-essay', boardName: '技术随笔', tags: ['性能优化', 'Rust'], views: '2.4k', body: '从 Vite、Rolldown 到 Rspack，整理了各自的优劣势和适用场景，附迁移成本对比。' },
    'rust-cli': { id: 'rust-cli', type: 'topic', title: '用 Rust 重写内部 CLI 后，CI 时间缩短了 60%', author: 'Ken', board: 'rust', boardName: 'Rust', tags: ['Rust', 'Docker'], views: '1.1k', body: '记录一次从 Python 到 Rust 的迁移实践，包含交叉编译与发布流程改造。' },
    'rust-async': { id: 'rust-async', type: 'topic', title: 'Rust 异步运行时选型：tokio、smol 还是 async-std？', author: 'Nina', board: 'rust', boardName: 'Rust', tags: ['Rust', 'tokio'], views: '1.4k', body: '三种运行时在 I/O 模型、任务调度和生态兼容上的取舍，附压测数据。' },
    'rust-ffi': { id: 'rust-ffi', type: 'topic', title: 'Rust FFI 与 Python 服务共存的边界怎么划？', author: 'Ken', board: 'rust', boardName: 'Rust', tags: ['Rust', 'FFI'], views: '732', body: '迁移到一半时，哪些模块继续用 Python、哪些下沉到 Rust，一个团队的判断标准。' },
    'design-system': { id: 'design-system', type: 'topic', title: '小团队如何落地一套轻量设计系统', author: 'Sara', board: 'chat', boardName: '闲聊', tags: ['UX', 'API'], views: '864', body: '从色板、间距到组件文档，一个两人小推的设计系统是怎么活过一年的。' },
    'load-wasm': { id: 'load-wasm', type: 'article', title: 'Wasm 边缘计算实践：把过滤器跑进 CDN', author: 'Ken', board: 'tech-essay', boardName: '技术随笔', tags: ['Wasm', '性能优化'], views: '0', body: '用 Wasm 把内容过滤逻辑下沉到边缘节点。' },
    'load-notif': { id: 'load-notif', type: 'topic', title: '通知偏好设置的可用性走查', author: 'Sara', board: 'product', boardName: '产品反馈', tags: ['产品', '可用性'], views: '0', body: '把通知选项按打扰强度重新分组。' },
    'load-design': { id: 'load-design', type: 'topic', title: '你删除过自己最满意的设计吗？', author: 'Zoe', board: 'chat', boardName: '闲聊', tags: ['闲聊', '设计'], views: '0', body: '有些方案当时觉得完美，半年后回头看全是问题。' }
  };
  var USERS = [
    { name: 'Chaos', initial: 'C', title: 'Rust / 自托管爱好者', bio: '写 Rust 和自托管，维护这个论坛。', level: 'LV.3', coin: 328, contrib: 146, exp: 2680, posts: 47, followers: 142, following: 38, joined: '2026-04-12', role: '版主' },
    { name: 'Yuwen', initial: 'Y', title: '站长与产品维护者', bio: '把社区做成一个可以长期维护的地方。', level: 'LV.4', coin: 1200, contrib: 312, exp: 6200, posts: 128, followers: 512, following: 61, joined: '2024-11-03', role: '站长' },    { name: 'Mark', initial: 'M', title: '全栈开发者', bio: '记录工程实践与产品反馈。', level: 'LV.2', coin: 124, contrib: 58, exp: 1850, posts: 23, followers: 87, following: 102, joined: '2025-06-20' },
    { name: 'Alice', initial: 'A', title: '后端与数据库调优', bio: '喜欢把复杂系统拆成可验证的小步。', level: 'LV.4', coin: 14, contrib: 22, exp: 5800, posts: 96, followers: 230, following: 45, joined: '2025-01-08' },
    { name: 'Nina', initial: 'N', title: '性能与可观测性', bio: '专注服务端性能优化与压测。', level: 'LV.3', coin: 412, contrib: 88, exp: 3400, posts: 61, followers: 175, following: 98, joined: '2025-09-15' },
    { name: 'Lin', initial: 'L', title: '前端工程化', bio: '喜欢把重复的事情变成工具。', level: 'LV.3', coin: 260, contrib: 96, exp: 2900, posts: 54, followers: 140, following: 88, joined: '2025-03-30' },
    { name: 'Ken', initial: 'K', title: 'Rust 系统编程', bio: '从 Python 迁移到 Rust 的半个当事人。', level: 'LV.3', coin: 190, contrib: 74, exp: 3100, posts: 42, followers: 96, following: 65, joined: '2025-08-11' },
    { name: 'Sara', initial: 'S', title: '设计与可用性', bio: '小团队里负责「看起来靠谱」的那个人。', level: 'LV.2', coin: 76, contrib: 31, exp: 1450, posts: 18, followers: 58, following: 73, joined: '2025-12-02' },
    { name: 'Zoe', initial: 'Z', title: '产品与设计', bio: '删代码比写代码多。', level: 'LV.2', coin: 54, contrib: 27, exp: 1200, posts: 15, followers: 44, following: 90, joined: '2026-02-14' }
  ];
  var GENERIC_ADMIN = {
    '#admin-download-billing': ['下载计费', '启停策略、默认价格、授权有效期、等级免费规则与日限额。', ['计费开关|开启', '默认下载价格|10 积分', '授权有效期|24 小时', '日限额|20 次']],
    '#admin-ai': ['大模型设置', 'Provider、模型、数据策略、预算与任务状态。', ['Provider|受控 Gateway', '当前模型|bblbb-format-v1', '数据策略|脱敏后发送', '预算|本月 72%']],
    '#admin-video': ['视频插件', '来源白名单、HLS 限制、CSP 状态与解析失败回退。', ['视频解析|已启用', 'HLS 来源|白名单校验', 'CSP|严格模式', '失败回退|显示安全链接']],
    '#admin-marketplace': ['市场与交易', 'Client 审批、限额、Webhook、对账与紧急禁用。', ['启用 Client|2 个', '待对账交易|3 笔', 'Webhook|健康', '紧急开关|未触发']],
    '#admin-bi': ['BI 数据看板', '活跃、内容增长、积分流水、审核通过率和导出。', ['日活 / 成员|421 / 1,284', '内容增长率|+2.6%', '审核通过率|88%', '导出|CSV 可用']],
    '#admin-users': ['用户管理', '管理账号状态、等级、验证状态与最近活动。', ['Chaos|LV.3 · 正常', 'Alice|LV.4 · 已验证', 'spam_bot_42|LV.1 · 封禁', 'Nina|LV.3 · 正常']],
    '#admin-roles': ['角色与权限', '角色范围与 operation 权限以服务端注册表为准。', ['管理员|admin.manage · audit.read', '版主|board.moderate · post.manage', '成员|content.create', '访客|public only']],
    '#admin-boards': ['板块管理', '板块可见性、版主、发帖策略和今日内容。', ['Rust|64 主题 · 版主 Chaos', 'Web 开发|52 主题 · 公开', '站务|18 主题 · 公开', '闲聊|27 主题 · 公开']],
    '#admin-posts': ['内容管理', '审核状态、作者、板块与内容操作。', ['使用 SvelteKit 构建博客与轻量论坛是否合理？|待审核', 'Rust 小机器上的 SQLite 并发实践|精华', 'OAuth Provider 应该自研到什么程度？|公开', 'Markdown 渲染沙箱与 CSP 实战|已发布']],
    '#admin-tags': ['标签管理', '标签名称、使用次数与合并/停用入口。', ['Rust|248 次使用', 'SvelteKit|182 次使用', '自托管|94 次使用', '性能优化|86 次使用']],
    '#admin-attachments': ['附件管理', '上传者等级、容量占用、临时链接策略和扫描状态。', ['report.png|安全扫描通过', 'architecture.pdf|私有 · 已完成', 'demo.zip|待扫描', 'cover.webp|公开 · 已完成']],
    '#admin-notifications': ['通知与邮件', '通知模板、投递状态、失败重试与退订策略。', ['验证邮件|待发送 · 0 失败', '通知摘要|已启用 · 每日 08:00', '失败队列|2 条待重试', '退订|策略已配置']],
    '#admin-audit': ['审计日志', '不可变管理员操作、request id、对象和处理原因。', ['积分调整 · Chaos|+50 B币 · 已记录', '举报处理 · R-1024|禁言 7 天', '主题切换|暗色主题', '存储测试|request_9f2a']],
    '#admin-settings': ['系统设置', '站点公开信息、注册策略、限流和维护模式。', ['注册|开放 · 邮箱验证', '维护模式|关闭', '默认语言|简体中文', '公开源|https://bblbb.local']]
  };

  function defaults() {
    return {
      auth: false,
      role: 'admin',
      account: { username: 'admin', displayName: 'Chaos', bio: '写 Rust 和自托管，维护这个论坛。' },
      returnTo: '',
      loginFailures: 0,
      lockedUntil: 0,
      draft: { title: '', body: '', board: '', tags: '', summary: '', visibility: 'public', paidPrice: 10, schedule: 'now', scheduleAt: '' },
      favorites: [],
      devices: ['current', 'iphone', 'linux'],
      topicUnlocked: {},
      comments: {},
      published: [],
      balances: { coin: 328, exp: 2680, contrib: 146 },
      reports: { 'R-1024': { status: 'pending', reason: '', timeline: ['提交举报 · 1 小时前', '自动分配给 Chaos · 45 分钟前'] } },
      toggles: { reply: true, email: false, index: true },
      messageFail: false,
      messages: {},
      activeConversation: 'Lin',
      followings: {},
      publishedTopics: [],
      publishedComments: [],
      ledger: [],
      notificationsRead: false,
      pendingVerification: null,
      customNotifications: [],
      deviceRevoked: {},
      oauthRevoked: {},
      preferences: { profileVisibility: 'public' }
    };
  }
  var state = defaults();

  function readState() {
    var base = defaults();
    try {
      var raw = localStorage.getItem(APP_KEY);
      if (raw) {
        var saved = JSON.parse(raw);
        state = Object.assign(base, saved || {});
        state.account = Object.assign(base.account, saved.account || {});
        state.draft = Object.assign(base.draft, saved.draft || {});
        state.balances = Object.assign(base.balances, saved.balances || {});
        state.reports = Object.assign(base.reports, saved.reports || {});
        state.toggles = Object.assign(base.toggles, saved.toggles || {});
        state.followings = Object.assign(base.followings, saved.followings || {});
        state.messages = Object.assign(base.messages, saved.messages || {});
        state.deviceRevoked = Object.assign(base.deviceRevoked, saved.deviceRevoked || {});
        state.oauthRevoked = Object.assign(base.oauthRevoked, saved.oauthRevoked || {});
        state.preferences = Object.assign(base.preferences, saved.preferences || {});
        state.ledger = Array.isArray(saved.ledger) ? saved.ledger : base.ledger;
        state.publishedTopics = Array.isArray(saved.publishedTopics) ? saved.publishedTopics : base.publishedTopics;
        state.publishedComments = Array.isArray(saved.publishedComments) ? saved.publishedComments : base.publishedComments;
        state.pendingVerification = saved.pendingVerification || base.pendingVerification;
        state.customNotifications = Array.isArray(saved.customNotifications) ? saved.customNotifications : base.customNotifications;
        state.activeConversation = saved.activeConversation || base.activeConversation;
      } else {
        state = base;
      }
      var legacy = JSON.parse(localStorage.getItem('bblbb-demo-session') || 'null');
      if (legacy && legacy.username === 'admin') state.auth = true;
    } catch (e) {
      state = base;
    }
  }
  function saveState() {
    try { localStorage.setItem(APP_KEY, JSON.stringify(state)); } catch (e) {}
  }
  function pushLedger(kind, amount, source, user, asset) {
    if (!Array.isArray(state.ledger)) state.ledger = [];
    state.ledger.unshift({ kind: kind, amount: amount, source: source || '原型演示', user: user || (state.account && state.account.displayName) || 'Chaos', asset: asset || (kind === 'topic_unlock' ? 'coin' : kind === 'admin_adjust' ? 'coin' : 'exp'), time: new Date().toISOString() });
    state.ledger = state.ledger.slice(0, 50);
  }
  function esc(value) {
    return String(value == null ? '' : value).replace(/[&<>"']/g, function (c) {
      return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c];
    });
  }
  function q(selector, root) { return (root || document).querySelector(selector); }
  function qa(selector, root) { return Array.prototype.slice.call((root || document).querySelectorAll(selector)); }
  function icon(name, cls) { return '<svg class="ic ' + (cls || 'ic-14') + '" aria-hidden="true"><use href="#i-' + esc(name) + '"></use></svg>'; }
  function button(label, cls, attrs) { return '<button type="button" class="btn ' + (cls || 'ghost') + '" ' + (attrs || '') + '>' + label + '</button>'; }
  function link(label, href, cls) { return '<a class="' + (cls || 'app-link') + '" href="' + esc(href) + '">' + label + '</a>'; }
  function profileBadge(name, initial) { return '<span class="avatar cyan profile-hover-trigger" data-profile-name="' + esc(name) + '" tabindex="0" role="button" aria-label="查看 ' + esc(name) + ' 的用户信息" style="width:38px;height:38px;flex-basis:38px;font-size:14px">' + esc(initial || String(name || 'U').charAt(0)) + '</span>'; }

  /* 全站头像资料预览：覆盖原始首页和由运行时挂载的所有论坛路由。 */
  var profileCard;
  var profileCloseTimer;
  var activeProfileTrigger;
  function profileFor(name) {
    name = String(name || '用户');
    var user = USERS.filter(function (item) { return item.name.toLowerCase() === name.toLowerCase(); })[0];
    return user || { name: name, initial: name.charAt(0).toUpperCase(), title: 'BBLBB 社区成员', bio: '正在 BBLBB 社区参与讨论与创作。', level: 'LV.1', contrib: 0 };
  }
  function ensureProfileCard() {
    if (profileCard) return profileCard;
    profileCard = document.createElement('section');
    profileCard.className = 'profile-hover-card';
    profileCard.setAttribute('role', 'dialog');
    profileCard.setAttribute('aria-label', '用户信息');
    profileCard.addEventListener('mouseenter', function () { window.clearTimeout(profileCloseTimer); });
    profileCard.addEventListener('mouseleave', scheduleProfileClose);
    document.body.appendChild(profileCard);
    return profileCard;
  }
  function renderProfileCard(name) {
    var user = profileFor(name);
    var card = ensureProfileCard();
    card.innerHTML = '<div class="profile-hover-card__cover"></div><div class="profile-hover-card__body"><div class="profile-hover-card__avatar">' + esc(user.initial) + '</div><div class="profile-hover-card__head"><strong class="profile-hover-card__name">' + esc(user.name) + '</strong><span class="profile-hover-card__level">' + esc(user.level) + '</span></div><p class="profile-hover-card__role">' + esc(user.title) + '</p><p class="profile-hover-card__bio">' + esc(user.bio) + '</p><div class="profile-hover-card__footer"><span>贡献 ' + esc(user.contrib) + '</span><a class="profile-hover-card__link" href="#user:' + encodeURIComponent(user.name) + '">查看个人主页</a></div></div>';
    return card;
  }
  function showProfileCard(trigger) {
    if (window.matchMedia && window.matchMedia('(max-width: 767px)').matches) return;
    window.clearTimeout(profileCloseTimer);
    activeProfileTrigger = trigger;
    var name = trigger.getAttribute('data-profile-name') || '用户';
    var card = renderProfileCard(name);
    card.classList.add('is-visible');
    var rect = trigger.getBoundingClientRect();
    var width = card.offsetWidth || 316;
    var height = card.offsetHeight || 250;
    var left = Math.max(12, Math.min(rect.left + rect.width / 2 - width / 2, window.innerWidth - width - 12));
    var top = rect.top - height - 10;
    if (top < 12) top = rect.bottom + 10;
    card.style.left = Math.round(left) + 'px';
    card.style.top = Math.round(Math.max(12, Math.min(top, window.innerHeight - height - 12))) + 'px';
  }
  function scheduleProfileClose() {
    window.clearTimeout(profileCloseTimer);
    profileCloseTimer = window.setTimeout(function () { if (profileCard) profileCard.classList.remove('is-visible'); activeProfileTrigger = null; }, 180);
  }
  function inferProfileName(avatar) {
    var explicit = avatar.getAttribute('data-profile-name');
    if (explicit) return explicit;
    var container = avatar.closest('.thread,.follow-row,.comment,.topic-comment,.app-post-row,.app-article-card,.search-user,.conv');
    var label = container && container.querySelector('.thread-meta b,.cm-meta b,.topic-comment__meta b,.follow-row b,.app-post-row b,.app-article-card__meta span,.search-user b,.conv-body b');
    return label ? label.textContent.trim() : avatar.textContent.trim();
  }
  function decorateProfileTriggers(root) {
    qa('.avatar:not(.profile-hover-trigger)', root || document).forEach(function (avatar) {
      var name = inferProfileName(avatar);
      if (!name || name === '站') return;
      avatar.classList.add('profile-hover-trigger');
      avatar.setAttribute('data-profile-name', name);
      avatar.setAttribute('tabindex', '0');
      avatar.setAttribute('role', 'button');
      avatar.setAttribute('aria-label', '查看 ' + name + ' 的用户信息');
    });
  }
  /* Route pages no longer use a visible title block. Keep the title for screen
   * readers and move any action that only lived in the old block into the page
   * content so the control remains available. */
  function removeRouteHeads(root) {
    if (!root) return;
    qa('.app-route-head', root).forEach(function (head) {
      var heading = q('h1', head);
      var actions = q('.app-route-head__actions', head);
      var actionButtons = actions ? qa('button, a', actions) : [];
      var moved = [];
      actionButtons.forEach(function (action) {
        var dataAttr = Array.prototype.slice.call(action.attributes || []).filter(function (attr) { return attr.name.indexOf('data-') === 0; })[0];
        var selector = dataAttr ? '[' + dataAttr.name + ']' : '';
        var duplicate = selector && qa(selector, root).some(function (candidate) { return !head.contains(candidate); });
        if (!duplicate) moved.push(action);
      });
      if (heading) {
        heading.className = 'sr-only';
        heading.setAttribute('tabindex', '-1');
        root.insertBefore(heading, root.firstElementChild);
      }
      if (moved.length) {
        var row = document.createElement('div');
        row.className = 'route-action-row app-toolbar';
        moved.forEach(function (action) { row.appendChild(action); });
        root.insertBefore(row, root.firstElementChild ? root.firstElementChild.nextElementSibling : null);
      }
      head.remove();
    });
  }
  function topicContext(post) {
    return '<nav class="topic-context" aria-label="内容位置"><a href="#home">首页</a><span aria-hidden="true">/</span><a href="#board:' + encodeURIComponent(post.board) + '">' + esc(post.boardName) + '</a><span aria-hidden="true">/</span><span class="topic-context__current">内容详情</span></nav>';
  }
  function relocateTopicChrome(root) {
    if (!root) return;
    var head = q('.topic-head-card', root);
    var body = q('.topic-body-card', root);
    var side = q('.topic-side', root);
    if (!head || !body || !side) return;
    var prose = q('.topic-prose', body);
    if (prose) {
      head.classList.add('topic-reading-card');
      head.appendChild(prose);
      body.remove();
    }
  }
  function card(title, body, foot) {
    return '<section class="app-card"><header class="app-card__head"><h2>' + title + '</h2></header><div class="app-card__body">' + body + '</div>' + (foot ? '<footer class="app-card__foot">' + foot + '</footer>' : '') + '</section>';
  }
  function empty(iconName, title, hint, action) {
    return '<div class="app-empty">' + icon(iconName || 'inbox', 'ic-16') + '<b>' + esc(title) + '</b><span>' + esc(hint) + '</span>' + (action || '') + '</div>';
  }
  function mutationButton(label, cls, attrs) { return button(label, cls, (attrs || '') + ' data-mutation-button'); }
  function stateMarkup(code, title, detail, actions) {
    return '<div class="app-state-card"><div class="app-state-code">' + esc(code) + '</div><h1>' + esc(title) + '</h1><p>' + esc(detail) + '</p><div class="app-state-card__actions">' + (actions || button('返回首页', 'primary', 'data-go-home')) + '</div></div>';
  }
  function ensurePage(id) {
    var el = document.getElementById('page-' + id);
    if (el) return el;
    var host = q('.layout') || document.querySelector('main');
    if (!host) return null;
    el = document.createElement('section');
    /* login 路由在 pages/login.html 模板上声明了 login-page（整高居中认证页），
       SPA 重建容器时需保留，否则卡片会贴在通用内容列左侧。 */
    el.className = 'page app-page' + (id === 'login' ? ' login-page' : '');
    el.id = 'page-' + id;
    el.hidden = true;
    host.appendChild(el);
    return el;
  }
  function ensurePages() { PAGE_IDS.forEach(ensurePage); }
  function mount(id, html) {
    var el = ensurePage(id);
    if (!el) return null;
    el.classList.add('app-page', 'app-route-' + id);
    el.innerHTML = html;
    el.hidden = false;
    return el;
  }
  function hidePages() { qa('.page').forEach(function (el) { el.hidden = true; }); }
  /* ============== 页面模板层：每个路由的标记都拆在 pages/<name>.html ==============
   * pages/*.html 是完整文档：浏览器可直接打开预览，CSS 全站共用；
   * TPL() 首次访问时 fetch 对应文件并抽取 section.page 内容，缓存到 window.__PAGES；
   * mountTpl() 挂载后执行回调（状态同步 + 事件绑定），
   * TPL_TOKEN 保证路由快速切换时过期渲染被丢弃。 */
  var TPL_PROMISES = {};
  var TPL_TOKEN = 0;
  function TPL(name, onReady) {
    var bank = window.__PAGES || (window.__PAGES = {});
    if (typeof bank[name] === 'string') { onReady(bank[name]); return; }
    if (!TPL_PROMISES[name]) {
      TPL_PROMISES[name] = fetch('/pages/' + name + '.html', { cache: 'no-store' }).then(function (res) {
        if (!res.ok) throw new Error('HTTP ' + res.status);
        return res.text();
      }).then(function (text) {
        var parsed = new DOMParser().parseFromString(text, 'text/html');
        var section = parsed.querySelector('section.page');
        if (!section) throw new Error('missing section.page');
        bank[name] = section.innerHTML;
      }).catch(function (err) {
        bank[name] = '<div class="app-notice">页面模板加载失败：pages/' + name + '.html（' + err.message + '），请刷新重试。</div>';
        console.error('[BBLBB] 模板加载失败 ' + name + ': ' + err.message);
      });
    }
    TPL_PROMISES[name].then(function () { onReady(bank[name]); });
  }
  function mountTpl(pageId, name, build) {
    var token = ++TPL_TOKEN;
    var placeholder = ensurePage(pageId);
    if (placeholder && !(window.__PAGES && typeof window.__PAGES[name] === 'string')) {
      placeholder.innerHTML = '<div class="app-skeleton-page" aria-label="页面加载中"><div class="app-skeleton app-skeleton--title"></div><div class="app-skeleton app-skeleton--line"></div><div class="app-skeleton app-skeleton--card"></div><div class="app-skeleton app-skeleton--card app-skeleton--short"></div></div>';
      placeholder.hidden = false;
    }
    TPL(name, function (markup) {
      if (token !== TPL_TOKEN) return;
      mount(pageId, markup);
      if (pageId === 'admin') ensureAdminActions(placeholder);
      if (build) build();
      removeRouteHeads(placeholder);
      setupAdminEnhancements();
      syncBellBadge();
      decorateProfileTriggers(q('.page:not([hidden])') || document);
      window.scrollTo(0, 0);
      var heading = q('.page:not([hidden]) h1');
      if (heading) { if (!heading.hasAttribute('tabindex')) heading.setAttribute('tabindex', '-1'); heading.focus({ preventScroll: true }); }
    });
  }
  function fillAdminTbody(selector, rows) {
    var host = q(selector);
    if (!host) return;
    var tbody = q('tbody', host);
    if (tbody) tbody.innerHTML = Array.isArray(rows) ? rows.join('') : rows;
  }
  function setDocumentTitle(title) { document.title = title ? title + ' · BBLBB' : 'BBLBB 社区'; }
  function toast(message) {
    var el = q('#toast');
    if (!el) return;
    el.textContent = message;
    el.classList.add('show');
    window.clearTimeout(toast.timer);
    toast.timer = window.setTimeout(function () { el.classList.remove('show'); }, 2200);
  }
  var modalOpener = null;
  function modalFocusable(root) { return qa('a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])', root).filter(function (el) { return !el.hidden && el.getClientRects().length; }); }
  function closeModal() { var modal = q('.app-modal-backdrop'); if (!modal) return; modal.remove(); document.body.classList.remove('app-modal-open'); var opener = modalOpener; modalOpener = null; if (opener && opener.isConnected && typeof opener.focus === 'function') opener.focus(); }
  function openModal(title, body, foot, afterOpen) {
    closeModal();
    modalOpener = document.activeElement;
    var backdrop = document.createElement('div');
    backdrop.className = 'app-modal-backdrop';
    backdrop.setAttribute('role', 'presentation');
    backdrop.innerHTML = '<div class="app-modal" role="dialog" aria-modal="true" aria-labelledby="app-modal-title" tabindex="-1"><header class="app-modal__head"><h2 id="app-modal-title">' + esc(title) + '</h2><button type="button" class="app-modal__close" aria-label="关闭" data-modal-close>' + icon('x') + '</button></header><div class="app-modal__body">' + body + '</div><footer class="app-modal__foot">' + (foot || button('关闭', 'ghost', 'data-modal-close')) + '</footer></div>';
    document.body.appendChild(backdrop);
    document.body.classList.add('app-modal-open');
    qa('[data-modal-close]', backdrop).forEach(function (b) { b.addEventListener('click', closeModal); });
    backdrop.addEventListener('click', function (event) { if (event.target === backdrop) closeModal(); });
    backdrop.addEventListener('keydown', function (event) {
      if (event.key === 'Escape') { event.preventDefault(); closeModal(); return; }
      if (event.key === 'Tab') { var items = modalFocusable(backdrop); if (!items.length) { event.preventDefault(); return; } var first = items[0], last = items[items.length - 1]; if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); } else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); } }
    });
    var focusable = modalFocusable(backdrop)[0] || q('.app-modal', backdrop);
    if (focusable) focusable.focus();
    if (afterOpen) afterOpen(backdrop);
    return backdrop;
  }
  function actionPending(btn, pendingText, done, fail) {
    if (!btn || btn.disabled) return;
    var old = btn.textContent;
    btn.disabled = true;
    btn.setAttribute('aria-busy', 'true');
    btn.textContent = pendingText || '处理中…';
    window.setTimeout(function () {
      try {
        if (fail && fail()) { if (btn.isConnected) { btn.disabled = false; btn.removeAttribute('aria-busy'); btn.textContent = old; } return; }
        if (done) done();
      } finally {
        if (btn.isConnected) { btn.disabled = false; btn.removeAttribute('aria-busy'); btn.textContent = old; }
      }
    }, 450);
  }
  function routeKey(raw) {
    var value = String(raw || '#home');
    if (value.charAt(0) === '#') value = value.slice(1);
    var parts = value.split(':');
    var base = parts.shift() || 'home';
    var rest = parts.join(':');
    // 链接可能来自已编码的 href，也可能来自复制后的二次编码值；
    // 最多解码两次，避免把普通文本误处理，同时保证中文标签可读。
    for (var i = 0; i < 2 && /%[0-9a-f]{2}/i.test(rest); i += 1) {
      try {
        var decoded = decodeURIComponent(rest);
        if (decoded === rest) break;
        rest = decoded;
      } catch (e) {
        break;
      }
    }
    return { base: base, rest: rest, raw: '#' + value };
  }
  function userByName(name) { return USERS.find(function (u) { return u.name.toLowerCase() === String(name || '').toLowerCase(); }) || { name: String(name || '匿名'), initial: String(name || 'U').charAt(0).toUpperCase(), title: '社区成员', bio: '这位成员还没有填写简介。', level: 'LV.2', coin: 0, contrib: 0, exp: 850, posts: 5, followers: 12, following: 20, joined: '2026-03-01' }; }
  function topicForKey(key) { if (TOPICS[key]) return key; if (Array.isArray(state.publishedTopics) && state.publishedTopics.some(function (item) { return item.id === key; })) return key; return null; }
  function isLoggedIn() { return !!state.auth; }
  function requireAuth(target) {
    if (state.auth) return true;
    state.returnTo = target;
    saveState();
    try { localStorage.setItem('returnTo', target); sessionStorage.setItem('returnTo', target); } catch (e) {}
    go('#login');
    toast('请先登录后继续');
    return false;
  }
  function requireAdmin(target) {
    if (!requireAuth(target)) return false;
    if (state.role === 'admin') return true;
    go('#403');
    return false;
  }
  function returnTarget() {
    var target = '';
    try { target = localStorage.getItem('returnTo') || sessionStorage.getItem('returnTo') || ''; } catch (e) {}
    return target || state.returnTo || '#home';
  }
  function updateUserChrome() {
    qa('.desktop-nav button').forEach(function (el) { if (el.textContent.trim() === '文章') el.childNodes[el.childNodes.length - 1].textContent = '内容'; });
    var displayName = state.auth ? ((state.account && state.account.displayName) || 'admin') : '登录';
    var name = q('#header-username');
    var avatar = q('#header-avatar');
    if (name) name.textContent = displayName;
    if (avatar) avatar.textContent = state.auth ? String(displayName.charAt(0)).toUpperCase() : '↗';
    qa('[data-auth-required]').forEach(function (el) { el.hidden = !state.auth; });
    qa('[data-auth-action="login"]').forEach(function (el) { el.hidden = state.auth; });
    qa('[data-auth-action="logout"]').forEach(function (el) { el.hidden = !state.auth; });
    qa('[data-guest-hide]').forEach(function (el) { el.hidden = !state.auth; });
    qa('.desktop-nav .nav-dot').forEach(function (el) { el.hidden = !state.auth; });
    qa('.desktop-nav > a[data-route], .bottom-nav a[data-route]').forEach(function (el) { var href = el.getAttribute('href') || ''; el.classList.toggle('active', href === (location.hash || '#home')); });
    var bottom = q('.bottom-nav a[href="#me"]');
    if (bottom) bottom.setAttribute('aria-label', state.auth ? '我的' : '登录');
  }
  function like(button, initial) { return legacyLike(button, initial); }
  function legacyLike(button, initial) {
    if (!button) return;
    var holder = button.querySelector('em');
    var currentText = holder ? holder.textContent : ((button.textContent || '').match(/\d+/) || ['0'])[0];
    var current = Number.parseInt(currentText, 10) || 0;
    var active = button.classList.toggle('liked');
    var use = button.querySelector('use');
    if (use) use.setAttribute('href', active ? '#i-heart-fill' : '#i-heart');
    if (holder) holder.textContent = String(Math.max(0, current + (active ? 1 : -1)));
    else if (button.lastChild && button.lastChild.nodeType === 3) button.lastChild.textContent = ' ' + Math.max(0, current + (active ? 1 : -1));
    toast(active ? '已点赞' : '已取消点赞');
  }
  function legacyPickCat(event, anchor) {
    if (event) event.preventDefault();
    if (!anchor) return;
    var group = anchor.closest('.category-card');
    if (group) qa('a', group).forEach(function (item) { item.classList.toggle('selected', item === anchor); });
    var label = (anchor.textContent || '').replace(/\s+/g, '').charAt(0);
    var kind = { '技': 'tech', '产': 'product', '站': 'notice', '闲': 'life' }[label] || 'all';
    qa('#page-home #thread-list .thread').forEach(function (row) { row.hidden = kind !== 'all' && row.getAttribute('data-cat') !== kind; });
    var end = q('#home-end');
    if (end) end.textContent = kind === 'all' ? '— 已经到底了 —' : '该分类暂无更多内容';
  }
  function legacyPickFilter(event) {
    if (event) event.preventDefault();
    var button = event && event.currentTarget;
    var group = button && button.parentElement;
    if (group) qa('button', group).forEach(function (item) { item.classList.toggle('active', item === button); });
  }
  function legacyCycleSort(button) {
    var labels = ['最新', '热门', '精华'];
    var current = button && button.getAttribute('data-sort-label') || '最新';
    var next = labels[(labels.indexOf(current) + 1) % labels.length];
    if (button) { button.setAttribute('data-sort-label', next); button.childNodes[0].textContent = next; }
    if (next === '热门') applyLegacySort('data-hot');
    else if (next === '精华') applyLegacySort('data-ess');
    else applyLegacySort('data-time');
    toast('已按 ' + next + ' 排序');
  }
  function applyLegacySort(attribute) {
    var list = q('#page-home #thread-list');
    if (!list) return;
    qa('.thread', list).sort(function (a, b) { return (Number(b.getAttribute(attribute)) || 0) - (Number(a.getAttribute(attribute)) || 0); }).forEach(function (row) { list.appendChild(row); });
  }
  function legacyShuffleDiscover() {
    var list = q('#page-discover .thread-list');
    if (!list) return;
    qa('.d-thread', list).sort(function () { return Math.random() - 0.5; }).forEach(function (row) { list.appendChild(row); });
    toast('已换一批内容');
  }
  function legacyFilterTag(button) {
    var tag = button && button.getAttribute('data-tag');
    qa('#page-discover .tag-cloud button').forEach(function (item) { item.classList.toggle('on', item === button); });
    qa('#page-discover .d-thread').forEach(function (row) { row.hidden = !!tag && (row.getAttribute('data-tags') || '').toLowerCase().split(',').indexOf(String(tag).toLowerCase()) < 0; });
    toast(tag ? '标签筛选：' + tag : '已清除筛选');
  }
  function legacyLoadMore() {
    var button = q('#load-more');
    if (!button || button.disabled) return;
    button.click();
  }
  function legacyOpenConv(button, name, message) {
    if (!requireAuth('#messages')) return;
    state.activeConversation = name || 'Lin';
    saveState();
    go('#messages');
  }
  function legacyAddBubble(message, who) {
    var body = q('[data-chat-body]');
    if (!body) return;
    var bubble = document.createElement('div');
    bubble.className = 'bubble ' + (who === 'me' ? 'me' : 'them');
    bubble.textContent = message || '';
    body.appendChild(bubble);
  }
  function legacySendMsg() {
    var input = q('[data-chat-input]');
    if (!input) return;
    var value = input.value.trim();
    if (!value) return;
    var name = state.activeConversation || 'Lin';
    if (!Array.isArray(state.messages[name])) state.messages[name] = [];
    state.messages[name].push({ id: 'message-' + Date.now(), text: value, failed: false });
    saveState();
    legacyAddBubble(value, 'me');
    input.value = '';
    toast('消息已发送（原型演示）');
  }
  function legacyUpdateThreadDetail(key) {
    var target = key || 'welcome';
    go('#thread:' + encodeURIComponent(target));
  }
  function startLoading() {
    if (location.hash !== '#loading') return;
    var page = q('#page-loading');
    if (!page) return;
    page.classList.remove('is-ready');
    window.requestAnimationFrame(function () { page.classList.add('is-ready'); });
  }
  function legacyReplySubmit() {
    if (!isLoggedIn()) { requireAuth(location.hash || '#topic:welcome'); return; }
    var textarea = q('#reply-text');
    if (!textarea || !textarea.value.trim()) { if (textarea) textarea.classList.add('field-error'); toast('回复内容不能为空'); return; }
    toast('回复已加入演示列表');
  }
  function legacyUnlockRestricted(button) {
    if (!isLoggedIn()) { requireAuth(location.hash || '#topic:welcome'); return; }
    var restricted = button && button.closest('.restricted');
    if (restricted) restricted.hidden = true;
    var peek = q('.restricted-peek');
    if (peek) peek.hidden = false;
    toast('受限内容已解锁（原型演示）');
  }
  function legacyCopyCode(button) {
    var code = button && button.closest('.code-block') && button.closest('.code-block').querySelector('pre');
    var value = code ? code.textContent : '';
    if (navigator.clipboard && navigator.clipboard.writeText) navigator.clipboard.writeText(value).catch(function () {});
    toast('代码已复制');
  }
  function closeDrops() {
    qa('.dropdown').forEach(function (menu) { menu.classList.remove('open'); });
    var chip = q('#user-chip'), more = q('.nav-more-btn');
    if (chip) chip.setAttribute('aria-expanded', 'false');
    if (more) more.setAttribute('aria-expanded', 'false');
  }
  function syncBellBadge() {
    var count = 0;
    var list = q('#page-notifications [data-notif-list]');
    if (state.auth) {
      if (list) {
        count = qa('.app-post-row.notif-unread', list).length;
      } else if (state.notificationsRead === false) {
        count = 2;
        var custom = Array.isArray(state.customNotifications) ? state.customNotifications : [];
        custom.forEach(function (c) { if (c && c.unread) count += 1; });
      }
    }
    var badge = document.getElementById('bell-badge');
    if (badge) {
      badge.textContent = String(count);
      badge.hidden = count === 0;
      badge.setAttribute('aria-label', count ? count + ' 条未读通知' : '没有未读通知');
    }
    var ddCount = document.getElementById('dd-bell-count');
    if (ddCount) ddCount.textContent = count ? '· ' + count + ' 条未读' : '· 暂无未读';
    var ddRead = q('#dd-bell [onclick*="markAllRead"]');
    if (ddRead) { ddRead.disabled = count === 0; ddRead.textContent = count ? '全部已读' : '已全部读完'; }
    qa('[data-read-all]').forEach(function (b) { b.disabled = count === 0; });
  }
  function markAllRead() {
    var dynamic = q('[data-read-all]:visible');
    if (dynamic) { dynamic.click(); return; }
    var staticItems = qa('#page-notifications .notif-item.unread');
    staticItems.forEach(function (item) { item.classList.remove('unread'); var dot = q('.n-dot', item); if (dot) dot.remove(); });
    try { localStorage.setItem('bblbb-notification-read', JSON.stringify(staticItems.map(function (item) { return item.getAttribute('data-id'); }).filter(Boolean))); } catch (e) {}
    state.notificationsRead = true;
    if (Array.isArray(state.customNotifications)) state.customNotifications.forEach(function (c) { c.unread = false; });
    saveState();
    if (typeof window.updateNotifCounts === 'function') window.updateNotifCounts();
    syncBellBadge();
    toast('通知已全部标记为已读');
  }
  function switchTab(tab) {
    if (tab === 'fav') { go('#me'); window.setTimeout(function () { var item = q('[data-user-tab="favorites"]'); if (item) item.click(); }, 0); }
    else if (tab === 'set') go('#settings:profile');
    else go('#me');
  }
  /* 草稿已并入「我的」页的草稿 tab；旧 #drafts 路由与草稿箱入口统一跳到这里 */
  function goDrafts() { go('#me'); window.setTimeout(function () { var t = q('[data-user-tab="drafts"]'); if (t) t.click(); }, 0); }
  function logout() {
    openModal('退出登录', '<p>退出后本设备的演示会话将被清除。</p>', '取消 ' + button('确认退出', 'danger', 'data-confirm-logout'));
    q('[data-confirm-logout]').addEventListener('click', function () { state.auth = false; state.role = 'member'; state.returnTo = ''; saveState(); try { localStorage.removeItem('returnTo'); sessionStorage.removeItem('returnTo'); } catch (e) {} closeModal(); go('#login'); toast('已退出登录'); });
  }
  function bindLegacyChrome() {
    var userChip = q('#user-chip');
    if (userChip) userChip.onclick = function (event) { event.preventDefault(); event.stopPropagation(); var menu = q('#dd-user'); if (menu) menu.classList.toggle('open'); };
    var more = q('.nav-more-btn');
    if (more) more.onclick = function (event) { event.preventDefault(); event.stopPropagation(); var menu = q('#dd-more'); if (menu) menu.classList.toggle('open'); };
    var bell = q('.icon-btn[aria-label="通知"]');
    if (bell) bell.onclick = function (event) { event.preventDefault(); event.stopPropagation(); var menu = q('#dd-bell'); if (menu) menu.classList.toggle('open'); };
    var themeButtons = qa('.theme-toggle, .mobile-theme');
    themeButtons.forEach(function (buttonEl) { buttonEl.onclick = function (event) { event.preventDefault(); window.toggleTheme(); }; });
    var publishButtons = qa('.publish, .bottom-nav button[aria-label="发布"]');
    publishButtons.forEach(function (buttonEl) { buttonEl.onclick = function (event) { event.preventDefault(); window.openPublish(); }; });
  }

  function bindHomeInteractions() {
    var page = q('#page-home');
    if (!page || page.__completionBound) return;
    page.__completionBound = true;
    qa('.hero-stats button', page).forEach(function (b, i) { b.onclick = function () { if (i === 2) { try { navigator.clipboard.writeText(location.href); } catch (e) {} toast('首页链接已复制'); } else toast(i === 0 ? '当前社区共有 128 名成员' : '当前已有 56 条内容'); }; });
    qa('.mobile-categories button', page).forEach(function (b, i) { b.onclick = function () { qa('.mobile-categories button', page).forEach(function (x) { x.classList.remove('active'); }); b.classList.add('active'); if (i === 2) page.classList.toggle('show-categories'); else { qa('.category-card a', page).forEach(function (x) { x.classList.toggle('selected', i === 0 ? x.getAttribute('href') === '#all' : false); }); } }; });
    qa('.filters button', page).forEach(function (b, i) { b.onclick = function () { qa('.filters button', page).forEach(function (x) { x.classList.remove('active'); }); b.classList.add('active'); if (i < 3) { var kind = i === 0 ? 'all' : i === 1 ? 'featured' : 'followed'; qa('#thread-list .thread', page).forEach(function (row) { row.hidden = kind === 'featured' ? !row.classList.contains('featured') : kind === 'followed' ? row.getAttribute('data-followed') !== 'true' : false; }); } else { toast('已切换排序：最新'); } }; });
    var load = q('#load-more', page);
    if (load) load.onclick = function () { if (load.dataset.loading === 'true' || load.disabled) return; load.dataset.loading = 'true'; load.setAttribute('aria-busy', 'true'); load.textContent = '加载中…'; var existing = qa('#thread-list .thread', page).length; var extra = [{ id: 'load-wasm', title: 'Wasm 边缘计算实践：把过滤器跑进 CDN', author: 'Ken', type: 'article', cat: 'tech', body: '用 Wasm 把内容过滤逻辑下沉到边缘节点。' }, { id: 'load-notif', title: '通知偏好设置的可用性走查', author: 'Sara', type: 'topic', cat: 'product', body: '把 12 个开关按打扰强度重新分组。' }, { id: 'load-design', title: '你删除过自己最满意的设计吗？', author: 'Zoe', type: 'topic', cat: 'life', body: '聊聊你们忍痛删掉过的得意之作。' }]; var item = extra[(existing - 3) % extra.length]; var row = document.createElement('article'); row.className = 'thread'; row.setAttribute('data-post-key', item.id); row.setAttribute('data-post-type', item.type); row.setAttribute('data-cat', item.cat); var avatar = document.createElement('div'); avatar.className = 'avatar cyan'; avatar.textContent = item.author.charAt(0); var content = document.createElement('div'); var meta = document.createElement('div'); meta.className = 'thread-meta'; var author = document.createElement('b'); author.textContent = item.author; var time = document.createElement('span'); time.textContent = '· 刚刚'; meta.appendChild(author); meta.appendChild(time); var detail = document.createElement('a'); detail.className = 'thread-detail-link'; detail.href = '#topic:' + encodeURIComponent(item.id); detail.setAttribute('aria-label', '查看帖子：' + item.title); var heading = document.createElement('h2'); heading.textContent = item.title; var summary = document.createElement('p'); summary.textContent = item.body; detail.appendChild(heading); detail.appendChild(summary); var footer = document.createElement('div'); footer.className = 'thread-footer'; var category = document.createElement('span'); category.textContent = item.cat; var replies = document.createElement('span'); replies.textContent = '0 回复'; footer.appendChild(category); footer.appendChild(replies); content.appendChild(meta); content.appendChild(detail); content.appendChild(footer); row.appendChild(avatar); row.appendChild(content); q('#thread-list', page).appendChild(row); setTimeout(function () { var exhausted = existing >= 5; load.dataset.loading = 'false'; load.removeAttribute('aria-busy'); load.textContent = '加载更多'; load.disabled = exhausted; load.hidden = exhausted; var end = q('#home-end', page); if (end) end.hidden = !exhausted; toast('已加载更多内容'); }, 380); };
  }
  function ensureHomeDataAttributes() {
    var page = q('#page-home');
    if (!page) return;
    qa('#thread-list .thread', page).forEach(function (row) {
      if (!row.getAttribute('data-post-type')) row.setAttribute('data-post-type', row.classList.contains('featured') ? 'article' : 'topic');
    });
  }
  function enhanceHome() {
    var page = q('#page-home');
    if (!page) return;
    ensureHomeDataAttributes();
    bindHomeInteractions();
    if (!q('h1', page)) {
      var title = document.createElement('h1');
      title.className = 'sr-only';
      title.textContent = '首页';
      page.insertBefore(title, page.firstChild);
    }
    var categoryMap = ['#home', '#board:tech-essay', '#board:web-dev', '#board:meta', '#board:chat'];
    qa('.category-card a', page).forEach(function (a, index) {
      a.removeAttribute('onclick');
      a.setAttribute('href', categoryMap[index] || '#articles');
      a.onclick = function (event) { event.preventDefault(); go(a.getAttribute('href')); };
    });
    qa('[data-post-key]', page).forEach(function (article) {
      var key = article.getAttribute('data-post-key');
      var resolvedKey = topicForKey(key);
      var href = resolvedKey ? '#topic:' + encodeURIComponent(resolvedKey) : '#404';
      var likeButton = q('.act-like', article);
      if (likeButton) { likeButton.onclick = function (event) { event.preventDefault(); event.stopPropagation(); legacyLike(likeButton); }; }
      var detail = q('.thread-detail-link', article);
      if (detail) { detail.setAttribute('href', href); detail.onclick = function (event) { event.preventDefault(); go(href); }; }
      var comment = q('.thread-comment-link', article);
      if (comment) { comment.setAttribute('href', href); comment.onclick = function (event) { event.preventDefault(); event.stopPropagation(); go(href); }; }
    });
  }
  function bindDiscoverInteractions(page) {
    if (!page || page.__discoverBound) return;
    page.__discoverBound = true;
    qa('.filters button', page).forEach(function (tab) { tab.onclick = function () { qa('.filters button', page).forEach(function (x) { x.classList.remove('active'); }); tab.classList.add('active'); var label = tab.textContent.trim(); var rows = qa('.d-thread', page); rows.forEach(function (row, index) { row.hidden = label === '精华' ? !row.classList.contains('featured') : label === '最新' ? index > 1 : false; }); }; });
    qa('.tag-cloud button', page).forEach(function (tag) { tag.onclick = function () { var wanted = tag.getAttribute('data-tag'); qa('.tag-cloud button', page).forEach(function (x) { x.classList.toggle('on', x === tag); }); qa('.d-thread', page).forEach(function (row) { row.hidden = !!wanted && (row.getAttribute('data-tags') || '').toLowerCase().split(',').indexOf(wanted.toLowerCase()) < 0; }); }; });
  }
  function enhanceDiscover() {
    var page = q('#page-discover');
    if (!page) return;
    bindDiscoverInteractions(page);
    qa('.side-hot a', page).forEach(function (a, i) {
      var tags = ['性能优化', '开源项目', 'AI 工具链', '设计系统'];
      a.removeAttribute('onclick');
      a.setAttribute('href', '#tag:' + encodeURIComponent(tags[i] || tags[0]));
      a.onclick = function (event) { event.preventDefault(); go(a.getAttribute('href')); };
    });
    qa('.tag-cloud button', page).forEach(function (b) {
      b.onclick = function (event) { event.preventDefault(); var tag = b.getAttribute('data-tag') || b.textContent.trim(); go('#tag:' + encodeURIComponent(tag)); };
    });
  }

  function renderArticles() {
    var allContent = Object.keys(TOPICS).map(function (id) { return TOPICS[id]; }).concat((state.publishedTopics || []).map(function (item) { return Object.assign({ id: item.id, author: 'Chaos', boardName: item.boardName || '板块', views: item.views || '0', body: item.body || '', tags: [] }, item); })).filter(function (item, index, items) { return items.findIndex(function (candidate) { return candidate.id === item.id; }) === index; });
    mountTpl('articles', 'articles', function () {
    var routeTitle = q('.app-route-head h1'); if (routeTitle) routeTitle.textContent = '内容';
    var routeLede = q('.app-route-head p'); if (routeLede) routeLede.textContent = '浏览社区中的全部内容';
    qa('[data-go-publish]').forEach(function (b) { b.textContent = '发布内容'; });
    var listHost = q('[data-article-list]');
    if (listHost) listHost.innerHTML = allContent.map(function (item) { return '<a class="app-article-card" href="#topic:' + encodeURIComponent(item.id) + '" data-article-board-name="' + esc(item.board) + '"><div class="app-article-card__cover"></div><span class="sbadge sb-brand">内容</span><h3>' + esc(item.title) + '</h3><p>' + esc(item.summary || item.body || '') + '</p><div class="app-article-card__meta">' + profileBadge(item.author) + '<span>' + esc(item.author) + ' · ' + esc(item.views || '0') + ' 阅读</span></div><span class="sr-only">板块：' + esc(item.boardName || item.board) + '</span></a>'; }).join('');
    var input = q('[data-article-search]');
    var select = q('[data-article-board]');
    function filter() {
      var needle = (input.value || '').toLowerCase();
      var board = (select.value || '');
      var visible = 0;
      qa('[data-article-list] .app-article-card').forEach(function (row) {
        var ok = (!needle || row.textContent.toLowerCase().indexOf(needle) >= 0) && (!board || row.getAttribute('data-article-board-name') === board);
        row.hidden = !ok;
        if (ok) visible += 1;
      });
      q('[data-article-empty]').hidden = visible > 0;
    }
    if (input) input.addEventListener('input', filter);
    if (select) select.addEventListener('change', filter);
    qa('[data-article-reset]').forEach(function (b) { b.addEventListener('click', function () { input.value = ''; select.value = ''; filter(); }); });
    });
}
  function renderBoards() {
    
    mountTpl('boards', 'boards', function () {

    });
}
  function renderBoard(slug) {
    var board = BOARDS.find(function (b) { return b.slug === slug; }) || BOARDS[0];
    
    
    mountTpl('board', 'board', function () {
      var boardKicker = q('.app-route-head .app-kicker'); if (boardKicker) boardKicker.textContent = 'COMMUNITY / ' + board.slug.toUpperCase();
      var boardH1 = q('.app-route-head h1'); if (boardH1) boardH1.textContent = board.name;
      var boardLede = q('.app-route-head p'); if (boardLede) boardLede.textContent = board.desc;
      qa('[data-go-publish]').forEach(function (b) { b.textContent = '发布内容'; });
      qa('[data-board-list] .app-post-row').forEach(function (r) { r.hidden = r.getAttribute('data-post-board') !== board.slug; });
      var boardInfo = q('#page-board aside .app-card__body');
      if (boardInfo) boardInfo.innerHTML = '<p class="app-muted">版主：<a class="app-link" href="#user:' + encodeURIComponent(board.mod || 'Chaos') + '">' + esc(board.mod || 'Chaos') + '</a></p><p class="app-muted">' + esc(board.today) + ' · ' + esc(board.count) + '</p>';

    var search = q('[data-board-search]');
    function filter() {
      var needle = (search.value || '').toLowerCase();
      var visible = 0;
      qa('[data-board-list] .app-post-row').forEach(function (row) { if (row.getAttribute('data-post-board') !== board.slug) { row.hidden = true; return; } var ok = !needle || row.textContent.toLowerCase().indexOf(needle) >= 0; row.hidden = !ok; if (ok) visible += 1; });
      q('[data-board-empty]').hidden = visible > 0;
    }
    if (search) { search.addEventListener('input', filter); search.addEventListener('keydown', function (event) { if (event.key === 'Enter') filter(); }); }
    qa('[data-board-clear]').forEach(function (b) { b.addEventListener('click', function () { search.value = ''; filter(); }); });
    var boardTablist = q('.app-filter-tabs'); if (boardTablist) boardTablist.setAttribute('role', 'tablist');
    qa('[data-board-tab]').forEach(function (tab) { tab.setAttribute('role', 'tab'); tab.setAttribute('aria-selected', String(tab.classList.contains('is-active'))); tab.addEventListener('click', function () { qa('[data-board-tab]').forEach(function (x) { x.classList.remove('is-active'); x.setAttribute('aria-selected', 'false'); }); tab.classList.add('is-active'); tab.setAttribute('aria-selected', 'true'); var mode = tab.getAttribute('data-board-tab'); var rows = qa('[data-board-list] .app-post-row'); if (mode === 'unanswered') { rows.forEach(function (r) { r.hidden = true; }); q('[data-board-empty]').hidden = false; } else { rows.forEach(function (r) { r.hidden = r.getAttribute('data-post-board') !== board.slug; }); filter(); if (mode === 'hot') { var viewNum = function (v) { v = String(v || '').trim(); var m = v.match(/([\d.]+)\s*k/i); return m ? parseFloat(m[1]) * 1000 : (parseFloat(v) || 0); }; var host = q('[data-board-list]'); rows.filter(function (r) { return !r.hidden; }).sort(function (a, b) { return viewNum(b.querySelector('.app-post-row__right b')?.textContent) - viewNum(a.querySelector('.app-post-row__right b')?.textContent); }).forEach(function (r) { host.appendChild(r); }); } } }); });
    var boardFollow = q('[data-board-follow]'); if (boardFollow) boardFollow.addEventListener('click', function () { if (!requireAuth('#board:' + board.slug)) return; var key = 'board:' + board.slug; state.followings[key] = !state.followings[key]; this.textContent = state.followings[key] ? '已关注' : '关注板块'; saveState(); toast(state.followings[key] ? '已关注板块' : '已取消关注'); });
    });
}
  function renderTags(tag) {
    if (tag) {
      var rows = ARTICLES.concat([{ id: '201', title: TOPICS['201'].title, summary: TOPICS['201'].body, author: 'Chaos', board: 'web-dev', tags: 'SvelteKit / 自托管', views: '2.1k' }]).filter(function (item) { return (item.tags + ' ' + item.title).toLowerCase().indexOf(tag.toLowerCase()) >= 0; }).map(function (item) { return '<a class="app-post-row" href="#topic:' + item.id + '">' + profileBadge(item.author) + '<span class="app-post-row__main"><h3>' + esc(item.title) + '</h3><p>' + esc(item.summary) + '</p><span class="app-post-row__meta"># ' + esc(tag) + ' · ' + esc(item.board) + '</span></span></a>'; }).join('');
      mountTpl('tag', 'tag', function () {
        var tagH1 = q('.app-route-head h1'); if (tagH1) tagH1.textContent = '标签：' + tag;
        var tagLede = q('.app-route-head p'); if (tagLede) tagLede.textContent = '聚合所有包含该标签的内容';
        var tagList = q('.app-post-list'); if (tagList) tagList.innerHTML = rows || empty('tag', '还没有相关内容', '去发布第一篇带有该标签的内容', button('发布内容', 'primary', 'data-go-publish="topic"'));
      });
      return;
    }
    mountTpl('tags', 'tags', function () {});
  }


  function seedComments(id) {
    if (state.comments[id]) return state.comments[id];
    state.comments[id] = [
      { author: 'Yuwen', text: '同意这条实践，建议补充回滚路径。', time: '5 小时前', likes: 8, depth: 0 },
      { author: 'Mark', text: '有没有完整的压测数据？', time: '2 小时前', likes: 3, depth: 0 },
      { author: 'Nina', text: '这个方案对小团队很有帮助。', time: '昨天', likes: 5, depth: 1 }
    ];
    return state.comments[id];
  }
  function renderTopic(id) {
    var post = TOPICS[id];
    if (!post && (state.publishedTopics || []).some(function (item) { return item.id === id; })) {
      var ownPost = (state.publishedTopics || []).find(function (item) { return item.id === id; });
      post = { id: ownPost.id, type: 'content', title: ownPost.title, author: 'admin', board: ownPost.board || 'tech-essay', boardName: (BOARDS.find(function (b) { return b.slug === ownPost.board; }) || BOARDS[0]).name, tags: String(ownPost.tags || '').split(',').map(function (x) { return x.trim(); }).filter(Boolean), views: '0', body: ownPost.visibility && ownPost.visibility !== 'public' ? (ownPost.summary || '该内容包含受限正文，请按提示解锁后查看。') : (ownPost.body || ''), restrictedBody: ownPost.visibility && ownPost.visibility !== 'public' ? (ownPost.body || '') : '', paidPrice: Number(ownPost.paidPrice) || 10, restricted: ownPost.visibility !== 'public' ? ownPost.visibility : '', locked: ownPost.visibility === 'paid' ? '支付后解锁' : '回复后可见' };
    }
    if (!post) {
      mount('topic', stateMarkup('404', '主题不存在', '这条内容可能已被删除，或链接已经失效。', button('返回首页', 'primary', 'data-go-home') + button('去搜索', 'ghost', 'data-go-search')));
      setDocumentTitle('主题不存在');
      return;
    }
    var unlocked = !!state.topicUnlocked[id];
    var comments = seedComments(id);
    (state.publishedComments || []).filter(function (comment) { return comment.topicId === id; }).forEach(function (comment) { comments.push(comment); });
    var favorite = state.favorites.indexOf(id) >= 0;
    var restricted = post.restricted && !unlocked;
    var commentsHtml = comments.map(function (comment, index) {
      return '<article class="topic-comment" style="margin-left:' + (comment.depth ? '22px' : '0') + '"><div>' + profileBadge(comment.author) + '</div><div class="topic-comment__body"><div class="topic-comment__meta"><b>' + esc(comment.author) + '</b><span>' + esc(userByName(comment.author).level) + '</span><span>' + esc(comment.time) + '</span></div><p>' + esc(comment.text) + '</p><div class="topic-comment__actions">' + button('赞 ' + comment.likes, 'ghost', 'data-comment-like aria-label="赞 ' + esc(comment.author) + ' 的回复"') + button('回复', 'ghost', 'data-comment-reply aria-label="回复 ' + esc(comment.author) + '"') + button('引用', 'ghost', 'data-comment-quote="' + esc(comment.text) + '" aria-label="引用 ' + esc(comment.author) + ' 的回复"') + button('举报', 'ghost', 'data-comment-report aria-label="举报 ' + esc(comment.author) + ' 的回复"') + '</div></div></article>';
    }).join('');
    var restrictedHtml = restricted ? '<aside class="topic-restricted" data-restricted><span class="topic-restricted__icon">' + icon(post.restricted === 'paid' ? 'coins' : 'lock') + '</span><div class="topic-restricted__body"><h3>' + esc(post.locked) + '</h3><p>解锁前正文不会进入页面 DOM；服务端授权通过后才会渲染完整内容。</p><div class="topic-restricted__actions">' + (post.restricted === 'paid' ? button('支付 10 B币解锁', 'primary', 'data-paid-unlock') : button('去回复', 'primary', 'data-focus-reply')) + button('订阅解锁通知', 'ghost', 'data-subscribe-unlock') + '</div></div></aside>' : '';
    var unlockedHtml = unlocked ? '<section class="topic-prose" data-unlocked-content><h2>解锁后的完整内容</h2><p>' + esc(post.restrictedBody || '这里展示服务端授权后返回的额外段落：包括具体配置、失败回退和可观测性验证。') + '</p><ul><li>授权结果与缓存键按用户身份隔离。</li><li>支付与回复事件写入不可变流水。</li><li>失败时保留操作上下文并允许重试。</li></ul></section>' : '';
    var board = BOARDS.find(function (b) { return b.slug === post.board; }) || BOARDS[0];
    
    var topicAuthorCard = '<div class="topic-side-author"><div class="topic-side-author__avatar">' + profileBadge(post.author) + '</div><div class="topic-side-author__copy"><strong>' + link(post.author, '#user:' + encodeURIComponent(post.author)) + '</strong><span>&nbsp;' + esc(userByName(post.author).level) + ' · 内容 ' + (userByName(post.author).posts || 47) + ' · 回复 ' + (userByName(post.author).followers || 142) + (userByName(post.author).role ? ' · <span class="role-badge ' + (userByName(post.author).role === '站长' ? 'role-admin' : 'role-mod') + '">' + esc(userByName(post.author).role) + '</span>' : '') + '</span></div></div>';
    var topicActionCard = '<div class="topic-actions topic-actions--side">' + button(favorite ? '已收藏' : '收藏', 'ghost', 'data-topic-favorite aria-pressed="' + String(favorite) + '"') + button('赞 14', 'ghost', 'data-topic-like') + button('分享', 'ghost', 'data-topic-share') + button('举报', 'ghost', 'data-topic-report') + '</div>';
    mountTpl('topic', 'topic', function () {
      var topicPage = q('#page-topic');
      var crumbBoard = q('.topic-context a[href^="#board:"]', topicPage);
      if (crumbBoard) { crumbBoard.setAttribute('href', '#board:' + encodeURIComponent(post.board)); crumbBoard.textContent = post.boardName; }
      var crumbNow = q('.topic-context__current', topicPage); if (crumbNow) crumbNow.textContent = '内容详情';
      var metaSpans = qa('.topic-meta > span', topicPage);
      if (metaSpans[0]) metaSpans[0].textContent = '内容';
      if (metaSpans[2]) metaSpans[2].textContent = post.boardName;
      if (metaSpans[3]) metaSpans[3].textContent = post.views + ' 浏览';
      var headH1 = q('.topic-head-card h1', topicPage); if (headH1) headH1.textContent = post.title;
      var prose = q('.topic-prose', topicPage);
      if (prose) {
        var firstP = q('p', prose); if (firstP) firstP.textContent = post.body;
        qa('.topic-restricted, [data-unlocked-content]', prose).forEach(function (el) { el.remove(); });
        prose.insertAdjacentHTML('beforeend', restrictedHtml + unlockedHtml);
      }
      var commentsHead = q('.topic-comments__head h2', topicPage); if (commentsHead) commentsHead.textContent = '回复 · ' + comments.length + ' 条';
      var commentsHost = q('[data-topic-comments]', topicPage); if (commentsHost) commentsHost.innerHTML = commentsHtml;
      var replyBox = q('[data-topic-reply]', topicPage); if (replyBox) replyBox.value = state.draft.body || '';
      var replyCount = q('[data-reply-count]', topicPage); if (replyCount) replyCount.textContent = (state.draft.body || '').length + ' / 1000';
      var side = q('.topic-side', topicPage);
      if (side) side.innerHTML = card('关于作者', topicAuthorCard + topicActionCard) + card('所在板块', '<p>' + link(board.name, '#board:' + board.slug) + '</p><p>规则、版主与热门标签</p>');

    relocateTopicChrome(q('#page-topic'));
    setDocumentTitle(post.title);
    wireTopic(id);
    });
}
  function wireTopic(id) {
    var textarea = q('[data-topic-reply]');
    var counter = q('[data-reply-count]');
    function updateCount() { if (counter) counter.textContent = textarea.value.length + ' / 1000'; }
    if (textarea) textarea.addEventListener('input', updateCount);
    updateCount();
    qa('[data-comment-sort]').forEach(function (tab) { tab.addEventListener('click', function () { qa('[data-comment-sort]').forEach(function (x) { x.classList.remove('is-active'); x.setAttribute('aria-selected', 'false'); }); tab.classList.add('is-active'); tab.setAttribute('aria-selected', 'true'); var mode = tab.getAttribute('data-comment-sort'); var host = q('[data-topic-comments]'); var items = qa('.topic-comment', host); if (mode === 'oldest') items.reverse().forEach(function (item) { host.appendChild(item); }); else if (mode === 'author') items.forEach(function (item) { item.hidden = item.textContent.indexOf('Chaos') < 0; }); else items.forEach(function (item) { item.hidden = false; }); }); });
    var favorite = q('[data-topic-favorite]');
    if (favorite) favorite.addEventListener('click', function () { if (!requireAuth('#topic:' + id)) return; var index = state.favorites.indexOf(id); if (index < 0) { state.favorites.push(id); favorite.textContent = '已收藏'; favorite.setAttribute('aria-pressed', 'true'); toast('已加入收藏'); } else { state.favorites.splice(index, 1); favorite.textContent = '收藏'; favorite.setAttribute('aria-pressed', 'false'); toast('已取消收藏'); } saveState(); });
    var likeBtn = q('[data-topic-like]');
    if (likeBtn) {
      var likeKey = 'like:' + id;
      var liked = state.toggles[likeKey] === true;
      likeBtn.classList.toggle('primary', liked);
      likeBtn.setAttribute('aria-pressed', String(liked));
      likeBtn.addEventListener('click', function () { if (!requireAuth('#topic:' + id)) return; liked = !liked; state.toggles[likeKey] = liked; this.classList.toggle('primary', liked); this.setAttribute('aria-pressed', String(liked)); saveState(); toast(liked ? '已点赞' : '已取消点赞'); });
    }
    var share = q('[data-topic-share]');
    if (share) share.addEventListener('click', function () { openModal('分享主题', '<p>复制此链接分享给朋友。</p><div class="app-secret"><input readonly value="https://bblbb.local/#topic:' + esc(id) + '">' + button('复制链接', 'ghost', 'data-copy-share') + '</div>'); });
    var report = q('[data-topic-report]');
    if (report) report.addEventListener('click', function () { openReportModal('主题举报'); });
    var code = q('[data-copy-code]');
    if (code) code.addEventListener('click', function () { toast('代码已复制'); });
    qa('[data-comment-like]').forEach(function (b) { b.addEventListener('click', function () { if (!requireAuth('#topic:' + id)) return; this.classList.toggle('primary'); toast('回复点赞状态已更新'); }); });
    qa('[data-comment-quote]').forEach(function (b) { b.addEventListener('click', function () { if (textarea) { textarea.value = '> ' + b.getAttribute('data-comment-quote') + '\n\n'; textarea.focus(); updateCount(); } }); });
    qa('[data-comment-reply]').forEach(function (b) { b.addEventListener('click', function () { if (textarea) { textarea.focus(); textarea.scrollIntoView({ behavior: 'smooth', block: 'center' }); } }); });
    qa('[data-comment-report]').forEach(function (b) { b.addEventListener('click', function () { openReportModal('回复举报'); }); });
    var focus = q('[data-focus-reply]');
    if (focus) focus.addEventListener('click', function () { if (textarea) { textarea.focus(); textarea.scrollIntoView({ behavior: 'smooth', block: 'center' }); } });
    var paid = q('[data-paid-unlock]');
    if (paid) paid.addEventListener('click', function () {
      if (!requireAuth('#topic:' + id)) return;
      if (state.balances.coin < 10) { toast('B币余额不足，请先充值'); return; }
      openModal('确认支付并解锁', '<p class="app-confirm-copy">当前余额 <strong>' + state.balances.coin + ' B币</strong></p><p class="app-confirm-copy">本次扣除 <strong>10 B币</strong>，解锁后余额 <strong>' + (state.balances.coin - 10) + ' B币</strong>。</p><div class="app-notice">支付后不可撤销；历史流水仅可追加补偿记录。</div>', '取消 ' + button('确认支付并解锁', 'primary', 'data-confirm-paid'));
      q('[data-confirm-paid]').addEventListener('click', function () {
        var confirmButton = this;
        actionPending(confirmButton, '支付中…', function () {
          state.balances.coin -= 10;
          state.topicUnlocked[id] = true;
          pushLedger('topic_unlock', -10, '受限内容解锁');
          saveState();
          closeModal();
          renderTopic(id);
          toast('解锁成功 · 余额已同步为 ' + state.balances.coin + ' B币');
        });
      });
    });
    var submit = q('[data-submit-reply]');
    if (submit) submit.addEventListener('click', function () {
      var value = textarea ? textarea.value.trim() : '';
      var error = q('[data-reply-error]');
      if (!value) { error.hidden = false; error.textContent = '回复内容不能为空'; if (textarea) textarea.focus(); return; }
      if (value.length > 1000) { error.hidden = false; error.textContent = '回复不能超过 1000 字'; return; }
      if (!requireAuth('#topic:' + id)) { state.draft.body = value; saveState(); return; }
      var currentButton = this;
      setMutationState(currentButton, '发布中…', '已发布', function () {
        var list = seedComments(id);
        var createdComment = { id: 'comment-' + Date.now(), topicId: id, author: 'Chaos', text: value, time: '刚刚', likes: 0, depth: 0 };
        list.push(createdComment);
        if (!Array.isArray(state.publishedComments)) state.publishedComments = [];
        state.publishedComments.push(createdComment);
        if (postHasRestriction(id)) state.topicUnlocked[id] = true;
        state.draft.body = '';
        pushLedger('reply', 1, '发布回复');
        saveState();
        renderTopic(id);
        toast(postHasRestriction(id) ? '回复已发布，隐藏内容已解锁' : '回复已发布');
      });
    });
    var save = q('[data-save-reply]');
    if (save) save.addEventListener('click', function () { if (textarea) state.draft.body = textarea.value; saveState(); toast('回复草稿已保存'); });
    var sub = q('[data-subscribe-unlock]');
    if (sub) sub.addEventListener('click', function () { if (!requireAuth('#topic:' + id)) return; this.textContent = '已订阅解锁通知'; toast('解锁通知已订阅'); });
  }
  function postHasRestriction(id) { return !!((TOPICS[id] && TOPICS[id].restricted) || (Array.isArray(state.publishedTopics) && state.publishedTopics.some(function (item) { return item.id === id && item.visibility && item.visibility !== 'public'; }))); }
  function openReportModal(title) {
    openModal(title, '<label class="app-field-label">举报原因 <span class="app-required">*</span></label><select class="app-select" data-report-kind><option value="spam">垃圾广告</option><option value="abuse">人身攻击</option><option value="illegal">违法违规</option><option value="other">其他</option></select><label class="app-field-label" style="margin-top:12px">补充说明</label><textarea class="app-textarea" data-report-detail placeholder="可选"></textarea><p class="app-field-error" data-report-error hidden></p>', '取消 ' + button('提交举报', 'danger', 'data-submit-report'));
    q('[data-submit-report]').addEventListener('click', function () {
      if (!requireAuth(location.hash || '#home')) return;
      var kind = q('[data-report-kind]').value;
      var detail = q('[data-report-detail]').value.trim();
      if (!kind) { q('[data-report-error]').hidden = false; q('[data-report-error]').textContent = '请选择举报原因'; return; }
      var reportLabels = { spam: '垃圾广告', abuse: '人身攻击', illegal: '违法违规', other: '其他' };
      state.reports['local-' + Date.now()] = { status: 'pending', title: reportLabels[kind] || '内容举报', reason: kind, detail: detail, time: '刚刚', accused: '目标用户', timeline: ['提交举报 · 刚刚'] };
      saveState();
      closeModal();
      state.customNotifications.unshift({ id: 'report-' + Date.now(), type: '系统', title: '举报已受理', detail: '案件状态：待处理', target: '#appeals', unread: true });
      state.notificationsRead = false;
      saveState();
      syncBellBadge();
      toast('举报已提交 · 状态：待处理');
    });
  }

  function renderPublish(mode) {
    var selectedBoard = String(mode || '').split(':')[1] || '';
    
    
    

    

    

    

    

    

    mountTpl('publish', 'publish', function () {
      var typeSwitch = q('.composer-type-switch'); if (typeSwitch) typeSwitch.remove();
      var pubH1 = q('#page-publish h1'); if (pubH1) pubH1.textContent = '发布内容';
      var pubEyebrow = q('.composer-eyebrow'); if (pubEyebrow) pubEyebrow.textContent = 'CREATE / CONTENT';
      var pubContext = q('.topic-context__current'); if (pubContext) pubContext.textContent = '发布内容';
      var summaryLabel = q('label[for="publish-summary"]'); if (summaryLabel) summaryLabel.textContent = '摘要';
      var summaryInput = q('[data-publish-summary-input]'); if (summaryInput) { summaryInput.setAttribute('aria-label', '内容摘要'); summaryInput.setAttribute('placeholder', '一句话概括内容核心观点…'); }
      var pubBoard = q('[data-publish-board]'); if (pubBoard) pubBoard.value = selectedBoard;
      var pubTitle = q('[data-publish-title]'); if (pubTitle) pubTitle.value = state.draft.title || '';
      var pubTags = q('[data-publish-tags]'); if (pubTags) pubTags.value = (state.draft.tags || []).join(', ');
      var pubBody = q('[data-publish-body]'); if (pubBody) pubBody.value = state.draft.body || '';
      var pubSummary = q('[data-publish-summary]'); if (pubSummary) pubSummary.value = state.draft.summary || '';
      var pubTitleCount = q('[data-title-count]'); if (pubTitleCount) pubTitleCount.textContent = (state.draft.title || '').length + ' / 80';
      var pubBodyCount = q('[data-body-count]'); if (pubBodyCount) pubBodyCount.textContent = (state.draft.body || '').length + ' / 5000';
      var pubVis = state.draft.visibility || 'public';
      qa('input[name="visibility"]').forEach(function (r) { r.checked = r.value === pubVis; var visCard = r.closest('label'); if (visCard) visCard.classList.toggle('is-active', r.value === pubVis); });
      var pubPaidPanel = q('[data-paid-options]'); if (pubPaidPanel) pubPaidPanel.classList.toggle('is-visible', pubVis === 'paid');
      var pubPrice = q('[data-paid-price]'); if (pubPrice) pubPrice.value = state.draft.paidPrice || 10;
      var pubSched = state.draft.schedule || 'immediate';
      qa('input[name="schedule"]').forEach(function (r) { r.checked = r.value === pubSched; var schedCard = r.closest('label'); if (schedCard) schedCard.classList.toggle('is-active', r.value === pubSched); });
      var pubSchedPanel = q('[data-schedule-options]'); if (pubSchedPanel) pubSchedPanel.classList.toggle('is-visible', pubSched === 'scheduled');
      var pubAt = q('[data-schedule-at]'); if (pubAt) pubAt.value = state.draft.scheduledAt || '';


    wirePublish();
    });
}
  function wirePublish() {
    var form = q('[data-publish-form]');
    var title = q('[data-publish-title]');
    var board = q('[data-publish-board]');
    var body = q('[data-publish-body]');
    var preview = q('[data-publish-preview]');
    var titleCount = q('[data-title-count]');
    var bodyCount = q('[data-body-count]');

    function updateCounts() {
      if (titleCount && title) titleCount.textContent = title.value.length + ' / 100';
      if (bodyCount && body) bodyCount.textContent = body.value.length + ' 字';
    }

    function validate() {
      var ok = true;
      var messages = [];
      var selectedFile = (q('[data-publish-file]') || {}).files ? q('[data-publish-file]').files[0] : null;
      if (selectedFile && selectedFile.size > 10 * 1024 * 1024) {
        if (q('[data-file-error]')) q('[data-file-error]').hidden = false;
        messages.push('附件超过 10 MB 上限');
        ok = false;
      } else if (q('[data-file-error]')) {
        q('[data-file-error]').hidden = true;
      }
      if (!title.value.trim()) {
        if (q('[data-title-error]')) q('[data-title-error]').hidden = false;
        title.classList.add('is-error');
        messages.push('标题不能为空');
        ok = false;
      } else {
        if (q('[data-title-error]')) q('[data-title-error]').hidden = true;
        title.classList.remove('is-error');
      }
      if (!board.value) {
        if (q('[data-board-error]')) q('[data-board-error]').hidden = false;
        board.classList.add('is-error');
        messages.push('必须选择板块');
        ok = false;
      } else {
        if (q('[data-board-error]')) q('[data-board-error]').hidden = true;
        board.classList.remove('is-error');
      }
      if (!body.value.trim()) {
        body.classList.add('is-error');
        messages.push('正文不能为空');
        ok = false;
      } else {
        body.classList.remove('is-error');
      }
      var summary = q('[data-publish-summary]');
      if (summary) summary.hidden = ok;
      var summaryText = q('[data-publish-summary-text]');
      if (summaryText) summaryText.textContent = messages.join(' · ');
      if (!ok) {
        if (!title.value.trim()) title.focus();
        else if (!board.value) board.focus();
        else body.focus();
      }
      return ok;
    }

    function autosave() {
      state.draft.title = title.value;
      state.draft.body = body.value;
      state.draft.board = board.value;
      state.draft.tags = (q('[data-publish-tags]') || {}).value || '';
      state.draft.summary = (q('[data-publish-summary-input]') || {}).value || '';
      state.draft.visibility = (q('input[name=visibility]:checked') || {}).value || 'public';
      state.draft.paidPrice = Number((q('[data-paid-price]') || {}).value || 10);
      state.draft.schedule = (q('input[name=schedule]:checked') || {}).value || 'now';
      state.draft.scheduleAt = (q('[data-schedule-at]') || {}).value || '';
      saveState();
      updateCounts();
      var draftStatus = q('[data-draft-status]');
      if (draftStatus) draftStatus.innerHTML = '<i class="save-dot"></i>草稿已自动保存 · 刚刚';
    }

    title.addEventListener('input', autosave);
    body.addEventListener('input', autosave);
    board.addEventListener('change', autosave);
    if (q('[data-publish-tags]')) q('[data-publish-tags]').addEventListener('input', autosave);
    if (q('[data-publish-summary-input]')) q('[data-publish-summary-input]').addEventListener('input', autosave);
    if (q('[data-paid-price]')) q('[data-paid-price]').addEventListener('input', autosave);
    if (q('[data-schedule-at]')) q('[data-schedule-at]').addEventListener('input', autosave);

    qa('input[name=visibility]').forEach(function (radio) {
      radio.addEventListener('change', function () {
        qa('.app-visibility-card').forEach(function (card) {
          var input = card.querySelector('input');
          card.classList.toggle('is-active', input && input.checked);
        });
        var paid = q('[data-paid-options]');
        if (paid) paid.classList.toggle('is-visible', radio.value === 'paid');
        autosave();
      });
    });

    qa('input[name=schedule]').forEach(function (radio) {
      radio.addEventListener('change', function () {
        qa('.app-schedule-choice').forEach(function (choice) {
          var input = choice.querySelector('input');
          choice.classList.toggle('is-active', input && input.checked);
        });
        var schedule = q('[data-schedule-options]');
        if (schedule) schedule.classList.toggle('is-visible', radio.value === 'scheduled');
        autosave();
      });
    });


    function renderPreviewHtml(src) {
      if (!src.trim()) return '<p class="app-muted">（空内容）</p>';
      var lines = src.split('\n');
      var out = [];
      var inCode = false;
      var codeBuf = [];
      for (var i = 0; i < lines.length; i++) {
        var l = lines[i];
        if (l.indexOf('```') === 0) {
          if (inCode) {
            out.push('<div class="topic-code"><pre><code>' + esc(codeBuf.join('\n')) + '</code></pre></div>');
            codeBuf = [];
            inCode = false;
          } else {
            inCode = true;
          }
          continue;
        }
        if (inCode) { codeBuf.push(l); continue; }
        if (l.indexOf('### ') === 0) { out.push('<h3>' + esc(l.slice(4)) + '</h3>'); continue; }
        if (l.indexOf('## ') === 0) { out.push('<h2>' + esc(l.slice(3)) + '</h2>'); continue; }
        if (l.indexOf('# ') === 0) { out.push('<h1>' + esc(l.slice(2)) + '</h1>'); continue; }
        if (l.indexOf('> ') === 0) { out.push('<blockquote>' + esc(l.slice(2)) + '</blockquote>'); continue; }
        if (l.indexOf('- ') === 0) { out.push('<ul><li>' + esc(l.slice(2)) + '</li></ul>'); continue; }
        if (!l.trim()) continue;
        out.push('<p>' + esc(l) + '</p>');
      }
      if (inCode) out.push('<div class="topic-code"><pre><code>' + esc(codeBuf.join('\n')) + '</code></pre></div>');
      return out.join('');
    }

    qa('[data-editor-tab]').forEach(function (tab) {
      tab.addEventListener('click', function () {
        qa('[data-editor-tab]').forEach(function (x) { x.classList.remove('is-active'); });
        tab.classList.add('is-active');
        var mode = tab.getAttribute('data-editor-tab');
        var splitContainer = q('.app-editor-split-container');
        if (mode === 'preview') {
          if (splitContainer) splitContainer.classList.remove('is-split');
          preview.hidden = false;
          preview.innerHTML = renderPreviewHtml(body.value);
          body.hidden = true;
        } else if (mode === 'split') {
          if (splitContainer) splitContainer.classList.add('is-split');
          preview.hidden = false;
          preview.innerHTML = renderPreviewHtml(body.value);
          body.hidden = false;
        } else {
          if (splitContainer) splitContainer.classList.remove('is-split');
          preview.hidden = true;
          body.hidden = false;
        }
      });
    });

    qa('.editor-tool-btn').forEach(function (btn) {
      btn.addEventListener('click', function () {
        var tool = btn.getAttribute('data-tool');
        var start = body.selectionStart || 0;
        var end = body.selectionEnd || 0;
        var val = body.value;
        var sel = val.slice(start, end);
        var before = '', after = '', insert = '';
        if (tool === 'bold') { before = '**'; after = '**'; insert = sel || '粗体文本'; }
        else if (tool === 'italic') { before = '*'; after = '*'; insert = sel || '斜体文本'; }
        else if (tool === 'heading') { before = '## '; after = ''; insert = sel || '标题'; }
        else if (tool === 'quote') { before = '> '; after = ''; insert = sel || '引用内容'; }
        else if (tool === 'code') {
          if (sel.indexOf('\n') >= 0) { before = '```typescript\n'; after = '\n```'; insert = sel || 'const x = 1;'; }
          else { before = '`'; after = '`'; insert = sel || 'code'; }
        }
        else if (tool === 'link') { before = '['; after = '](https://)'; insert = sel || '链接文字'; }
        else if (tool === 'list') { before = '- '; after = ''; insert = sel || '列表项'; }
        else if (tool === 'table') { before = '| 列 1 | 列 2 |\n| --- | --- |\n| 内容 | 内容 |'; after = ''; insert = ''; }
        
        body.value = val.slice(0, start) + before + insert + after + val.slice(end);
        body.focus();
        var cursorStart = start + before.length;
        var cursorEnd = cursorStart + insert.length;
        body.setSelectionRange(cursorStart, cursorEnd);
        autosave();
        if (!preview.hidden) preview.innerHTML = renderPreviewHtml(body.value);
      });
    });

    var fileInput = q('[data-publish-file]');
    if (fileInput) {
      fileInput.addEventListener('change', function () {
        var file = this.files[0];
        var nameSpan = q('[data-upload-filename]');
        if (file && file.size > 10 * 1024 * 1024) {
          this.value = '';
          if (q('[data-file-error]')) q('[data-file-error]').hidden = false;
          if (nameSpan) nameSpan.textContent = '未选择任何文件';
          toast('附件超过 10 MB 上限');
        } else {
          if (q('[data-file-error]')) q('[data-file-error]').hidden = true;
          if (file) {
            if (nameSpan) nameSpan.textContent = file.name + ' (' + (file.size > 1048576 ? (file.size / 1048576).toFixed(1) + ' MB' : (file.size / 1024).toFixed(0) + ' KB') + ')';
            state.draft.attachment = { name: file.name, size: file.size, type: file.type || 'application/octet-stream', status: 'selected' };
            saveState();
            var draftStatus = q('[data-draft-status]');
            if (draftStatus) draftStatus.innerHTML = '<i class="save-dot"></i>附件已选择 · 需上传完成后发布';
          } else {
            if (nameSpan) nameSpan.textContent = '未选择任何文件';
          }
        }
      });
    }

    if (q('[data-save-publish]')) {
      q('[data-save-publish]').addEventListener('click', function () {
        autosave();
        toast('草稿已保存');
      });
    }

    if (q('[data-publish-failure]')) {
      q('[data-publish-failure]').addEventListener('click', function () {
        state.messageFail = !state.messageFail;
        this.textContent = state.messageFail ? '已开启失败模拟' : '模拟接口失败';
        toast(state.messageFail ? '下一次提交将模拟失败' : '已关闭失败模拟');
      });
    }

    function publishPayload() {
      return {
        title: title.value.trim(),
        body: body.value,
        board: board.value,
        tags: (q('[data-publish-tags]') || {}).value || '',
        summary: (q('[data-publish-summary-input]') || {}).value || '',
        visibility: (q('input[name=visibility]:checked') || {}).value || 'public',
        paidPrice: Number((q('[data-paid-price]') || {}).value || 10),
        schedule: (q('input[name=schedule]:checked') || {}).value || 'now',
        scheduleAt: (q('[data-schedule-at]') || {}).value || ''
      };
    }

    if (q('[data-submit-review]')) {
      q('[data-submit-review]').addEventListener('click', function () {
        if (!validate()) return;
        if (state.messageFail) {
          toast('提交失败，请重试；表单内容已保留');
          return;
        }
        var visLabels = { public: '所有人可见', members: '登录后可见', paid: '付费可见' };
        var pp = publishPayload();
        openModal('提交审核', '<p>内容将进入审核队列，审核通过后公开。</p><p class="app-muted">可见性：' + (visLabels[pp.visibility] || esc(pp.visibility)) + ' · 定时：' + (pp.schedule === 'scheduled' ? '定时 ' + esc(pp.scheduleAt) : '立即') + '</p>', '取消 ' + button('确认提交', 'primary', 'data-confirm-publish'));
        var confirmBtn = q('[data-confirm-publish]');
        if (confirmBtn) {
          confirmBtn.addEventListener('click', function () {
            var reviewPost = Object.assign({ id: 'draft-' + Date.now(), status: 'pending_review', author: (state.account && state.account.displayName) || 'Chaos', type: 'content', boardName: ((BOARDS.find(function (b) { return b.slug === pp.board; }) || {}).name) || '板块', views: '1' }, publishPayload());
            state.publishedTopics.unshift(reviewPost);
            state.draft = { title: '', body: '', board: '', tags: '', summary: '', visibility: 'public', paidPrice: 10, schedule: 'now', scheduleAt: '' };
            pushLedger('submit_review', 1, '提交审核');
            saveState();
            closeModal();
            toast('已提交审核');
            goDrafts();
          });
        }
      });
    }

    if (q('[data-submit-publish]')) {
      q('[data-submit-publish]').addEventListener('click', function () {
        if (!validate()) return;
        if (state.messageFail) {
          toast('发布失败，请重试；表单内容已保留');
          return;
        }
        var payload = publishPayload();
        var visLabels2 = { public: '所有人可见', members: '登录后可见', paid: '付费可见' };
        openModal('立即发布', '<p>确认发布这篇内容？发布后仍可继续编辑。</p><p class="app-muted">可见性：' + (visLabels2[payload.visibility] || esc(payload.visibility)) + ' · 定时：' + (payload.schedule === 'scheduled' ? '定时 ' + esc(payload.scheduleAt) : '立即') + '</p>', '取消 ' + button('确认发布', 'primary', 'data-confirm-publish'));
        var confirmBtn = q('[data-confirm-publish]');
        if (confirmBtn) {
          confirmBtn.addEventListener('click', function () {
            var newPost = Object.assign({ id: 'post-' + Date.now(), status: payload.schedule === 'scheduled' ? 'scheduled' : 'published', author: (state.account && state.account.displayName) || 'Chaos', type: 'content', boardName: ((BOARDS.find(function (b) { return b.slug === payload.board; }) || {}).name) || '板块', views: '1' }, payload);
            if (payload.visibility === 'paid') { newPost.restricted = 'paid'; newPost.locked = '支付 ' + payload.paidPrice + ' B币永久解锁'; }
            state.publishedTopics.unshift(newPost);
            state.draft = { title: '', body: '', board: '', tags: '', summary: '', visibility: 'public', paidPrice: 10, schedule: 'now', scheduleAt: '' };
            pushLedger('publish', 1, '发布内容');
            saveState();
            closeModal();
            toast(payload.schedule === 'scheduled' ? '已安排定时发布' : '发布成功');
            if (payload.schedule === 'scheduled') goDrafts(); else go('#topic:' + newPost.id);
          });
        }
      });
    }

    var draftsLink = q('[data-go-drafts]');
    if (draftsLink) draftsLink.addEventListener('click', function () { goDrafts(); });
  }
  function renderUser(name, target) {
    var user = userByName(name);
    var own = user.name === 'Chaos' || user.name === 'admin';
    var tabs = [['posts', '内容'], ['replies', '回复'], ['activity', '动态']];
    if (own) { tabs.splice(1, 0, ['drafts', '草稿']); tabs.splice(4, 0, ['favorites', '收藏']); tabs.push(['points', '积分明细']); }
    var tabButtons = tabs.map(function (tab, i) { return '<button class="' + (i === 0 ? 'is-active' : '') + '" data-user-tab="' + tab[0] + '">' + tab[1] + '</button>'; }).join('');
    var list = ARTICLES.slice(0, 3).map(function (article) { return '<a class="app-post-row" href="#topic:' + article.id + '">' + profileBadge(user.name, user.initial) + '<span class="app-post-row__main"><h3>' + esc(article.title) + '</h3><p>' + esc(article.summary) + '</p></span><span class="app-post-row__right">' + esc(article.views) + '</span></a>'; }).join('');
    var published = (state.publishedTopics || []).map(function (article) { return '<a class="app-post-row" href="#topic:' + encodeURIComponent(article.id) + '">' + profileBadge(user.name, user.initial) + '<span class="app-post-row__main"><h3>' + esc(article.title) + '</h3><p>' + esc(article.summary || article.body.slice(0, 100)) + '</p><span class="sbadge ' + (article.status === 'pending_review' ? 'sb-hot' : 'sb-gray') + '">' + (article.status === 'pending_review' ? '待审核' : article.status === 'scheduled' ? '定时发布' : '已发布') + '</span></span></a>'; }).join('');
    list += published;
    var topicRows = Object.keys(TOPICS).map(function (k) { return TOPICS[k]; }).filter(function (p) { return p.author === user.name && ['101', '102', '103'].indexOf(p.id) < 0; }).map(function (p) { return '<a class="app-post-row" href="#topic:' + p.id + '">' + profileBadge(user.name, user.initial) + '<span class="app-post-row__main"><h3>' + esc(p.title) + '</h3><p>' + esc(p.boardName) + ' · ' + esc(p.views) + ' 浏览</p></span></a>'; }).join('');
    list += topicRows;
    var replyRows = [['使用 SvelteKit 构建博客与轻量论坛是否合理？', 'SSR 首屏和 CSR 交互的取舍，建议先固定路由再谈框架。'], ['Rust 异步运行时选型：tokio、smol 还是 async-std？', 'tokio 生态最完整，smol 适合嵌入式场景。']].map(function (x) { return '<div class="app-post-row"><span class="app-post-row__main"><h3>在「' + x[0] + '」中回复</h3><p>' + x[1] + '</p></span><span class="app-post-row__right"><span class="app-muted">2 小时前</span></span></div>'; }).join('');
    var activityRows = [['发布了新内容', '「' + (ARTICLES[0].title) + '」', '昨天'], ['解锁了成就「签到达人」', '连续签到 30 天', '3 天前'], ['发布了新内容', '「' + (TOPICS['201'] ? TOPICS['201'].title : '新内容') + '」', '1 周前']].map(function (x) { return '<div class="app-post-row"><span class="app-post-row__main"><h3>' + x[0] + '</h3><p>' + x[1] + '</p></span><span class="app-post-row__right"><span class="app-muted">' + x[2] + '</span></span></div>'; }).join('');
    var tabExtra = { replies: replyRows, activity: activityRows };
    var autoDraftRow = (own && state.draft && (state.draft.title || state.draft.body)) ? '<div class="app-post-row"><span class="app-post-row__main"><h3>' + esc(state.draft.title || '无题草稿') + '</h3><p><span class="sbadge sb-hot">自动保存</span> · 刚刚 · ' + esc(String(state.draft.body || '').slice(0, 48)) + '</p></span>' + button('继续编辑', 'ghost', 'data-go-publish="article"') + '</div>' : '';
    var isFollowing = state.followings['user:' + user.name] === true;
    var actions = own ? button('编辑资料', 'secondary', 'data-go-settings="profile"') : button(isFollowing ? '已关注' : '关注', 'secondary', 'data-user-follow' + (isFollowing ? ' disabled' : ''));
    mountTpl((target || 'user'), (target === 'me' ? 'me' : 'user'), function () {
      var userH1 = q('.app-route-head h1'); if (userH1) userH1.textContent = own ? '我的' : user.name;
      var userActionsHost = q('.app-route-head__actions'); if (userActionsHost) userActionsHost.innerHTML = actions;
      var userProfile = q('section.app-profile');
      if (userProfile) {
        var profAvatar = q('.avatar', userProfile); if (profAvatar) profAvatar.outerHTML = profileBadge(user.name, user.initial);
        var profH2 = q('h2', userProfile); if (profH2) profH2.innerHTML = esc(user.name) + ' <span class="lvbadge">' + esc(user.level) + '</span>' + (user.role ? ' <span class="role-badge ' + (user.role === '站长' ? 'role-admin' : 'role-mod') + '">' + esc(user.role) + '</span>' : '');
        if (profH2 && profH2.nextElementSibling) profH2.nextElementSibling.textContent = user.bio;
      }
      var userCards = qa('.app-account-cards .app-account-card strong');
      var userCardsAll = qa('.app-account-cards .app-account-card');
      if (userCards[0]) userCards[0].textContent = own ? state.balances.exp : (user.exp || 0);
      var expSpan = userCardsAll[0] && userCardsAll[0].querySelector('span');
      if (expSpan && !own) expSpan.textContent = user.level + ' · ' + (user.exp || 0) + ' 经验';
      if (userCards[1]) userCards[1].textContent = own ? state.balances.coin : user.coin;
      if (userCards[2]) userCards[2].textContent = own ? state.balances.contrib : user.contrib;
      var contribSpan = userCardsAll[2] && userCardsAll[2].querySelector('span');
      if (contribSpan && !own) contribSpan.textContent = (user.contrib || 0) >= 300 ? '已达当前称号上限' : '距离下一称号还有 ' + (300 - (user.contrib || 0));
      var profMeta = q('.app-profile__meta');
      if (profMeta) profMeta.innerHTML = '<span><b>' + (user.posts || 0) + '</b> 帖子</span><span><b>' + (user.followers || 0) + '</b> 关注者</span><span><b>' + (user.following || 0) + '</b> 正在关注</span><span>加入于 ' + (user.joined || '2026-01-01') + '</span>';
      var userOwnLinks = q('.app-account-cards') && q('.app-account-cards').nextElementSibling;
      if (userOwnLinks && userOwnLinks.classList.contains('app-toolbar')) userOwnLinks.hidden = !own;
      var userTabsHost = q('.app-filter-tabs'); if (userTabsHost) userTabsHost.innerHTML = tabButtons;
      var userListHost = q('[data-user-list]'); if (userListHost) userListHost.innerHTML = list;

    qa('[data-go-settings]').forEach(function (edit) { edit.addEventListener('click', function () { go('#settings:profile'); }); });
    qa('[data-user-follow]').forEach(function (follow) { follow.addEventListener('click', function () { if (!requireAuth('#user:' + encodeURIComponent(user.name))) return; state.followings['user:' + user.name] = true; qa('[data-user-follow]').forEach(function (x) { x.textContent = '已关注'; x.disabled = true; }); saveState(); toast('已关注 ' + user.name); }); });
    var follow = q('[data-user-follow]');
    if (follow) follow.addEventListener('click', function () { if (!requireAuth('#user:' + encodeURIComponent(user.name))) return; state.followings['user:' + user.name] = true; this.textContent = '已关注'; this.disabled = true; saveState(); toast('已关注 ' + user.name); });
    var userTablist = q('.app-filter-tabs', q('[data-user-list]').parentElement); if (userTablist) userTablist.setAttribute('role', 'tablist');
    qa('[data-user-tab]').forEach(function (tab) { tab.setAttribute('role', 'tab'); tab.setAttribute('aria-selected', String(tab.classList.contains('is-active'))); tab.addEventListener('click', function () { qa('[data-user-tab]').forEach(function (x) { x.classList.remove('is-active'); x.setAttribute('aria-selected', 'false'); }); tab.classList.add('is-active'); tab.setAttribute('aria-selected', 'true'); var listHost = q('[data-user-list]'); if (tab.getAttribute('data-user-tab') === 'favorites') { var favs = state.favorites.map(function (id) { return TOPICS[id]; }).filter(Boolean); listHost.innerHTML = favs.length ? favs.map(function (p) { return '<a class="app-post-row" href="#topic:' + p.id + '"><span class="app-post-row__main"><h3>' + esc(p.title) + '</h3><p>已加入收藏</p></span></a>'; }).join('') : empty('bookmark', '还没有收藏', '去发现一些值得保存的内容', button('去发现', 'primary', 'data-go-discover')); } else if (tab.getAttribute('data-user-tab') === 'points') { listHost.innerHTML = '<div class="app-card__body"><table class="app-table"><tr><th>时间</th><th>类型</th><th>变化</th><th>余额</th></tr><tr><td>今天</td><td>每日签到</td><td>+5 经验</td><td>' + state.balances.exp + '</td></tr><tr><td>昨天</td><td>商城消费</td><td>-30 B币</td><td>' + state.balances.coin + '</td></tr></table></div>'; } else if (tab.getAttribute('data-user-tab') === 'drafts') { listHost.innerHTML = '<div class="app-toolbar" style="padding:12px 2px 10px">' + button('新建文章', 'primary', 'data-go-publish="article"') + '</div>' + autoDraftRow + [['Paxos 在分布式消息队列中的落地笔记', '昨天'], ['Rust 异步运行时对比：tokio vs smol', '3 天前'], ['Docker 多阶段构建瘦身实战', '1 周前']].map(function (x) { return '<div class="app-post-row"><span class="app-post-row__main"><h3>' + x[0] + '</h3><p><span class="sbadge sb-gray">草稿</span> · 更新于 ' + x[1] + '</p></span>' + button('继续编辑', 'ghost', 'data-go-publish="article"') + button('删除', 'ghost', 'data-delete-draft') + '</div>'; }).join(''); listHost.querySelectorAll('[data-delete-draft]').forEach(function (b) { b.addEventListener('click', function () { openModal('删除草稿', '<p>删除后无法恢复，确认继续？</p>', '取消 ' + button('确认删除', 'danger', 'data-confirm-delete-draft')); q('[data-confirm-delete-draft]').addEventListener('click', function () { b.closest('.app-post-row').remove(); closeModal(); toast('草稿已删除'); }); }); }); } else { var key = tab.getAttribute('data-user-tab'); listHost.innerHTML = tabExtra[key] || list; } }); });
    qa('[data-go-discover]').forEach(function (b) { b.addEventListener('click', function () { go('#discover'); }); });
    });
}
  function renderFavorites() {
    var rows = state.favorites.map(function (id) { return TOPICS[id]; }).filter(Boolean);
    mountTpl('favorites', 'favorites', function () {
      var favLede = q('.app-route-head p'); if (favLede) favLede.textContent = '收藏的内容 · ' + rows.length + ' 项';
      var favHost = q('.app-post-list'); if (favHost) favHost.innerHTML = rows.length ? rows.map(function (p) { return '<div class="app-post-row" data-favorite-row="' + esc(p.id) + '">' + profileBadge(p.author) + '<a class="app-post-row__main" href="#topic:' + p.id + '"><h3>' + esc(p.title) + '</h3><p>' + esc(p.boardName) + ' · 已收藏</p></a><button type="button" class="btn ghost" aria-label="移除收藏：' + esc(p.title) + '" data-remove-favorite="' + p.id + '">移除</button></div>'; }).join('') : empty('bookmark', '还没有收藏', '去首页发现一些值得保存的内容', button('去首页', 'primary', 'data-go-home'));

    qa('[data-remove-favorite]').forEach(function (b) { b.addEventListener('click', function (event) { event.preventDefault(); var id = b.getAttribute('data-remove-favorite'); state.favorites = state.favorites.filter(function (x) { return x !== id; }); saveState(); renderFavorites(); toast('已取消收藏'); }); });
    });
}

  function renderSettings(tab) {
    var current = tab || 'profile';
    var panels = [['profile', '个人资料', 'user'], ['security', '账号安全', 'shield'], ['devices', '登录设备', 'monitor'], ['notifications', '通知设置', 'bell'], ['oauth', 'OAuth 授权', 'key'], ['privacy', '隐私设置', 'lock']];
    
    var body = {
      profile: card('个人资料', '<div class="app-form-field"><label class="app-field-label">昵称</label><input class="app-field" aria-label="昵称" data-profile-name value="' + esc(state.account.displayName) + '"></div><div class="app-form-field"><label class="app-field-label">个人简介</label><textarea class="app-textarea" aria-label="个人简介" data-profile-bio>' + esc(state.account.bio) + '</textarea></div>' + button('保存修改', 'primary', 'data-settings-save')),
      security: card('账号安全', '<div class="app-form-field"><label class="app-field-label" for="current-password">当前密码</label><input id="current-password" class="app-field" aria-label="当前密码" type="password"></div><div class="app-form-field"><label class="app-field-label" for="new-password">新密码</label><input id="new-password" class="app-field" aria-label="新密码" type="password"></div><div class="app-form-field"><label class="app-field-label" for="confirm-password">确认新密码</label><input id="confirm-password" class="app-field" aria-label="确认新密码" type="password"></div>' + button('更新密码', 'primary', 'data-settings-save') + '<hr><div class="app-notice">两步验证 TOTP · 当前未启用</div>' + button('启用两步验证', 'ghost', 'data-go-mfa')),
      devices: card('登录设备', '<div class="app-device"><span>' + icon('monitor') + '</span><div class="app-device__body"><b>当前设备 · Chrome / macOS</b><span>上海 · 当前会话</span></div><span class="sbadge sb-success">活跃</span></div>' + (state.devices.indexOf('iphone') >= 0 ? '<div class="app-device" data-device="iphone"><span>' + icon('smartphone') + '</span><div class="app-device__body"><b>iPhone 15 Pro</b><span>Safari · 2 小时前</span></div>' + button('下线', 'danger', 'data-revoke-device="iphone"') + '</div>' : '') + (state.devices.indexOf('linux') >= 0 ? '<div class="app-device" data-device="linux"><span>' + icon('monitor') + '</span><div class="app-device__body"><b>Desktop Linux · Firefox</b><span>北京 · 12 小时前</span></div>' + button('下线', 'danger', 'data-revoke-device="linux"') + '</div>' : '') + (state.devices.length <= 1 ? '<div class="app-empty"><b>没有其他登录设备</b><span>新设备登录后会显示在这里。</span></div>' : '') + '</div>'),
      notifications: card('通知设置', '<div class="app-device"><div class="app-device__body"><b>回复与提及</b><span>有人回复或 @ 你时提醒</span></div><button class="switch ' + (state.toggles.reply ? 'on' : '') + '" role="switch" aria-checked="' + String(!!state.toggles.reply) + '" data-toggle="reply" aria-label="切换回复与提及"><i></i></button></div><div class="app-device"><div class="app-device__body"><b>邮件摘要</b><span>每日摘要邮件（需要真实 sender）</span></div><button class="switch ' + (state.toggles.email ? 'on' : '') + '" role="switch" aria-checked="' + String(!!state.toggles.email) + '" data-toggle="email" aria-label="切换邮件摘要"><i></i></button></div>'),
      oauth: card('OAuth 授权应用', (state.oauthRevoked.cli ? '<div class="app-device"><div class="app-device__body"><b>我的 CLI 工具</b><span>cli_*** · 已撤销</span></div><span class="sbadge sb-gray">已撤销</span></div>' : '<div class="app-device" data-oauth-id="cli"><div class="app-device__body"><b>我的 CLI 工具</b><span>cli_*** · openid, profile</span></div>' + button('撤销授权', 'danger', 'data-revoke-oauth="cli"') + '</div>') + (state.oauthRevoked.demo ? '<div class="app-device"><div class="app-device__body"><b>测试 demo</b><span>demo_*** · 已撤销</span></div><span class="sbadge sb-gray">已撤销</span></div>' : '<div class="app-device" data-oauth-id="demo"><div class="app-device__body"><b>测试 demo</b><span>demo_*** · openid, email</span></div>' + button('撤销授权', 'danger', 'data-revoke-oauth="demo"') + '</div>')),
      privacy: card('隐私设置', '<div class="app-device"><div class="app-device__body"><b>个人资料可见性</b><span>决定谁能查看你的主页</span></div><select class="app-select" aria-label="个人资料可见性" data-privacy-visibility style="width:130px"><option value="public"' + (state.preferences.profileVisibility === 'public' ? ' selected' : '') + '>公开</option><option value="members"' + (state.preferences.profileVisibility === 'members' ? ' selected' : '') + '>仅会员</option><option value="private"' + (state.preferences.profileVisibility === 'private' ? ' selected' : '') + '>私密</option></select></div><div class="app-device"><div class="app-device__body"><b>允许搜索引擎收录</b><span>关闭后主页加 noindex</span></div><button class="switch ' + (state.toggles.index ? 'on' : '') + '" role="switch" aria-checked="' + String(!!state.toggles.index) + '" data-toggle="index" aria-label="切换搜索引擎收录"><i></i></button></div>')
    };
    mountTpl('settings', 'settings', function () {
      qa('[data-settings-tab]').forEach(function (b) {
        var active = b.getAttribute('data-settings-tab') === current;
        b.classList.toggle('is-active', active);
        b.setAttribute('aria-selected', String(active));
      });
      qa('[data-settings-panel]').forEach(function (panel) { panel.hidden = panel.getAttribute('data-settings-panel') !== current; });
      var setName = q('[data-profile-name]'); if (setName) setName.value = state.account.displayName;
      var setBio = q('[data-profile-bio]'); if (setBio) setBio.value = state.account.bio;
      var setBadge = q('[data-settings-tab="devices"] .sbadge'); if (setBadge) setBadge.textContent = state.devices.length;
      var setVis = q('[data-privacy-visibility]'); if (setVis) setVis.value = state.preferences.profileVisibility;
      qa('[data-toggle]').forEach(function (b) { var key = b.getAttribute('data-toggle'); var on = !!state.toggles[key]; b.classList.toggle('on', on); b.setAttribute('aria-checked', String(on)); });
      var devPanel = q('[data-settings-panel="devices"]'); if (devPanel) devPanel.innerHTML = body.devices;
      var oauthPanel = q('[data-settings-panel="oauth"]'); if (oauthPanel) oauthPanel.innerHTML = body.oauth;

    qa('[data-settings-tab]').forEach(function (b) { b.addEventListener('click', function () { go('#settings:' + b.getAttribute('data-settings-tab')); }); });
    qa('[data-toggle]').forEach(function (b) { b.addEventListener('click', function () { var key = b.getAttribute('data-toggle'); state.toggles[key] = !state.toggles[key]; b.classList.toggle('on', state.toggles[key]); b.setAttribute('aria-checked', String(!!state.toggles[key])); saveState(); toast(state.toggles[key] ? '已开启' : '已关闭'); }); });
    var visibility = q('[data-privacy-visibility]');
    if (visibility) visibility.addEventListener('change', function () { state.preferences.profileVisibility = visibility.value; saveState(); toast('隐私设置已保存'); });
    qa('[data-revoke-device]').forEach(function (b) { b.addEventListener('click', function () { openModal('下线设备', '<p>确认将该设备下线？现有会话将失效。</p>', '取消 ' + button('确认下线', 'danger', 'data-confirm-device')); q('[data-confirm-device]').addEventListener('click', function () { var row = b.closest('[data-device]'); if (row) row.remove(); state.devices = state.devices.filter(function (x) { return x !== b.getAttribute('data-revoke-device'); }); saveState(); closeModal(); toast('设备已下线'); }); }); });
    qa('[data-revoke-oauth]').forEach(function (b) { b.addEventListener('click', function () { var oauthId = b.getAttribute('data-revoke-oauth') || 'cli'; openModal('撤销授权', '<p>撤销后应用将无法继续访问已授权资源。</p>', '取消 ' + button('确认撤销', 'danger', 'data-confirm-oauth')); q('[data-confirm-oauth]').addEventListener('click', function () { state.oauthRevoked[oauthId] = true; saveState(); closeModal(); renderSettings('oauth'); toast('授权已撤销'); }); }); });
    var save = q('[data-settings-save]'); if (save) save.addEventListener('click', function () { if (current === 'profile') { state.account.displayName = (q('[data-profile-name]') || {}).value || state.account.displayName; state.account.bio = (q('[data-profile-bio]') || {}).value || state.account.bio; } saveState(); toast('设置已保存'); });
    var mfa = q('[data-go-mfa]'); if (mfa) mfa.addEventListener('click', function () { go('#mfa'); });
    var mfaCancel = q('[data-go-settings="security"]'); if (mfaCancel) mfaCancel.addEventListener('click', function () { go('#settings:security'); });
    });
}

  function renderMessages(openMobileChat) {
    var conversations = [{ name: 'Lin', username: 'linlv3', initial: 'L', level: 3, role: '活跃成员', online: true, text: '结论会在下次更新公告里公布。', time: '12 分钟前' }, { name: 'Nina', username: 'nina', initial: 'N', level: 3, role: '性能与可观测性', online: true, text: '好的，性能那篇我明天发出来一起讨论。', time: '1 小时前' }, { name: 'Mark', username: 'mark', initial: 'M', level: 4, role: '技术随笔版主', online: false, text: '收到，谢谢！', time: '昨天' }, { name: '站务助手', username: 'bot', initial: '站', level: 5, role: '官方账号', online: true, text: '欢迎加入 BBLBB！有任何问题随时私信我。', time: '3 天前' }];
    var activeName = state.activeConversation || 'Lin';
    function conversationMessages(name) { return Array.isArray(state.messages[name]) ? state.messages[name] : []; }
    function renderConversationBody(name) { var seed = conversations.find(function (c) { return c.name === name; }) || conversations[0]; var own = conversationMessages(name); return '<div class="bubble them">' + esc(seed.text) + '</div>' + own.map(function (m) { return '<div class="bubble ' + (m.failed ? 'message-failed' : 'me') + '" data-message-id="' + esc(m.id) + '">' + esc(m.text) + (m.failed ? ' · 发送失败，可重试' : '') + '</div>'; }).join(''); }
    mountTpl('messages', 'messages', function () {
      var messagePage = q('#page-messages');
      if (messagePage) {
        messagePage.classList.toggle('is-mobile-chat-open', !!openMobileChat && !!(window.matchMedia && window.matchMedia('(max-width: 767px)').matches));
        if (!q('[data-message-back]', messagePage)) {
          var chatHead = q('.app-chat__head', messagePage);
          var chatIdentity = q('.chat-identity', messagePage);
          if (chatHead && chatIdentity) {
            var back = document.createElement('button');
            back.type = 'button';
            back.className = 'app-chat__back';
            back.setAttribute('data-message-back', '');
            back.setAttribute('aria-label', '返回消息列表');
            back.title = '返回消息列表';
            back.innerHTML = icon('chevron-down', 'app-chat__back-icon');
            chatHead.insertBefore(back, chatIdentity);
          }
        }
      }
      var activeConv = conversations.find(function (c) { return c.name === activeName; }) || conversations[0];
      qa('[data-conversation]').forEach(function (b) { b.classList.toggle('is-active', b.getAttribute('data-conversation') === activeConv.name); });
      var msgHeadAvatar = q('.chat-identity .avatar');
      if (msgHeadAvatar) { msgHeadAvatar.setAttribute('data-profile-name', activeConv.initial); msgHeadAvatar.textContent = activeConv.initial; }
      var msgHeadName = q('.chat-identity h2'); if (msgHeadName) msgHeadName.textContent = activeConv.name;
      var msgHeadSub = q('.chat-identity small'); if (msgHeadSub) msgHeadSub.textContent = '@' + activeConv.username + ' · LV.' + activeConv.level + ' · ' + activeConv.role;
      var msgBody = q('[data-chat-body]'); if (msgBody) { msgBody.innerHTML = renderConversationBody(activeName); msgBody.scrollTop = msgBody.scrollHeight; }
      

    qa('[data-conversation]').forEach(function (b) { b.addEventListener('click', function () { var name = b.getAttribute('data-conversation'); state.activeConversation = name; saveState(); renderMessages(true); }); });
    var messageBack = q('[data-message-back]'); if (messageBack) messageBack.addEventListener('click', function () { var page = q('#page-messages'); if (page) page.classList.remove('is-mobile-chat-open'); var firstConversation = q('[data-conversation]'); if (firstConversation) firstConversation.focus(); });
    
    q('[data-send-message]').addEventListener('click', function () { var input = q('[data-chat-input]'); var status = q('[data-message-status]'); var value = input.value.trim(); if (!value) return; var name = state.activeConversation || 'Lin'; var id = 'message-' + Date.now(); var message = { id: id, text: value, failed: false }; if (!Array.isArray(state.messages[name])) state.messages[name] = []; state.messages[name].push(message); saveState(); var bubble = document.createElement('div'); bubble.className = 'bubble me'; bubble.textContent = value; bubble.setAttribute('data-message-id', id); var msgBody = q('[data-chat-body]'); msgBody.appendChild(bubble); msgBody.scrollTop = msgBody.scrollHeight; input.value = ''; status.textContent = '发送中…'; this.disabled = true; var send = this; window.setTimeout(function () { if (state.messageFail) { message.failed = true; saveState(); bubble.classList.add('message-failed'); status.textContent = '发送失败 · 可重试'; var retry = document.createElement('button'); retry.type = 'button'; retry.className = 'btn ghost sm'; retry.textContent = '重试'; retry.setAttribute('data-retry-message', 'true'); status.appendChild(retry); retry.addEventListener('click', function () { retry.disabled = true; state.messageFail = false; saveState(); status.textContent = '重新发送中…'; window.setTimeout(function () { message.failed = false; saveState(); bubble.classList.remove('message-failed'); bubble.classList.add('message-sent'); status.textContent = '已发送 · 刚刚'; retry.remove(); send.disabled = false; }, 450); }); } else { message.failed = false; saveState(); bubble.classList.add('message-sent'); status.textContent = '已发送 · 刚刚'; send.disabled = false; } }, 500); });
    });
}
  function renderNotifications() {
    var items = [{ id: 'reply-1', type: '回复', title: 'Lin 回复了你的帖子', detail: '使用 SvelteKit 构建博客与轻量论坛是否合理？', target: '#topic:201', unread: !state.notificationsRead }, { id: 'like-1', type: '点赞', title: 'Mark 赞了你的评论', detail: 'Rust 小机器上的 SQLite 并发实践', target: '#topic:101', unread: !state.notificationsRead }, { id: 'achievement-1', type: '系统', title: '你解锁了成就「连续签到 7 天」', detail: '系统通知', target: '#achievements', unread: false }, { id: 'follow-1', type: '关注', title: 'Nina 关注了你', detail: '互动提醒', target: '#user:Nina', unread: false }];
    var customItems = Array.isArray(state.customNotifications) ? state.customNotifications : [];
    items = customItems.concat(items);
    mountTpl('notifications', 'notifications', function () {
      var notifHost = q('[data-notif-list]');
      if (notifHost) notifHost.innerHTML = items.map(function (item) { return '<a class="app-post-row ' + (item.unread ? 'notif-unread' : '') + '" data-notif-type="' + item.type + '" data-notif-id="' + item.id + '" href="' + item.target + '"><span class="n-icon n-' + (item.type === '回复' ? 'reply' : item.type === '点赞' ? 'like' : 'sys') + '">' + icon(item.type === '点赞' ? 'heart' : item.type === '回复' ? 'message-square' : 'bell') + '</span><span class="app-post-row__main"><h3>' + esc(item.title) + '</h3><p>' + esc(item.detail) + ' · 刚刚</p></span>' + (item.unread ? '<span class="n-dot"></span>' : '') + '</a>'; }).join('');

    qa('[data-notif-filter]').forEach(function (tab) { tab.addEventListener('click', function () { qa('[data-notif-filter]').forEach(function (x) { x.classList.remove('is-active'); x.setAttribute('aria-selected', 'false'); }); tab.classList.add('is-active'); tab.setAttribute('aria-selected', 'true'); var mode = tab.getAttribute('data-notif-filter'); var visible = 0; qa('[data-notif-list] .app-post-row').forEach(function (row) { var ok = mode === 'all' || (mode === 'unread' && row.classList.contains('notif-unread')) || row.getAttribute('data-notif-type') === (mode === 'reply' ? '回复' : mode === 'like' ? '点赞' : mode === 'system' ? '系统' : mode); row.hidden = !ok; if (ok) visible += 1; }); q('[data-notif-empty]').hidden = visible > 0; }); });
    var readAll = q('[data-read-all]'); if (readAll) readAll.addEventListener('click', function () { qa('.notif-unread').forEach(function (row) { row.classList.remove('notif-unread'); var dot = q('.n-dot', row); if (dot) dot.remove(); }); state.notificationsRead = true; if (Array.isArray(state.customNotifications)) state.customNotifications.forEach(function (c) { c.unread = false; }); saveState(); syncBellBadge(); toast('通知已全部标记为已读'); });
    qa('[data-notif-list] .app-post-row').forEach(function (row) { row.addEventListener('click', function () { if (row.classList.contains('notif-unread')) { row.classList.remove('notif-unread'); var dot = q('.n-dot', row); if (dot) dot.remove(); var id = row.getAttribute('data-notif-id'); if (Array.isArray(state.customNotifications)) state.customNotifications.forEach(function (c) { if (c && c.id === id) c.unread = false; }); if (qa('.notif-unread').length === 0) { state.notificationsRead = true; } saveState(); syncBellBadge(); } }); });
    });
}
  function renderSearch(initialQuery) {
    var query = String(initialQuery || '');
    if (query) { try { query = decodeURIComponent(query); } catch (e) {} }
    
    mountTpl('search', 'search', function () {
      var searchLede = q('.app-route-head p'); if (searchLede) searchLede.textContent = query ? '正在搜索：' + query : '搜索帖子、用户或标签';
      var searchInput = q('[data-search-input]'); if (searchInput) searchInput.value = query;

    var form = q('[data-search-form]'); var input = q('[data-search-input]');
    function run() { var term = input.value.trim().toLowerCase(); var visible = 0; qa('[data-search-row]').forEach(function (row) { var ok = !term || row.textContent.toLowerCase().indexOf(term) >= 0; row.hidden = !ok; if (ok) visible += 1; }); q('[data-search-empty]').hidden = visible > 0; var title = q('[data-search-count]'); if (title) title.textContent = visible + ' 条结果'; }
    form.addEventListener('submit', function (event) { event.preventDefault(); var term = input.value.trim(); var target = '#search:' + encodeURIComponent(term); if (location.hash !== target) location.hash = target; else run(); });
    q('[data-search-reset]').addEventListener('click', function () { input.value = ''; var target = '#search:'; if (location.hash !== target) location.hash = target; else run(); });
    run();
    });
}

  function renderMarket() {
    mountTpl('market', 'market', function () {
 qa('[data-market-offer]').forEach(function (b) { b.addEventListener('click', function () { openModal('市场接入', '<p>创建 checkout intent，确认 scope、单笔限额和授权有效期。</p>', '取消 ' + button('继续', 'primary', 'data-market-confirm')); q('[data-market-confirm]').addEventListener('click', function () { closeModal(); go('#checkout'); }); }); }); qa('[data-market-purchase]').forEach(function (b) { b.addEventListener('click', function () { toast('交易详情：原子事务已提交，等待 Webhook 对账'); }); });
    });
}
  function renderCheckout() {
    mountTpl('checkout', 'checkout', function () {

    q('[data-go-market]').addEventListener('click', function () { go('#market'); });
    q('[data-confirm-checkout]').addEventListener('click', function () { actionPending(this, '提交中…', function () { toast('Checkout intent 已确认 · 等待 Webhook'); go('#purchases'); }); });
    });
}
  function renderPurchases() {
    mountTpl('purchases', 'purchases', function () {

    qa('[data-market-refund]').forEach(function (b) { b.addEventListener('click', function () { openModal('申请退款', '<p>退款需要由市场交易与对账策略审核。</p>', '取消 ' + button('确认申请', 'danger', 'data-confirm-refund')); q('[data-confirm-refund]').addEventListener('click', function () { closeModal(); toast('退款申请已提交'); }); }); });
    qa('[data-market-retry]').forEach(function (b) { b.addEventListener('click', function () { actionPending(b, '重试中…', function () { toast('Webhook 已加入重试队列'); }); }); });
    });
}
  function renderApiKeys() {
    mountTpl('apikeys', 'apikeys', function () {

    var createKey = q('[data-create-key]'); if (createKey) createKey.addEventListener('click', function () { openModal('创建 API 密钥', '<label class="app-field-label">名称</label><input class="app-field" aria-label="密钥名称" placeholder="deploy-bot"><label class="app-field-label">Scopes</label><div class="app-radio-list"><label><input type="checkbox" checked> posts:read</label><label><input type="checkbox"> drafts:write</label></div>', '取消 ' + button('创建', 'primary', 'data-confirm-key')); q('[data-confirm-key]').addEventListener('click', function () { closeModal(); openModal('密钥仅显示一次', '<p class="app-mono">bbl_live_demo_secret_once</p><p class="app-muted">请立即保存，关闭后不可再次查看。</p>', '我已保存'); }); });
    qa('[data-revoke-key]').forEach(function (b) { b.addEventListener('click', function () { openModal('撤销 API 密钥', '<p>撤销后使用该密钥的客户端会立即失败。</p>', '取消 ' + button('确认撤销', 'danger', 'data-confirm-key-revoke')); q('[data-confirm-key-revoke]').addEventListener('click', function () { b.closest('.app-device').remove(); closeModal(); toast('API 密钥已撤销'); }); }); });
    });
}
  function renderShop() {
    var products = [['头像框·星河', 30, '头像框'], ['昵称特效·流光', 45, '昵称特效'], ['主题皮肤·暗夜蓝', 60, '主题皮肤'], ['置顶券', 20, '实用道具']];
    mountTpl('shop', 'shop', function () {
      var shopCards = qa('.app-account-cards .app-account-card strong');
      if (shopCards[0]) shopCards[0].textContent = state.balances.exp;
      if (shopCards[1]) shopCards[1].textContent = state.balances.coin;
      if (shopCards[2]) shopCards[2].textContent = state.balances.contrib;

    var billingLink = q('[data-go-billing]'); if (billingLink) billingLink.addEventListener('click', function () { go('#billing'); });
    qa('[data-buy-price]').forEach(function (b) { b.addEventListener('click', function () { if (state.balances.coin < Number(b.getAttribute('data-buy-price'))) { toast('B币不足'); return; } if (!requireAuth('#shop')) return; var price = Number(b.getAttribute('data-buy-price')); openModal('确认购买', '<p>购买 <strong>' + esc(b.getAttribute('data-buy-name')) + '</strong>，扣除 ' + price + ' B币。</p><p>当前余额 ' + state.balances.coin + ' B币，购买后 ' + (state.balances.coin - price) + ' B币。</p>', '取消 ' + button('确认购买', 'primary', 'data-confirm-buy')); q('[data-confirm-buy]').addEventListener('click', function () { state.balances.coin -= price; saveState(); closeModal(); renderShop(); toast('购买成功 · 余额已更新'); }); }); });
    });
}
  function renderBilling() {
    mountTpl('billing', 'billing', function () {

    qa('[data-download-demo]').forEach(function (b) { b.addEventListener('click', function () { actionPending(b, '签名中…', function () { toast('已生成临时下载链接 · 10 分钟有效'); }); }); });
    qa('[data-auth-demo]').forEach(function (b) { b.addEventListener('click', function () { toast('授权详情：有效至明天 18:10'); }); });
    });
}
  function renderAppeals() {
    mountTpl('appeals', 'appeals', function () {
      var me = (state.account && state.account.displayName) || 'Chaos';
      var cases = REPORTS.filter(function (r) { return r.accused === me && r.status === 'resolved' && r.action; });
      var caseHost = q('[data-my-cases]');
      if (caseHost) caseHost.innerHTML = cases.length ? cases.map(function (r) {
        var ap = state.appeals && state.appeals[r.id];
        var badge = ap ? '<span class="sbadge sb-hot">申诉处理中</span>' : '<span class="sbadge sb-danger">可申诉</span>';
        var act = ap ? button('申诉处理中', 'ghost', 'disabled') : button('提交申诉', 'primary', 'data-open-appeal="' + r.id + '"');
        return '<div class="app-post-row"><span class="app-post-row__main"><h3>' + r.id + ' · ' + esc(r.title) + '</h3><p>处罚：' + esc(r.action) + ' · 举报人 ' + esc(r.reporter) + ' · ' + esc(r.time || '近期') + '</p><p class="app-muted">处罚原因：' + esc(r.reason) + '</p>' + (ap ? '<p class="app-muted">申诉 ' + esc(ap.no) + ' 已提交 · ' + esc(ap.time) + ' · 预计 48 小时内完成复核</p>' : '') + '</span><span class="app-post-row__right">' + badge + ' ' + act + '</span></div>';
      }).join('') : '<div class="app-empty">' + icon('inbox', 'ic-16') + '<b>没有处罚案件</b><span>保持记录干净，继续参与社区</span></div>';
      var myReports = Object.keys(state.reports || {}).filter(function (k) { return k.indexOf('local-') === 0; }).map(function (k) { return state.reports[k]; });
      var repHost = q('[data-my-reports]');
      if (repHost) repHost.innerHTML = myReports.length ? myReports.map(function (r) {
        return '<div class="app-post-row"><span class="app-post-row__main"><h3>' + (r.id || '举报') + ' · ' + esc(r.title || '内容举报') + '</h3><p>提交于 ' + esc(r.time || '刚刚') + ' · 处理状态可在下方时间线查看</p>' + (r.timeline && r.timeline.length ? '<p class="app-muted">' + esc(r.timeline[r.timeline.length - 1]) + '</p>' : '') + '</span><span class="app-post-row__right"><span class="sbadge ' + (r.status === 'pending' ? 'sb-hot' : 'sb-success') + '">' + (REPORT_STATUS[r.status] || '待处理') + '</span></span></div>';
      }).join('') : '<div class="app-empty">' + icon('inbox', 'ic-16') + '<b>还没有提交过举报</b><span>在帖子或回复的「举报」按钮提交，处理进度会显示在这里</span></div>';
      qa('[data-open-appeal]').forEach(function (b) { b.addEventListener('click', function () {
        var id = b.getAttribute('data-open-appeal');
        openModal('提交申诉 · ' + id, '<label class="app-field-label">申诉理由 <span class="app-required">*</span></label><textarea class="app-textarea" aria-label="申诉理由" data-appeal-reason placeholder="请说明你希望复核的事实"></textarea>', '取消 ' + button('提交申诉', 'primary', 'data-submit-appeal'));
        q('[data-submit-appeal]').addEventListener('click', function () {
          var reason = q('[data-appeal-reason]').value.trim();
          if (!reason) { toast('请填写申诉理由'); return; }
          state.appeals = state.appeals || {};
          state.appeals[id] = { no: 'S-' + (1001 + Object.keys(state.appeals).length), time: '刚刚', reason: reason, status: 'processing' };
          saveState();
          closeModal();
          toast('申诉已提交 · ' + state.appeals[id].no + ' · 状态：处理中');
          route();
        });
      }); });
    });
}
  function renderMfa() {
    var enabled = state.toggles.mfa === true;
    
    var recoveryForm = card('恢复码', '<p class="app-muted">启用后可生成一次性恢复码；生成前需要重新认证。</p>' + button('重新生成恢复码', 'ghost', 'data-recovery-codes'));
    mountTpl('mfa', 'mfa', function () {
      var mfaEnabled = state.toggles.mfa === true;
      var mfaBtn = q('[data-enable-mfa]');
      if (mfaBtn && mfaEnabled) { mfaBtn.textContent = '已启用'; mfaBtn.disabled = true; }

    var enable = q('[data-enable-mfa]');
    if (enable) enable.addEventListener('click', function () { var code = (q('[data-mfa-code]') || {}).value || ''; if (!/^\d{6}$/.test(code)) { toast('请输入 6 位验证码'); return; } state.toggles.mfa = true; saveState(); renderMfa(); toast('TOTP 已启用（演示）'); });
    var recovery = q('[data-recovery-codes]');
    if (recovery) recovery.addEventListener('click', function () { if (!state.toggles.mfa) { toast('请先启用两步验证'); return; } openModal('恢复码仅显示一次', '<p class="app-mono">ABCD-EFGH · JKMN-PQRS · TUVW-XY12</p><p class="app-muted">请立即保存，关闭后不能再次查看。</p>', '我已保存'); });
    });
}

  function adminSide(active) {
    var activeGroup = ADMIN_GROUPS.map(function (group, index) {
      return group[1].indexOf(active) >= 0 ? index : -1;
    }).filter(function (index) { return index >= 0; })[0];
    return '<div class="app-admin-side__brand">BBLBB Admin</div>' + ADMIN_GROUPS.map(function (group, index) {
      var expanded = index === activeGroup;
      var panelId = 'admin-group-items-' + index;
      return '<div class="app-admin-side__group' + (expanded ? ' is-expanded' : '') + '" data-admin-group>' +
        '<button type="button" class="app-admin-side__label" data-admin-group-toggle aria-expanded="' + expanded + '" aria-controls="' + panelId + '">' +
          '<span>' + group[0] + '</span>' + icon('chevron-down', 'ic-12') +
        '</button>' +
        '<div class="app-admin-side__items" id="' + panelId + '">' +
          group[1].map(function (href) { var item = ADMIN_ITEMS.find(function (x) { return x[0] === href; }); return '<a class="' + (href === active ? 'is-active' : '') + '" href="' + href + '">' + icon(item ? item[2] : 'grid') + (item ? item[1] : href) + '</a>'; }).join('') +
        '</div>' +
      '</div>';
    }).join('') + '<div class="app-admin-side__foot">' + (state.role === 'admin' ? '管理员演示态' : '普通成员演示态') + ' · 所有变更均需真实 API、权限和审计确认</div>';
  }
  function adminShell(active, title, lede, content, actions) {
    mount('admin', '<div class="app-admin-shell"><aside class="app-admin-side">' + adminSide(active) + '</aside><div class="app-admin-main"><h1 class="sr-only">' + esc(title) + '</h1>' + (actions ? '<div class="admin-route-actions">' + actions + '</div>' : '') + content + '</div></div>');
    ensureAdminMenu();
  }
  function renderAdminDashboard() {
    setDocumentTitle('管理后台');
    mountTpl('admin', 'admin', function () {

    var dashboard = q('.app-admin-main');
    if (dashboard && !q('[data-dashboard-period]', dashboard)) {
      var period = state.dashboardPeriod || '今日';
      var tabs = document.createElement('div');
      tabs.className = 'app-filter-tabs dashboard-period-tabs';
      tabs.setAttribute('role', 'tablist');
      tabs.setAttribute('aria-label', '仪表盘周期');
      tabs.innerHTML = ['今日', '本周', '本月', '今年'].map(function (p) { return '<button type="button" role="tab" aria-selected="' + (p === period) + '" class="' + (p === period ? 'is-active' : '') + '" data-dashboard-period="' + p + '">' + p + '</button>'; }).join('');
      dashboard.insertBefore(tabs, dashboard.querySelector('.app-stat-grid'));
      qa('[data-dashboard-period]', dashboard).forEach(function (b) { b.addEventListener('click', function () { state.dashboardPeriod = b.getAttribute('data-dashboard-period'); saveState(); logAudit('切换仪表盘周期', state.dashboardPeriod); renderAdminDashboard(); }); });
      var stats = qa('.app-stat', dashboard);
      var values = { '今日': ['1,284', '56', '3', '421'], '本周': ['1,284', '342', '12', '964'], '本月': ['1,284', '1,260', '38', '1,052'], '今年': ['1,284', '14,320', '286', '1,284'] }[period];
      stats.forEach(function (s, i) { if (values[i]) { var n = q('b', s); if (n) n.textContent = values[i]; } });
      var health = document.createElement('section'); health.className = 'app-card admin-dashboard-trend'; health.innerHTML = '<header class="app-card__head"><div><h2>运营趋势</h2><p>按周期查看内容、活跃和审核变化</p></div><span class="sbadge sb-success">实时 Mock</span></header><div class="app-card__body"><div class="dashboard-sparkline" role="img" aria-label="运营趋势图"><i style="height:28%"></i><i style="height:42%"></i><i style="height:35%"></i><i style="height:58%"></i><i style="height:52%"></i><i style="height:76%"></i><i style="height:68%"></i><i style="height:88%"></i></div><div class="dashboard-sparkline__labels"><span>周一</span><span>周二</span><span>周三</span><span>周四</span><span>周五</span><span>周六</span><span>周日</span></div></div>'; dashboard.insertBefore(health, dashboard.querySelector('.app-card'));
      var trendLabels = q('.dashboard-sparkline__labels', health); if (trendLabels) trendLabels.remove();
    }

    qa('[data-go-admin]').forEach(function (b) { b.addEventListener('click', function () { go('#admin-' + b.getAttribute('data-go-admin')); }); });
    q('[data-switch-role]').addEventListener('click', function () { state.role = state.role === 'admin' ? 'member' : 'admin'; saveState(); toast(state.role === 'admin' ? '已切换为管理员' : '已切换为普通成员'); });
    });
}
  var REPORTS = [
    { id: 'R-1020', status: 'resolved', prio: '中优先级', prioCls: 'sb-hot', title: '广告 / 垃圾信息', reporter: 'Nina', accused: 'Chaos', board: 'rust', topic: '#topic:201', content: '【内容摘要】帖子被判定含有站外商业推广信息。', action: '禁言 7 天', reason: '违反社区规范 §3：不得发布站外商业推广内容', time: '昨天 16:40', timeline: ['提交举报 · 前天 09:12', '处理完成 · 禁言 7 天 · 昨天 16:40'] },
    { id: 'R-1024', status: 'pending', prio: '高优先级', prioCls: 'sb-danger', title: '广告 / 垃圾信息', reporter: 'Yuwen', accused: 'Alice', board: 'rust', topic: '#topic:201', content: '【精彩广告】某商业产品大促销！现在购买 8 折优惠…', timeline: ['提交举报 · 1 小时前', '自动分配给 Chaos · 45 分钟前'] },
    { id: 'R-1023', status: 'pending', prio: '中优先级', prioCls: 'sb-hot', title: '人身攻击', reporter: 'Nina', accused: 'spam_bot_42', board: '闲聊', topic: '#topic:201', content: '【内容摘要】相关正文与处理上下文…', timeline: ['提交举报 · 2 小时前', '自动分配给 Chaos · 1 小时前'] },
    { id: 'R-1022', status: 'processing', prio: '中优先级', prioCls: 'sb-hot', title: '垃圾广告', reporter: 'Yuwen', accused: 'Alice', board: 'rust', topic: '#topic:201', content: '【内容摘要】相关正文与处理上下文…', timeline: ['提交举报 · 昨天', 'Chaos 开始复核 · 昨天 18:30'] },
    { id: 'R-1021', status: 'resolved', prio: '中优先级', prioCls: 'sb-hot', title: '垃圾广告', reporter: 'Yuwen', accused: 'spam_bot_42', board: 'web-dev', topic: '#topic:201', content: '【内容摘要】相关正文与处理上下文…', timeline: ['提交举报 · 2 天前', '隐藏内容 · 2 天前', '禁言 7 天 · 已记录'], action: '禁言 7 天', reason: '垃圾广告 · 违反社区规范' },
    { id: 'R-1018', status: 'rejected', prio: '低优先级', prioCls: 'sb-gray', title: '内容争议', reporter: 'Mark', accused: 'Nina', board: 'web-dev', topic: '#topic:201', content: '【内容摘要】相关正文与处理上下文…', timeline: ['提交举报 · 5 天前', '驳回 · 证据不足 · 5 天前'], action: '驳回举报', reason: '证据不足' }
  ];
  var REPORT_STATUS = { pending: '待处理', processing: '处理中', resolved: '已处理', rejected: '已驳回' };
  function reportMeta(id) { var hit = REPORTS.filter(function (r) { return r.id === id; })[0]; return hit || null; }
  function reportStatus(id) { var meta = reportMeta(id); if (!meta) return 'pending'; var saved = state.reports[id]; return saved && saved.status ? saved.status : meta.status; }
  function renderReports(route) {
    if (route !== '#admin-reports') {
      var id = route.slice('#admin-report:'.length);
      var meta = reportMeta(id);
      if (!meta) { renderGenericAdmin(route); return; }
      var report = state.reports[id] || { status: meta.status, reason: meta.reason || '', action: meta.action || '', timeline: meta.timeline.slice() };
      var openCase = report.status === 'pending' || report.status === 'processing';
      var statusCls = report.status === 'resolved' ? 'sb-success' : report.status === 'rejected' ? 'sb-gray' : 'sb-hot';
      var detailContent = '<div class="app-grid"><div class="app-stack">' + card('原内容预览', '<blockquote class="topic-prose">' + esc(meta.content) + '</blockquote><p>' + link('查看原帖', meta.topic) + '</p>') + (openCase ? card('举报原因与处罚', '<p>举报人 ' + meta.reporter + ' · 原因：<b>' + esc(meta.title) + '</b></p><label class="app-field-label">处理原因 <span class="app-required">*</span></label><textarea class="app-textarea" data-report-reason placeholder="必填，写入审计日志"></textarea><div class="admin-action-row" style="margin-top:12px">' + button('隐藏内容', 'ghost', 'data-report-action="隐藏内容"') + button('禁言 7 天', 'danger', 'data-report-action="禁言 7 天"') + button('驳回举报', 'ghost', 'data-report-action="驳回举报"') + button('提交处理', 'primary', 'data-report-submit') + '</div>') : card('处理结果', '<p>结果：<span class="sbadge ' + statusCls + '">' + REPORT_STATUS[report.status] + '</span> ' + esc(report.action || meta.action || '') + '</p><p class="app-muted">原因：' + esc(report.reason || meta.reason || '—') + '</p>') ) + card('处理时间线', '<ul class="admin-timeline" data-report-timeline>' + report.timeline.map(function (x) { return '<li>' + esc(x) + '</li>'; }).join('') + '</ul>') + '</div><aside class="app-stack">' + card('案件信息', '<p>举报单号 <code>' + id + '</code></p><p>板块 ' + link(meta.board, '#board:' + meta.board) + '</p><p>负责人 Chaos</p><p>状态：<span class="sbadge ' + statusCls + '">' + REPORT_STATUS[report.status] + '</span></p>') + card('申诉状态', '<p class="app-muted">处罚后用户可从申诉中心提交复核；申诉结果必须追加到审计时间线。</p>') + '</aside></div>';
      mountTpl('admin', 'admin-report', function () {
        var reportMain = q('.app-admin-main');
        if (reportMain) reportMain.innerHTML = '<h1 class="sr-only">举报 ' + esc(id) + '</h1>' + detailContent;
        qa('[data-report-action]').forEach(function (b) { b.addEventListener('click', function () { toast('已选择 ' + b.getAttribute('data-report-action') + '，请填写处理原因'); q('[data-report-reason]').focus(); }); });
        var submit = q('[data-report-submit]');
        if (submit) submit.addEventListener('click', function () { var reason = q('[data-report-reason]').value.trim(); if (!reason) { q('[data-report-reason]').classList.add('is-error'); toast('请填写处理原因'); return; } openModal('确认处理', '<p>将处理 ' + id + ' 并写入不可变审计时间线。</p><p class="app-muted">原因：' + esc(reason) + '</p>', '取消 ' + button('确认处理', 'danger', 'data-confirm-report')); q('[data-confirm-report]').addEventListener('click', function () { report.status = 'resolved'; report.action = '禁言 7 天'; report.reason = reason; report.timeline.push('禁言 7 天 · ' + reason + ' · 刚刚'); state.reports[id] = report; saveState(); closeModal(); renderReports(route); toast('处理完成 · 审计已记录'); }); });
      });
      return;
    }
    var counts = { pending: 0, processing: 0, resolved: 0, rejected: 0 };
    REPORTS.forEach(function (r) { counts[reportStatus(r.id)] += 1; });
    var rows = REPORTS.map(function (r) { var st = reportStatus(r.id); var stCls = st === 'pending' ? 'sb-hot' : st === 'processing' ? 'sb-brand' : st === 'resolved' ? 'sb-success' : 'sb-gray'; return '<a class="admin-report" href="#admin-report:' + r.id + '" data-report-row="' + st + '"><div class="admin-report__top"><code>' + r.id + '</code><span class="sbadge ' + r.prioCls + '">' + r.prio + '</span><span class="sbadge ' + stCls + '">' + REPORT_STATUS[st] + '</span></div><blockquote>' + esc(r.title) + ' 内容摘要与处理上下文</blockquote><div class="admin-report__foot">举报人 ' + r.reporter + ' · 被举报 ' + r.accused + ' <span class="app-spacer"></span>' + (st === 'resolved' || st === 'rejected' ? '查看' : '处理') + '</div></a>'; }).join('');
    mountTpl('admin', 'admin-reports', function () {
      var reportsTabs = q('.app-filter-tabs'); if (reportsTabs) reportsTabs.innerHTML = [['all', '全部'], ['pending', '待处理'], ['processing', '处理中'], ['resolved', '已处理'], ['rejected', '已驳回']].map(function (t, i) { var n = t[0] === 'all' ? REPORTS.length : counts[t[0]]; return '<button class="' + (i === 0 ? 'is-active' : '') + '" role="tab" aria-selected="' + (i === 0 ? 'true' : 'false') + '" data-report-tab="' + t[0] + '">' + t[1] + ' ' + n + '</button>'; }).join('');
      var reportsStack = q('.app-stack'); if (reportsStack) reportsStack.innerHTML = rows;
      qa('[data-report-tab]').forEach(function (tab) {
        tab.addEventListener('click', function () {
          qa('[data-report-tab]').forEach(function (x) { x.classList.remove('is-active'); x.setAttribute('aria-selected', 'false'); });
          tab.classList.add('is-active'); tab.setAttribute('aria-selected', 'true');
          applyReportFilter(tab.getAttribute('data-report-tab'));
          if (typeof window.__reportBulkSync === 'function') window.__reportBulkSync();
        });
      });
      applyReportFilter('all');
      bindReportBulk();
    });
  }

  function bindReportBulk() {
    var host = q('.app-admin-main');
    if (!host) return;
    qa('[data-report-batch-bar]').forEach(function (oldBar) { oldBar.remove(); });
    var cards = qa('.admin-report', host);
    var reportsStack = cards.length ? cards[0].parentNode : q('.app-stack', host);
    var existingToolbar = q('[data-report-bulk-toolbar]', host);
    if (existingToolbar) existingToolbar.remove();
    var toolbar = document.createElement('div');
    toolbar.className = 'report-bulk-toolbar';
    toolbar.setAttribute('data-report-bulk-toolbar', '');
    toolbar.innerHTML = '<label class="report-bulk-toolbar__select"><input type="checkbox" data-report-select-all aria-label="全选当前列表"><span>全选当前列表</span></label><span class="report-bulk-toolbar__count"><b data-report-batch-count>0</b> 项已选</span><span class="report-bulk-toolbar__hint">选择举报后执行批量操作</span><div class="report-bulk-toolbar__actions"><button type="button" class="btn ghost" data-report-batch-action="processing" disabled>标记处理中</button><button type="button" class="btn ghost" data-report-batch-action="resolved" disabled>批量关闭</button><button type="button" class="btn danger" data-report-batch-action="rejected" disabled>批量驳回</button></div>';
    if (reportsStack) reportsStack.parentNode.insertBefore(toolbar, reportsStack);
    cards.forEach(function (card) {
      if (q('[data-report-batch]', card)) return;
      card.style.position = 'relative';
      var label = document.createElement('label');
      label.className = 'admin-report__select';
      label.innerHTML = '<input type="checkbox" data-report-batch="' + esc(card.getAttribute('href').replace('#admin-report:', '')) + '" aria-label="选择此举报">';
      label.addEventListener('click', function (e) { e.stopPropagation(); });
      card.insertBefore(label, card.firstElementChild);
    });
    var selected = function () { return qa('[data-report-batch]', host).filter(function (c) { return c.checked && !c.closest('.admin-report').hidden; }); };
    var allChecks = function () { return qa('[data-report-batch]', host).filter(function (c) { return !c.closest('.admin-report').hidden; }); };
    var sync = function () {
      var picked = selected();
      var n = picked.length;
      var count = q('[data-report-batch-count]', toolbar);
      if (count) count.textContent = n;
      qa('[data-report-batch-action]', toolbar).forEach(function (btn) { btn.disabled = n === 0; });
      var selectAll = q('[data-report-select-all]', toolbar);
      var available = allChecks();
      if (selectAll) { selectAll.checked = available.length > 0 && available.every(function (c) { return c.checked; }); selectAll.indeterminate = n > 0 && n < available.length; }
      qa('[data-report-batch]', host).forEach(function (c) { c.closest('.admin-report').classList.toggle('is-selected', c.checked); });
    };
    window.__reportBulkSync = sync;
    qa('[data-report-batch]', host).forEach(function (c) { c.addEventListener('change', sync); });
    var selectAll = q('[data-report-select-all]', toolbar);
    if (selectAll) selectAll.addEventListener('change', function () { allChecks().forEach(function (c) { c.checked = selectAll.checked; }); sync(); });
    qa('[data-report-batch-action]', toolbar).forEach(function (btn) { btn.onclick = function () { var picked = selected(); if (!picked.length) return; var next = btn.getAttribute('data-report-batch-action'); var title = next === 'resolved' ? '批量关闭举报' : next === 'rejected' ? '批量驳回举报' : '批量标记处理中'; var label = next === 'resolved' ? '确认关闭' : next === 'rejected' ? '确认驳回' : '确认标记'; confirmAdmin(title, '确认处理选中的 ' + picked.length + ' 个举报单？', label, function () { picked.forEach(function (c) { var id = c.getAttribute('data-report-batch'); var r = state.reports[id] || {}; r.status = next; r.action = next === 'resolved' ? '批量关闭举报' : next === 'rejected' ? '驳回举报' : '开始复核'; r.timeline = (r.timeline || []).concat((next === 'resolved' ? '批量关闭' : next === 'rejected' ? '批量驳回' : '批量开始复核') + ' · 刚刚'); state.reports[id] = r; }); saveState(); logAudit('批量处理举报', picked.length + ' 个 · ' + next); renderReports('#admin-reports'); toast('举报批量处理完成 · 已写入审计'); }); }; });
    sync();
  }

  function applyReportFilter(key) {
    var visible = 0;
    qa('[data-report-row]').forEach(function (row) { var st = row.getAttribute('data-report-row'); var ok = key === 'all' || st === key; row.hidden = !ok; if (!ok) { var checkbox = q('[data-report-batch]', row); if (checkbox) checkbox.checked = false; } if (ok) visible += 1; });
    var emptyEl = q('[data-report-empty]');
    if (emptyEl) emptyEl.hidden = visible > 0;
    if (typeof window.__reportBulkSync === 'function') window.__reportBulkSync();
  }
  function renderPoints() {
    mountTpl('admin', 'admin-points', function () {
      ensurePointsSummaryMarkup();
      pointsSyncResult();
      var accountInput = q('[data-points-user]');
      var accountCard = accountInput ? accountInput.closest('.app-card') : null;
      if (accountCard) accountCard.remove();
      ensurePointsLedgerMarkup();
      renderPointsLedger();
      wirePointsAdjust();
    });
  }

  function ensurePointsSummaryMarkup() {
    var main = q('#page-admin .app-admin-main');
    if (!main || q('[data-points-result]', main)) return;
    main.insertAdjacentHTML('afterbegin', '<section class="app-card"><header class="app-card__head"><h2>账户积分</h2></header><div class="app-card__body"><div data-points-result></div></div></section>');
  }
  function pointsSyncResult() {
    var host = q('[data-points-result]');
    if (host) host.innerHTML = '<div class="app-profile" style="margin-top:12px">' + profileBadge('Chaos', 'C') + '<div><h2>Chaos</h2><p>经验 ' + state.balances.exp + ' · B币 ' + state.balances.coin + ' · 贡献 ' + state.balances.contrib + '</p></div>' + button('调整积分', 'primary', 'data-adjust-points data-adjust-user="Chaos"') + '</div>';
  }

  function pointUser(name) {
    var user = USERS.filter(function (item) { return item.name.toLowerCase() === String(name || '').toLowerCase(); })[0];
    if (!user) return null;
    state.pointsBalances = state.pointsBalances || {};
    var data = state.pointsBalances[user.name] || { coin: user.coin, exp: user.exp, contrib: user.contrib };
    if (user.name === 'Chaos') data = state.balances;
    return { name: user.name, data: data };
  }

  function ensurePointsLedgerMarkup() {
    var cards = qa('#page-admin .app-admin-main .app-card');
    var cardEl = cards.filter(function (card) { return !!q('[data-points-ledger]', card); })[0] || cards[0];
    if (!cardEl) return;
    cardEl.setAttribute('data-points-ledger-app', '');
    var head = q('.app-card__head', cardEl);
    var body = q('.app-card__body', cardEl);
    if (!head || !body) return;
    head.innerHTML = '<div><h2>全站流水</h2><p class="app-muted">默认展示所有账号的积分、经验与 B币变动，按时间倒序排列</p></div><span class="app-muted" data-points-ledger-count>共 0 条</span>';
    body.innerHTML = '<div class="app-toolbar points-ledger-filters"><label class="app-form-field"><span class="app-field-label">账号</span><select class="app-select" aria-label="按账号筛选" data-ledger-account><option value="">全部账号</option><option>Chaos</option><option>Yuwen</option><option>Mark</option><option>Alice</option><option>Reo</option><option>Nina</option></select></label><label class="app-form-field"><span class="app-field-label">资产</span><select class="app-select" aria-label="按资产筛选" data-ledger-asset><option value="">全部资产</option><option value="exp">经验</option><option value="coin">B币</option><option value="contrib">贡献</option></select></label><label class="app-form-field"><span class="app-field-label">类型</span><select class="app-select" aria-label="按流水类型筛选" data-ledger-type><option value="">全部类型</option><option value="credit">获得</option><option value="debit">消耗</option><option value="adjust">管理员调整</option></select></label><label class="app-form-field"><span class="app-field-label">开始日期</span><input class="app-field" type="date" aria-label="开始日期" data-ledger-from></label><label class="app-form-field"><span class="app-field-label">结束日期</span><input class="app-field" type="date" aria-label="结束日期" data-ledger-to></label><label class="app-form-field points-ledger-keyword"><span class="app-field-label">关键词</span><input class="app-field" data-ledger-q aria-label="搜索流水" placeholder="搜索行为或来源"></label><button type="button" class="btn secondary" data-ledger-filter>查询</button><button type="button" class="btn ghost" data-ledger-clear>清除</button></div><div class="app-table-wrap"><table class="app-table"><thead><tr><th>账号</th><th>行为</th><th>资产</th><th>变化</th><th>类型</th><th>来源</th><th>时间</th></tr></thead><tbody data-points-ledger></tbody></table><div class="app-empty" data-points-ledger-empty hidden><b>没有匹配的流水</b><span>请调整账号、时间或类型筛选条件后重试。</span></div></div>';
  }

  function wirePointsAdjust() {
    var adjust = q('[data-adjust-points]');
    if (!adjust || adjust.__wired) return;
    adjust.__wired = true;
    adjust.addEventListener('click', function () { var userName = adjust.getAttribute('data-adjust-user') || 'Chaos'; var target = pointUser(userName); if (!target) return; openModal('调整积分 · ' + userName, '<label class="app-field-label">币种</label><select class="app-select" data-adjust-kind><option value="coin">B币</option><option value="exp">经验</option><option value="contrib">贡献</option></select><label class="app-field-label" style="margin-top:12px">增加数额</label><input class="app-field" aria-label="调整数额" type="number" value="50" data-adjust-amount><label class="app-field-label" style="margin-top:12px">原因 <span class="app-required">*</span></label><textarea class="app-textarea" aria-label="调整原因" data-adjust-reason placeholder="必填，流水不可删除"></textarea>', '取消 ' + button('下一步', 'primary', 'data-confirm-adjust')); q('[data-confirm-adjust]').addEventListener('click', function () { var reason = q('[data-adjust-reason]').value.trim(); if (!reason) { toast('请填写原因'); return; } var amount = Number(q('[data-adjust-amount]').value) || 0; if (amount <= 0) { toast('数额必须大于 0'); return; } var kind = q('[data-adjust-kind]').value; openModal('二次确认', '<p>历史流水不可删除，仅可创建补偿记录。</p><p>确认给 ' + esc(userName) + ' 增加 ' + amount + '？</p>', '取消 ' + button('确认调整', 'primary', 'data-final-adjust')); q('[data-final-adjust]').addEventListener('click', function () { target.data[kind] += amount; if (userName === 'Chaos') state.balances = target.data; state.pointsBalances = state.pointsBalances || {}; state.pointsBalances[userName] = target.data; pushLedger('admin_adjust', amount, '管理员调整 · ' + reason, userName, kind); saveState(); closeModal(); renderPoints(); toast('调整已生效 · 流水已追加'); }); }); });
  }
  function renderLevels() {
    var DEFAULT_LEVELS = [
      { code: 'Lv0', name: 'Lv.0', replyCount: 0, postCount: 0, likesGiven: 0, likesReceived: 0, loginDays: 0, attachment: 2, dailyPosts: 10, perks: '2 MB 附件', mentionOfficial: false, moderatorApply: false, enabled: true },
      { code: 'Lv1', name: 'Lv.1', replyCount: 3, postCount: 1, likesGiven: 1, likesReceived: 1, loginDays: 3, attachment: 5, dailyPosts: 15, perks: '5 MB 附件', mentionOfficial: false, moderatorApply: false, enabled: true },
      { code: 'Lv2', name: 'Lv.2', replyCount: 10, postCount: 3, likesGiven: 5, likesReceived: 3, loginDays: 7, attachment: 10, dailyPosts: 20, perks: '10 MB 附件 · 可 @ 官方账号', mentionOfficial: true, moderatorApply: false, enabled: true },
      { code: 'Lv3', name: 'Lv.3', replyCount: 30, postCount: 8, likesGiven: 15, likesReceived: 10, loginDays: 30, attachment: 20, dailyPosts: 30, perks: '20 MB 附件 · 版主申请', mentionOfficial: true, moderatorApply: true, enabled: true },
      { code: 'Lv4', name: 'Lv.4', replyCount: 80, postCount: 20, likesGiven: 40, likesReceived: 30, loginDays: 90, attachment: 50, dailyPosts: 50, perks: '50 MB 附件', mentionOfficial: true, moderatorApply: true, enabled: true }
    ];
    mountTpl('admin', 'admin-levels', function () {
    var saved = state.levelConfigs || {};
    var levels = DEFAULT_LEVELS.map(function (level) { return Object.assign({}, level, saved[level.code] || {}); });
    var counts = {};
    ADM_ADMIN_USERS.forEach(function (user) { var code = userLevelCodeOf(user); counts[code] = (counts[code] || 0) + 1; });
    var total = ADM_ADMIN_USERS.length;
    var totalEl = q('[data-level-total]'); if (totalEl) totalEl.textContent = '共 ' + total + ' 名用户';
    var tbody = q('.app-level-table tbody');
    if (tbody) tbody.innerHTML = levels.map(function (level) {
      var count = counts[level.code.toLowerCase()] || 0;
      var activity = '回帖 ' + Number(level.replyCount || 0) + ' · 发帖 ' + Number(level.postCount || 0) + ' · 送赞 ' + Number(level.likesGiven || 0) + ' · 获赞 ' + Number(level.likesReceived || 0) + ' · 登录 ' + Number(level.loginDays || 0) + ' 天';
      return '<tr data-level-row="' + esc(level.code) + '"><td><button type="button" class="app-link app-level-name" data-level-settings="' + esc(level.code) + '">' + esc(level.name) + '</button></td><td class="level-activity-cell">' + activity + '</td><td><a class="app-link app-level-user-count" href="#admin-users:' + esc(level.code) + '">' + count + ' 名</a></td><td>' + esc(level.perks || ((level.attachment || 0) + ' MB 附件')) + '</td><td><span class="sbadge ' + (level.enabled === false ? 'sb-gray' : 'sb-success') + '">' + (level.enabled === false ? '停用' : '启用') + '</span></td><td><button type="button" class="btn ghost sm" data-level-settings="' + esc(level.code) + '">详细设置</button></td></tr>';
    }).join('');
    qa('[data-level-settings]').forEach(function (trigger) {
      trigger.addEventListener('click', function () {
        var code = trigger.getAttribute('data-level-settings');
        var level = levels.filter(function (item) { return item.code === code; })[0]; if (!level) return;
        var body = '<p class="app-muted">等级按用户活跃度评估，以下条件需同时满足；调整后会影响自动升级判定，不会自动修改历史用户等级。</p><div class="app-form-grid"><div class="app-form-field"><label class="app-field-label">等级名称</label><input class="app-field" data-level-field="name" value="' + esc(level.name) + '"></div><div class="app-form-field"><label class="app-field-label">回帖数 ≥</label><input class="app-field" type="number" min="0" data-level-field="replyCount" value="' + esc(level.replyCount) + '"></div><div class="app-form-field"><label class="app-field-label">发帖数 ≥</label><input class="app-field" type="number" min="0" data-level-field="postCount" value="' + esc(level.postCount) + '"></div><div class="app-form-field"><label class="app-field-label">送赞数 ≥</label><input class="app-field" type="number" min="0" data-level-field="likesGiven" value="' + esc(level.likesGiven) + '"></div><div class="app-form-field"><label class="app-field-label">获赞数 ≥</label><input class="app-field" type="number" min="0" data-level-field="likesReceived" value="' + esc(level.likesReceived) + '"></div><div class="app-form-field"><label class="app-field-label">登录天数 ≥</label><input class="app-field" type="number" min="0" data-level-field="loginDays" value="' + esc(level.loginDays) + '"></div><div class="app-form-field"><label class="app-field-label">单文件附件上限（MB）</label><input class="app-field" type="number" min="0" data-level-field="attachment" value="' + esc(level.attachment) + '"></div><div class="app-form-field"><label class="app-field-label">每日发帖上限</label><input class="app-field" type="number" min="0" data-level-field="dailyPosts" value="' + esc(level.dailyPosts) + '"></div><div class="app-form-field app-form-field--full"><label class="app-field-label">权益说明</label><textarea class="app-textarea" data-level-field="perks">' + esc(level.perks) + '</textarea></div></div><div class="app-radio-list" style="margin-top:14px"><label><input type="checkbox" data-level-field="mentionOfficial"' + (level.mentionOfficial ? ' checked' : '') + '> 可 @ 官方账号</label><label><input type="checkbox" data-level-field="moderatorApply"' + (level.moderatorApply ? ' checked' : '') + '> 可申请版主</label><label><input type="checkbox" data-level-field="enabled"' + (level.enabled !== false ? ' checked' : '') + '> 启用此等级</label></div>';
        openModal(level.name + ' · 详细设置', body, '取消 ' + button('保存设置', 'primary', 'data-save-level'));
        q('[data-save-level]').addEventListener('click', function () {
          var name = q('[data-level-field="name"]').value.trim(); var activityKeys = ['replyCount', 'postCount', 'likesGiven', 'likesReceived', 'loginDays']; var activity = {}; activityKeys.forEach(function (key) { activity[key] = Number(q('[data-level-field="' + key + '"]').value); }); var attachment = Number(q('[data-level-field="attachment"]').value); var dailyPosts = Number(q('[data-level-field="dailyPosts"]').value);
          if (!name || activityKeys.some(function (key) { return !Number.isFinite(activity[key]) || activity[key] < 0; }) || !Number.isFinite(attachment) || attachment < 0 || !Number.isFinite(dailyPosts) || dailyPosts < 0) { toast('请填写有效的等级参数'); return; }
          state.levelConfigs = state.levelConfigs || {};
          state.levelConfigs[code] = Object.assign({ code: code, name: name, attachment: attachment, dailyPosts: dailyPosts, perks: q('[data-level-field="perks"]').value.trim(), mentionOfficial: q('[data-level-field="mentionOfficial"]').checked, moderatorApply: q('[data-level-field="moderatorApply"]').checked, enabled: q('[data-level-field="enabled"]').checked }, activity);
          saveState(); logAudit('更新等级配置', name + ' · ' + code); closeModal(); renderLevels(); toast(name + ' 配置已保存 · 已写入审计');
        });
      });
    });
    });
}
  var DEFAULT_ACHIEVEMENTS = [
    { code: 'first_reply', title: '初次发言', category: '社区参与', icon: '✦', conditionType: 'reply_count', condition: '发表首条回复', target: 1, rewardExp: 10, rewardCoin: 0, unlocked: 642, status: 'enabled', visibility: 'public', notify: true, repeatable: false, description: '完成第一次有意义的社区互动。' },
    { code: 'featured_contributor', title: '精华贡献者', category: '内容创作', icon: '✹', conditionType: 'featured', condition: '主题被设为精华', target: 1, rewardExp: 200, rewardCoin: 50, unlocked: 86, status: 'enabled', visibility: 'public', notify: true, repeatable: false, description: '持续创作并贡献高质量内容。' },
    { code: 'weekly_writer', title: '坚持输出', category: '内容创作', icon: '✎', conditionType: 'post_count', condition: '发布 5 篇公开主题', target: 5, rewardExp: 80, rewardCoin: 20, unlocked: 124, status: 'enabled', visibility: 'public', notify: true, repeatable: false, description: '在社区保持稳定的创作节奏。' },
    { code: 'secret_event', title: '???', category: '特别', icon: '◈', conditionType: 'server', condition: '仅服务端条件', target: 1, rewardExp: 0, rewardCoin: 0, unlocked: 0, status: 'disabled', visibility: 'hidden', notify: false, repeatable: false, description: '由服务端事件触发的隐藏成就。' }
  ];
  function achievementDefs() {
    var saved = Array.isArray(state.achievementDefs) ? state.achievementDefs : null;
    return saved || DEFAULT_ACHIEVEMENTS.map(function (a) { return Object.assign({}, a); });
  }
  function userLevelCodeOf(user) {
    var saved = state.levelConfigs || {};
    var rules = [
      { code: 'Lv0', replyCount: 0, postCount: 0, likesGiven: 0, likesReceived: 0, loginDays: 0 },
      { code: 'Lv1', replyCount: 3, postCount: 1, likesGiven: 1, likesReceived: 1, loginDays: 3 },
      { code: 'Lv2', replyCount: 10, postCount: 3, likesGiven: 5, likesReceived: 3, loginDays: 7 },
      { code: 'Lv3', replyCount: 30, postCount: 8, likesGiven: 15, likesReceived: 10, loginDays: 30 },
      { code: 'Lv4', replyCount: 80, postCount: 20, likesGiven: 40, likesReceived: 30, loginDays: 90 }
    ];
    var qualified = 'Lv0';
    rules.forEach(function (rule) {
      var current = Object.assign({}, rule, saved[rule.code] || {});
      var ok = ['replyCount', 'postCount', 'likesGiven', 'likesReceived', 'loginDays'].every(function (key) { return Number(user[key] || 0) >= Number(current[key] || 0); });
      if (ok && current.enabled !== false) qualified = rule.code;
    });
    return qualified.toLowerCase();
  }
  function achievementStatus(a) { return a.status === 'enabled' ? '已启用' : '已停用'; }
  function achievementStatusClass(a) { return a.status === 'enabled' ? 'sb-success' : 'sb-gray'; }
  function achievementConditionLabel(a) {
    if (a.conditionType === 'reply_count') return '发表首条回复';
    if (a.conditionType === 'post_count') return '发布 ' + (a.target || 1) + ' 篇公开主题';
    if (a.conditionType === 'login_days') return '累计签到 ' + (a.target || 1) + ' 天';
    if (a.conditionType === 'likes_received') return '获得 ' + (a.target || 1) + ' 个赞';
    if (a.conditionType === 'featured') return '主题被设为精华';
    return a.condition || '仅服务端条件';
  }
  function achievementRewardLabel(a) {
    var rewards = [];
    if (Number(a.rewardExp)) rewards.push('+' + Number(a.rewardExp) + ' 经验');
    if (Number(a.rewardCoin)) rewards.push('+' + Number(a.rewardCoin) + ' B币');
    return rewards.length ? rewards.join(' · ') : '无奖励';
  }
  function achievementForm(a) {
    a = a || { code: '', title: '', category: '社区参与', icon: '✦', conditionType: 'reply_count', condition: '', target: 1, rewardExp: 10, rewardCoin: 0, visibility: 'public', notify: true, repeatable: false, status: 'enabled', description: '' };
    var isNew = !a.code;
    return '<div class="achievement-editor">' +
      '<div class="app-notice achievement-editor__notice">保存后会立即影响新产生的解锁事件；已发放的奖励不会被回收。</div>' +
      '<div class="achievement-section"><span class="achievement-section__eyebrow">01 / 基本信息</span><div class="adm-form-grid">' +
      '<div class="app-form-field"><label class="app-field-label">唯一 code <span class="app-required">*</span></label><input class="app-field" data-ach-field="code" value="' + esc(a.code) + '" placeholder="例如 weekly_writer" ' + (isNew ? '' : 'readonly') + '><span class="app-field-help">用于服务端事件匹配，创建后不可修改。</span></div>' +
      '<div class="app-form-field"><label class="app-field-label">图标</label><input class="app-field" data-ach-field="icon" value="' + esc(a.icon || '✦') + '" placeholder="✦"></div>' +
      '<div class="app-form-field"><label class="app-field-label">成就标题 <span class="app-required">*</span></label><input class="app-field" data-ach-field="title" value="' + esc(a.title) + '" placeholder="例如坚持输出"></div>' +
      '<div class="app-form-field"><label class="app-field-label">分类</label><select class="app-select" data-ach-field="category"><option ' + (a.category === '社区参与' ? 'selected' : '') + '>社区参与</option><option ' + (a.category === '内容创作' ? 'selected' : '') + '>内容创作</option><option ' + (a.category === '特别' ? 'selected' : '') + '>特别</option><option ' + (a.category === '活动' ? 'selected' : '') + '>活动</option></select></div>' +
      '<div class="app-form-field app-form-field--full"><label class="app-field-label">前台描述</label><textarea class="app-textarea" data-ach-field="description" placeholder="告诉成员如何获得这个成就">' + esc(a.description || '') + '</textarea></div></div></div>' +
      '<div class="achievement-section"><span class="achievement-section__eyebrow">02 / 解锁条件</span><div class="adm-form-grid">' +
      '<div class="app-form-field"><label class="app-field-label">条件类型</label><select class="app-select" data-ach-field="conditionType"><option value="reply_count" ' + (a.conditionType === 'reply_count' ? 'selected' : '') + '>回复数量</option><option value="post_count" ' + (a.conditionType === 'post_count' ? 'selected' : '') + '>公开主题数量</option><option value="login_days" ' + (a.conditionType === 'login_days' ? 'selected' : '') + '>连续签到天数</option><option value="likes_received" ' + (a.conditionType === 'likes_received' ? 'selected' : '') + '>收到点赞数</option><option value="featured" ' + (a.conditionType === 'featured' ? 'selected' : '') + '>内容被设精华</option><option value="server" ' + (a.conditionType === 'server' ? 'selected' : '') + '>服务端事件</option></select></div>' +
      '<div class="app-form-field"><label class="app-field-label">达成目标值</label><input class="app-field" type="number" min="1" data-ach-field="target" value="' + esc(a.target || 1) + '"><span class="app-field-help">服务端事件类型无需填写。</span></div>' +
      '<div class="app-form-field app-form-field--full"><label class="app-field-label">条件说明</label><input class="app-field" data-ach-field="condition" value="' + esc(a.condition || '') + '" placeholder="用于后台和审计记录的可读说明"></div></div></div>' +
      '<div class="achievement-section"><span class="achievement-section__eyebrow">03 / 奖励设置</span><div class="adm-form-grid">' +
      '<div class="app-form-field"><label class="app-field-label">经验奖励</label><div class="achievement-input-suffix"><input class="app-field" type="number" min="0" data-ach-field="rewardExp" value="' + esc(a.rewardExp || 0) + '"><span>经验</span></div></div>' +
      '<div class="app-form-field"><label class="app-field-label">B币奖励</label><div class="achievement-input-suffix"><input class="app-field" type="number" min="0" data-ach-field="rewardCoin" value="' + esc(a.rewardCoin || 0) + '"><span>B币</span></div></div>' +
      '<label class="app-check"><input type="checkbox" data-ach-field="repeatable" ' + (a.repeatable ? 'checked' : '') + '>允许重复获得</label><span class="app-field-help achievement-help-inline">重复奖励需谨慎，建议仅用于周期性活动。</span></div></div>' +
      '<div class="achievement-section"><span class="achievement-section__eyebrow">04 / 展示与通知</span><div class="adm-form-grid">' +
      '<div class="app-form-field"><label class="app-field-label">前台可见性</label><select class="app-select" data-ach-field="visibility"><option value="public" ' + (a.visibility === 'public' ? 'selected' : '') + '>公开展示</option><option value="members" ' + (a.visibility === 'members' ? 'selected' : '') + '>仅登录成员</option><option value="hidden" ' + (a.visibility === 'hidden' ? 'selected' : '') + '>隐藏（解锁后展示）</option></select></div>' +
      '<label class="app-check"><input type="checkbox" data-ach-field="notify" ' + (a.notify ? 'checked' : '') + '>解锁后发送站内通知</label></div></div>' +
      '<div class="achievement-editor__footer-note">变更将记录到审计日志 · 仅管理员可编辑成就定义</div></div>';
  }
  function readAchievementForm(root) {
    var out = {};
    qa('[data-ach-field]', root).forEach(function (el) { var key = el.getAttribute('data-ach-field'); out[key] = el.type === 'checkbox' ? el.checked : (el.type === 'number' ? Number(el.value || 0) : el.value.trim()); });
    return out;
  }
  function openAchievementEditor(existing) {
    var isNew = !existing;
    var modal = openModal(isNew ? '新建成就' : '编辑成就 · ' + existing.title, achievementForm(existing), button('取消', 'ghost', 'data-modal-close') + ' ' + button(isNew ? '创建成就' : '保存更改', 'primary', 'data-ach-save'));
    var modalPanel = q('.app-modal', modal); if (modalPanel) modalPanel.classList.add('app-modal--wide');
    q('[data-ach-save]', modal).addEventListener('click', function () {
      var form = readAchievementForm(modal);
      if (!form.code || !form.title) { toast('请填写 code 和成就标题'); return; }
      var defs = achievementDefs();
      if (isNew && defs.some(function (x) { return x.code === form.code; })) { toast('code 已存在，请换一个'); return; }
      var next = Object.assign({ unlocked: 0, status: 'enabled' }, existing || {}, form);
      if (isNew) defs.unshift(next); else defs = defs.map(function (x) { return x.code === existing.code ? next : x; });
      state.achievementDefs = defs; saveState(); logAudit(isNew ? '创建成就' : '编辑成就', next.code + ' · ' + next.title); closeModal(); renderAchievementsAdmin(); toast(isNew ? '成就已创建 · 已写入审计' : '成就配置已保存 · 已写入审计');
    });
  }
  function openAchievementMore(a) {
    var body = '<div class="achievement-detail"><div class="achievement-detail__hero"><span class="achievement-icon">' + esc(a.icon || '✦') + '</span><div><span class="sbadge ' + achievementStatusClass(a) + '">' + achievementStatus(a) + '</span><h3>' + esc(a.title) + '</h3><code>' + esc(a.code) + '</code></div></div><div class="achievement-detail__stats"><div><b>' + Number(a.unlocked || 0).toLocaleString() + '</b><span>已解锁</span></div><div><b>' + esc(achievementRewardLabel(a).split(' · ')[0]) + '</b><span>单次奖励</span></div><div><b>' + (a.visibility === 'hidden' ? '隐藏' : '公开') + '</b><span>前台展示</span></div></div><div class="achievement-detail__rule"><span>解锁条件</span><b>' + esc(achievementConditionLabel(a)) + '</b></div><div class="admin-action-row">' + button('编辑配置', 'primary', 'data-ach-edit-more') + button('手工授予', 'ghost', 'data-ach-grant') + button(a.status === 'enabled' ? '停用成就' : '启用成就', a.status === 'enabled' ? 'danger' : 'secondary', 'data-ach-toggle-more') + '</div><p class="app-muted">手工授予和状态变更都会写入审计日志；已解锁成员不会受到停用影响。</p></div>';
    var drawer = openDrawer('成就详情', body, button('关闭', 'ghost', 'data-drawer-close'));
    q('[data-ach-edit-more]', drawer).addEventListener('click', function () { closeDrawer(); openAchievementEditor(a); });
    q('[data-ach-toggle-more]', drawer).addEventListener('click', function () { var defs = achievementDefs(); defs.forEach(function (x) { if (x.code === a.code) x.status = x.status === 'enabled' ? 'disabled' : 'enabled'; }); state.achievementDefs = defs; saveState(); logAudit(a.status === 'enabled' ? '停用成就' : '启用成就', a.code); closeDrawer(); renderAchievementsAdmin(); toast('成就状态已更新 · 已写入审计'); });
    q('[data-ach-grant]', drawer).addEventListener('click', function () { closeDrawer(); openModal('手工授予成就', '<p>选择成员后将立即写入成就记录并发放一次奖励。</p><label class="app-field-label">成员用户名</label><input class="app-field" data-ach-grant-user placeholder="例如 Chaos"><p class="app-muted">奖励：' + esc(achievementRewardLabel(a)) + '</p>', '取消 ' + button('确认授予', 'primary', 'data-ach-grant-confirm')); q('[data-ach-grant-confirm]').addEventListener('click', function () { var user = (q('[data-ach-grant-user]').value || '').trim() || 'Chaos'; logAudit('手工授予成就', a.code + ' → ' + user); closeModal(); toast('已授予 ' + user + ' · 已写入审计'); }); });
  }
  function renderAchievementsAdmin() { mountTpl('admin', 'admin-achievements', function () {
    var host = q('[data-achievement-app]'); if (!host) return;
    /* 成就页也必须使用完整后台导航，避免模板侧栏与其他管理页不一致。 */
    var side = q('.app-admin-side'); if (side) side.innerHTML = adminSide('#admin-achievements');
    var defs = achievementDefs();
    var enabled = defs.filter(function (a) { return a.status === 'enabled'; }).length;
    var unlocked = defs.reduce(function (sum, a) { return sum + Number(a.unlocked || 0); }, 0);
    host.innerHTML = '<div class="achievement-stats"><div class="achievement-stat"><span>成就总数</span><b>' + defs.length + '</b><em>已配置定义</em></div><div class="achievement-stat"><span>启用中</span><b>' + enabled + '</b><em>实时参与判定</em></div><div class="achievement-stat"><span>累计解锁</span><b>' + unlocked.toLocaleString() + '</b><em>所有成员合计</em></div><div class="achievement-stat"><span>待完善</span><b>' + defs.filter(function (a) { return a.conditionType === 'server'; }).length + '</b><em>服务端事件</em></div></div>' +
      '<section class="app-card achievement-list-card"><header class="app-card__head"><div><h2>成就定义</h2><p>共 ' + defs.length + ' 项 · 支持按名称、分类和状态查找</p></div><div class="achievement-card-actions"><span class="sbadge sb-brand">配置中心</span><button type="button" class="btn ghost sm" data-ach-export>导出配置</button><button type="button" class="btn primary sm" data-create-achievement>+ 新建成就</button></div></header><div class="app-card__body"><div class="app-toolbar achievement-toolbar"><label class="app-search"><span class="sr-only">搜索成就</span><span>⌕</span><input data-admin-filter placeholder="搜索 code、标题或条件" aria-label="搜索成就"></label><select class="app-select admin-unified-filter__status" data-admin-status aria-label="按状态筛选"><option value="">全部状态</option><option>已启用</option><option>已停用</option></select><select class="app-select achievement-category-filter" data-ach-category aria-label="按分类筛选"><option value="">全部分类</option><option>社区参与</option><option>内容创作</option><option>特别</option><option>活动</option></select><button type="button" class="btn ghost" data-admin-filter-clear>清除</button></div><div class="app-table-wrap"><table class="app-table achievement-table"><thead><tr><th><input type="checkbox" data-ach-select-all aria-label="全选当前列表"></th><th>成就</th><th>分类</th><th>解锁条件</th><th>奖励</th><th>状态</th><th>操作</th></tr></thead><tbody>' + defs.map(function (a) { return '<tr data-ach-row data-ach-id="' + esc(a.code) + '"><td><input type="checkbox" data-ach-select="' + esc(a.code) + '" aria-label="选择 ' + esc(a.title) + '"></td><td><div class="achievement-cell"><span class="achievement-icon achievement-icon--sm">' + esc(a.icon || '✦') + '</span><span><code>' + esc(a.code) + '</code><b>' + esc(a.title) + '</b><small>' + esc(a.description || '暂无描述') + '</small></span></div></td><td>' + esc(a.category) + '</td><td>' + esc(achievementConditionLabel(a)) + '</td><td>' + esc(achievementRewardLabel(a)) + '</td><td><span class="sbadge ' + achievementStatusClass(a) + '">' + achievementStatus(a) + '</span></td><td><div class="achievement-actions">' + button('编辑', 'ghost sm', 'data-ach-edit="' + esc(a.code) + '"') + button('更多', 'ghost sm', 'data-ach-more="' + esc(a.code) + '"') + '</div></td></tr>'; }).join('') + '</tbody></table></div></div><footer class="app-card__foot"><span>所有配置变更都会追加到审计日志</span><button type="button" class="btn ghost" data-ach-refresh>刷新列表</button></footer></section>';
    q('[data-create-achievement]').onclick = function () { openAchievementEditor(); };
    q('[data-ach-export]').onclick = function () { if (adminCsv('bblbb-achievements.csv', ['code', '标题', '分类', '条件', '奖励', '状态'], defs.map(function (a) { return [a.code, a.title, a.category, achievementConditionLabel(a), achievementRewardLabel(a), achievementStatus(a)]; }))) { logAudit('导出成就配置', defs.length + ' 项'); toast('成就配置已导出'); } };
    qa('[data-ach-edit]').forEach(function (b) { b.onclick = function () { var a = defs.filter(function (x) { return x.code === b.getAttribute('data-ach-edit'); })[0]; if (a) openAchievementEditor(a); }; });
    qa('[data-ach-more]').forEach(function (b) { b.onclick = function () { var a = defs.filter(function (x) { return x.code === b.getAttribute('data-ach-more'); })[0]; if (a) openAchievementMore(a); }; });
    q('[data-ach-refresh]').onclick = function () { renderAchievementsAdmin(); toast('列表已刷新'); };
    var applyFilter = function () { var term = (q('[data-admin-filter]').value || '').toLowerCase(); var status = q('[data-admin-status]').value; var category = q('[data-ach-category]').value; qa('[data-ach-row]').forEach(function (row) { var a = defs.filter(function (x) { return x.code === row.getAttribute('data-ach-id'); })[0]; var text = row.textContent.toLowerCase(); row.hidden = !!((term && text.indexOf(term) < 0) || (status && achievementStatus(a) !== status) || (category && a.category !== category)); }); syncSelect(); };
    var selected = function () { return qa('[data-ach-select]', host).filter(function (x) { return x.checked && !x.closest('tr').hidden; }); };
    var syncSelect = function () { var picked = selected(); qa('[data-ach-row]', host).forEach(function (row) { row.classList.toggle('is-selected', !!q('[data-ach-select]:checked', row)); }); var master = q('[data-ach-select-all]', host); var visible = qa('[data-ach-row]', host).filter(function (r) { return !r.hidden; }); master.checked = visible.length > 0 && visible.every(function (r) { return q('[data-ach-select]', r).checked; }); master.indeterminate = picked.length > 0 && !master.checked; var bar = q('[data-ach-batch]', host); if (!bar) { bar = document.createElement('div'); bar.className = 'achievement-batch'; bar.setAttribute('data-ach-batch', ''); host.appendChild(bar); } bar.innerHTML = picked.length ? '<b>' + picked.length + ' 项已选中</b><button type="button" class="btn ghost sm" data-ach-batch-action="enable">批量启用</button><button type="button" class="btn ghost sm" data-ach-batch-action="disable">批量停用</button><button type="button" class="btn danger sm" data-ach-batch-action="archive">归档</button>' : ''; bar.hidden = !picked.length; qa('[data-ach-batch-action]', bar).forEach(function (btn) { btn.onclick = function () { var action = btn.getAttribute('data-ach-batch-action'); confirmAdmin(action === 'archive' ? '归档成就' : '批量更新状态', '确认对选中的 ' + picked.length + ' 项执行此操作？', action === 'archive' ? '确认归档' : '确认更新', function () { var codes = picked.map(function (x) { return x.value || x.getAttribute('data-ach-select'); }); state.achievementDefs = defs.filter(function (a) { if (codes.indexOf(a.code) < 0) return true; if (action === 'archive') return false; a.status = action === 'enable' ? 'enabled' : 'disabled'; return true; }); saveState(); logAudit(action === 'archive' ? '归档成就' : '批量更新成就状态', codes.join('、')); renderAchievementsAdmin(); toast('批量操作完成 · 已写入审计'); }); }; }); };
    q('[data-ach-select-all]').onchange = function () { qa('[data-ach-row]', host).forEach(function (row) { if (!row.hidden) q('[data-ach-select]', row).checked = this.checked; }.bind(this)); syncSelect(); };
    qa('[data-ach-select]').forEach(function (x) { x.onchange = syncSelect; });
    q('[data-admin-filter]').oninput = applyFilter; q('[data-admin-status]').onchange = applyFilter; q('[data-ach-category]').onchange = applyFilter; q('[data-admin-filter-clear]').onclick = function () { q('[data-admin-filter]').value = ''; q('[data-admin-status]').value = ''; q('[data-ach-category]').value = ''; applyFilter(); };
    applyFilter();
  }); }
  function renderThemesAdmin() { mountTpl('admin', 'admin-themes', function () {
 qa('[data-theme-preview]').forEach(function (b) { b.addEventListener('click', function () { openModal('主题预览', '<div class="admin-token-preview"></div><p>预览不会改变当前主题。</p>', '关闭'); }); }); qa('[data-theme-apply]').forEach(function (b) { b.addEventListener('click', function () { openModal('应用主题', '<p>切换后全站立即生效，是否继续？</p>', '取消 ' + button('确认应用', 'primary', 'data-confirm-theme')); q('[data-confirm-theme]').addEventListener('click', function () { closeModal(); window.toggleTheme(); toast('主题已切换'); }); }); });   });
}
  var PLUGIN_DEFS = [
    { id: 'video-embed', name: '视频嵌入', type: '配置型', version: '1.2.0', latestVersion: '1.3.0', status: 'enabled', owner: '内容展示', updated: '今天 10:24', description: '将白名单来源的视频安全嵌入帖子。', permissions: '读取帖子内容、读取白名单配置' },
    { id: 'search-highlight', name: '搜索高亮', type: '配置型', version: '2.0.1', latestVersion: '2.0.1', status: 'enabled', owner: '搜索', updated: '昨天 16:40', description: '为搜索结果标记匹配关键词，提升检索效率。', permissions: '读取搜索词、读取公开内容' },
    { id: 'stats-card', name: '统计卡片', type: '预编译 UI', version: '0.8.1', latestVersion: '0.9.0', status: 'error', owner: '运营看板', updated: '今天 09:18', description: '在内容页展示阅读与互动统计。', permissions: '读取内容统计', error: 'provider timeout · retry 2/3 · fallback enabled' },
    { id: 'notice-bar', name: '公告栏', type: '预编译 UI', version: '1.4.0', latestVersion: '1.5.0', status: 'enabled', owner: '站务', updated: '2026-08-30', description: '在站点顶部展示重要公告与维护提示。', permissions: '读取公告内容' },
    { id: 'markdown-preview', name: 'Markdown 预览', type: '配置型', version: '1.1.0', latestVersion: '1.1.0', status: 'disabled', owner: '内容编辑', updated: '2026-08-28', description: '在编辑器中提供安全的 Markdown 实时预览。', permissions: '读取草稿内容', installed: false },
    { id: 'emoji-reactions', name: '表情回应', type: '预编译 UI', version: '1.0.0', latestVersion: '1.0.0', status: 'disabled', owner: '社区互动', updated: '2026-08-26', description: '为帖子和回复提供轻量表情回应。', permissions: '读取帖子与回复 ID', installed: false }
  ];
  function pluginVersionCompare(a, b) {
    var left = String(a || '0').split('.').map(Number), right = String(b || '0').split('.').map(Number);
    for (var i = 0; i < 3; i += 1) { if ((left[i] || 0) !== (right[i] || 0)) return (left[i] || 0) - (right[i] || 0); }
    return 0;
  }
  function pluginHasUpdate(plugin) { return pluginVersionCompare(plugin.latestVersion, plugin.version) > 0; }
  function pluginState() {
    var saved = state.pluginStates || {};
    return PLUGIN_DEFS.filter(function (plugin) { return saved[plugin.id] ? saved[plugin.id].installed !== false : plugin.installed !== false; }).map(function (plugin) { return Object.assign({}, plugin, saved[plugin.id] || {}); });
  }
  function pluginAvailable() {
    var saved = state.pluginStates || {};
    return PLUGIN_DEFS.filter(function (plugin) { return saved[plugin.id] ? saved[plugin.id].installed === false : plugin.installed === false; }).map(function (plugin) { return Object.assign({}, plugin, saved[plugin.id] || {}); });
  }
  function pluginStatusLabel(status) { return status === 'enabled' ? '启用' : status === 'disabled' ? '停用' : status === 'error' ? '错误' : '未知'; }
  function pluginStatusClass(status) { return status === 'enabled' ? 'sb-success' : status === 'disabled' ? 'sb-gray' : status === 'error' ? 'sb-danger' : 'sb-hot'; }
  function renderPluginStats(host, plugins) {
    var enabled = plugins.filter(function (p) { return p.status === 'enabled'; }).length;
    var errors = plugins.filter(function (p) { return p.status === 'error'; }).length;
    host.innerHTML = '<div class="plugin-stats"><div class="plugin-stat"><span>插件总数</span><b>' + plugins.length + '</b><em>已注册插件</em></div><div class="plugin-stat"><span>运行中</span><b>' + enabled + '</b><em>前台正常提供服务</em></div><div class="plugin-stat"><span>错误</span><b class="' + (errors ? 'is-danger' : '') + '">' + errors + '</b><em>' + (errors ? '需要查看日志' : '暂无异常') + '</em></div><div class="plugin-stat"><span>最近检查</span><b class="plugin-stat__text">刚刚</b><em>本地 Mock 投影</em></div></div>';
  }
  function renderPluginsAdmin() { mountTpl('admin', 'admin-plugins', function () {
    var host = q('[data-plugin-app]');
    if (!host) return;
    var plugins = pluginState();
    var activeFilter = { query: '', status: '' };
    var statusOptions = '<option value="">全部状态</option><option value="enabled">启用</option><option value="disabled">停用</option><option value="error">错误</option>';
    function draw() {
      var list = pluginState();
      renderPluginStats(q('[data-plugin-stats]', host), list);
      var query = activeFilter.query.toLowerCase();
      var visible = list.filter(function (p) { return (!query || (p.name + ' ' + p.type + ' ' + p.owner + ' ' + p.description).toLowerCase().indexOf(query) >= 0) && (!activeFilter.status || p.status === activeFilter.status); });
      var rows = visible.map(function (p) { return '<tr data-plugin-row="' + esc(p.id) + '"><td><div class="plugin-cell"><span class="plugin-cell__icon">' + icon(p.type === '配置型' ? 'settings' : 'puzzle', 'ic-16') + '</span><span><b>' + esc(p.name) + '</b><small>' + esc(p.description) + '</small></span></div></td><td>' + esc(p.type) + '</td><td class="app-mono">' + esc(p.version) + (pluginHasUpdate(p) ? '<small class="plugin-update-hint">可更新至 ' + esc(p.latestVersion) + '</small>' : '') + '</td><td><span class="sbadge ' + pluginStatusClass(p.status) + '">' + pluginStatusLabel(p.status) + '</span>' + (p.status === 'error' ? '<small class="plugin-error-hint">运行异常</small>' : '') + '</td><td><div class="plugin-actions">' + button('详情', 'ghost sm', 'data-plugin-detail="' + esc(p.id) + '"') + (pluginHasUpdate(p) ? button('更新', 'secondary sm', 'data-plugin-update="' + esc(p.id) + '"') : '') + (p.status === 'error' ? button('查看日志', 'ghost sm', 'data-plugin-log="' + esc(p.id) + '"') : button(p.status === 'enabled' ? '停用' : '启用', p.status === 'enabled' ? 'ghost sm' : 'secondary sm', 'data-plugin-toggle="' + esc(p.id) + '"')) + button('删除', 'danger sm', 'data-plugin-uninstall="' + esc(p.id) + '"') + '</div></td></tr>'; }).join('');
      q('[data-plugin-tbody]', host).innerHTML = rows || '<tr><td colspan="5">' + empty('search', '没有匹配的插件', '修改关键词或状态筛选后重试。') + '</td></tr>';
      q('[data-plugin-count]', host).textContent = '显示 ' + visible.length + ' / ' + list.length + ' 个插件';
      qa('[data-plugin-toggle]', host).forEach(function (btn) { btn.onclick = function () { var p = pluginState().filter(function (x) { return x.id === btn.getAttribute('data-plugin-toggle'); })[0]; if (!p) return; var next = p.status === 'enabled' ? 'disabled' : 'enabled'; openModal(next === 'disabled' ? '停用插件' : '启用插件', '<p>确定要' + (next === 'disabled' ? '停用' : '启用') + '「' + esc(p.name) + '」吗？</p><p class="app-muted">相关区块将' + (next === 'disabled' ? '不再渲染' : '恢复渲染') + '，操作会写入审计日志。</p>', '取消 ' + button('确认' + (next === 'disabled' ? '停用' : '启用'), next === 'disabled' ? 'danger' : 'primary', 'data-plugin-confirm')); q('[data-plugin-confirm]').onclick = function () { state.pluginStates = state.pluginStates || {}; state.pluginStates[p.id] = Object.assign({}, state.pluginStates[p.id] || {}, { status: next, changedAt: Date.now() }); saveState(); logAudit(next === 'disabled' ? '停用插件' : '启用插件', p.name + ' · v' + p.version); closeModal(); draw(); toast('插件已' + (next === 'disabled' ? '停用' : '启用') + ' · 已写入审计'); }; }; });
      qa('[data-plugin-update]', host).forEach(function (btn) { btn.onclick = function () { var p = pluginState().filter(function (x) { return x.id === btn.getAttribute('data-plugin-update'); })[0]; if (!p || !pluginHasUpdate(p)) return; openModal('更新插件 · ' + p.name, '<p>将插件从 <b>v' + esc(p.version) + '</b> 更新到 <b>v' + esc(p.latestVersion) + '</b>。</p><p class="app-muted">更新期间该插件会短暂停止提供服务，完成后恢复原状态。</p>', '取消 ' + button('确认更新', 'primary', 'data-plugin-confirm-update')); q('[data-plugin-confirm-update]').onclick = function () { state.pluginStates = state.pluginStates || {}; state.pluginStates[p.id] = Object.assign({}, state.pluginStates[p.id] || {}, { version: p.latestVersion, updated: '刚刚', status: p.status === 'error' ? 'enabled' : p.status }); saveState(); logAudit('更新插件', p.name + ' · v' + p.version + ' → v' + p.latestVersion); closeModal(); draw(); toast('插件已更新至 v' + p.latestVersion + ' · 已写入审计'); }; }; });
      qa('[data-plugin-uninstall]', host).forEach(function (btn) { btn.onclick = function () { var p = pluginState().filter(function (x) { return x.id === btn.getAttribute('data-plugin-uninstall'); })[0]; if (!p) return; openModal('删除插件 · ' + p.name, '<p>确认删除「' + esc(p.name) + '」吗？</p><p class="app-text-danger">删除后将从已安装列表移除，插件配置也会被清理，且无法撤销。你仍可稍后从插件目录重新安装。</p>', '取消 ' + button('确认删除', 'danger', 'data-plugin-confirm-uninstall')); q('[data-plugin-confirm-uninstall]').onclick = function () { state.pluginStates = state.pluginStates || {}; state.pluginStates[p.id] = Object.assign({}, state.pluginStates[p.id] || {}, { installed: false }); saveState(); logAudit('删除插件', p.name + ' · v' + p.version); closeModal(); draw(); toast('插件已删除 · 已写入审计'); }; }; });
      qa('[data-plugin-detail]', host).forEach(function (btn) { btn.onclick = function () { var p = pluginState().filter(function (x) { return x.id === btn.getAttribute('data-plugin-detail'); })[0]; if (!p) return; openModal('插件详情 · ' + p.name, '<div class="plugin-detail"><div class="plugin-detail__hero"><span class="plugin-cell__icon">' + icon(p.type === '配置型' ? 'settings' : 'puzzle', 'ic-18') + '</span><div><h3>' + esc(p.name) + '</h3><p>' + esc(p.description) + '</p></div><span class="sbadge ' + pluginStatusClass(p.status) + '">' + pluginStatusLabel(p.status) + '</span></div><dl><div><dt>插件 ID</dt><dd class="app-mono">' + esc(p.id) + '</dd></div><div><dt>版本</dt><dd>' + esc(p.version) + (pluginHasUpdate(p) ? ' · 可更新至 ' + esc(p.latestVersion) : '') + '</dd></div><div><dt>归属模块</dt><dd>' + esc(p.owner) + '</dd></div><div><dt>权限范围</dt><dd>' + esc(p.permissions) + '</dd></div><div><dt>最近更新</dt><dd>' + esc(p.updated) + '</dd></div></dl><p class="app-muted">生产环境需由可信构建、服务端策略和权限系统共同决定插件是否可用。</p></div>', '关闭'); }; });
      qa('[data-plugin-log]', host).forEach(function (btn) { btn.onclick = function () { var p = pluginState().filter(function (x) { return x.id === btn.getAttribute('data-plugin-log'); })[0]; if (!p) return; openModal('插件运行日志 · ' + p.name, '<div class="plugin-log-toolbar"><span class="sbadge sb-danger">ERROR</span><span class="app-muted">最近 24 小时 · request_id req_plg_7f2a</span></div><pre class="app-plugin-log">' + esc(p.error || '暂无错误日志') + '\nprovider=controlled-gateway\nat=2026-09-02T09:18:42+08:00\naction=fallback</pre><p class="app-muted">日志仅用于演示；正式环境请接入结构化日志与 request ID 查询。</p>', '关闭'); }; });
    }
    host.innerHTML = '<div data-plugin-stats></div><section class="app-card plugin-list-card"><header class="app-card__head"><div><h2>插件列表</h2><p>管理已注册插件的状态、权限和运行日志</p></div><div class="plugin-head-actions"><span class="sbadge sb-brand">服务端注册</span><button type="button" class="btn primary sm" data-plugin-install>' + icon('plus', 'ic-14') + '安装插件</button><button type="button" class="btn ghost sm" data-plugin-refresh>' + icon('refresh', 'ic-14') + '刷新</button></div></header><div class="app-card__body"><div class="app-notice plugin-safety-notice">插件包必须来自可信目录；安装、更新和删除都会记录操作者、版本与时间。</div><div class="app-toolbar admin-unified-filter plugin-toolbar"><label class="app-search"><span aria-hidden="true">⌕</span><input data-plugin-search placeholder="搜索插件名称、类型或模块" aria-label="搜索插件"></label><select class="app-select admin-unified-filter__status" data-plugin-status aria-label="按状态筛选">' + statusOptions + '</select><button type="button" class="btn ghost" data-plugin-clear>清除</button></div><div class="plugin-filter-summary"><span data-plugin-count></span><span>变更会自动记录到审计日志</span></div><div class="app-table-wrap"><table class="app-table plugin-table"><thead><tr><th>插件</th><th>类型</th><th>版本</th><th>状态</th><th>操作</th></tr></thead><tbody data-plugin-tbody></tbody></table></div></div><footer class="app-card__foot"><span>不支持上传和执行任意插件代码</span><button type="button" class="btn ghost" data-plugin-export>导出清单</button></footer></section>';
    q('[data-plugin-search]', host).oninput = function (event) { activeFilter.query = event.target.value.trim(); draw(); };
    q('[data-plugin-status]', host).onchange = function (event) { activeFilter.status = event.target.value; draw(); };
    q('[data-plugin-clear]', host).onclick = function () { activeFilter = { query: '', status: '' }; q('[data-plugin-search]', host).value = ''; q('[data-plugin-status]', host).value = ''; draw(); };
    q('[data-plugin-refresh]', host).onclick = function () { draw(); logAudit('刷新插件列表', '插件管理'); toast('插件列表已刷新'); };
    q('[data-plugin-install]', host).onclick = function () { var available = pluginAvailable(); var body = available.length ? '<p class="app-muted">选择一个来自可信目录的插件安装。安装后默认处于停用状态，需要再次启用。</p><div class="plugin-directory">' + available.map(function (p) { return '<div class="plugin-directory__item"><div class="plugin-cell"><span class="plugin-cell__icon">' + icon(p.type === '配置型' ? 'settings' : 'puzzle', 'ic-16') + '</span><span><b>' + esc(p.name) + '</b><small>' + esc(p.description) + '</small><small>v' + esc(p.version) + ' · ' + esc(p.type) + '</small></span></div>' + button('安装', 'secondary sm', 'data-plugin-install-confirm="' + esc(p.id) + '"') + '</div>'; }).join('') + '</div>' : empty('check', '暂无可安装插件', '当前目录中的插件都已安装。'); var dialog = openModal('安装插件', body, '取消'); qa('[data-plugin-install-confirm]', dialog).forEach(function (btn) { btn.onclick = function () { var p = available.filter(function (x) { return x.id === btn.getAttribute('data-plugin-install-confirm'); })[0]; if (!p) return; state.pluginStates = state.pluginStates || {}; state.pluginStates[p.id] = Object.assign({}, state.pluginStates[p.id] || {}, { installed: true, status: 'disabled', version: p.version, updated: '刚刚' }); saveState(); logAudit('安装插件', p.name + ' · v' + p.version); closeModal(); draw(); toast('插件已安装 · 默认停用 · 已写入审计'); }; }); };
    q('[data-plugin-export]', host).onclick = function () { var rows = pluginState().map(function (p) { return [p.name, p.id, p.type, p.version, pluginStatusLabel(p.status), p.owner, p.updated]; }); if (adminCsv('bblbb-plugins.csv', ['插件', '插件 ID', '类型', '版本', '状态', '归属模块', '最近更新'], rows)) { logAudit('导出插件清单', rows.length + ' 项'); toast('插件清单已导出'); } };
    draw();
  }); }
  function renderOAuthAdmin() { mountTpl('admin', 'admin-oauth', function () {
 var createOAuth = q('[data-create-oauth]'); if (createOAuth) createOAuth.addEventListener('click', function () { openModal('创建 OAuth 客户端', '<label class="app-field-label">应用名称</label><input class="app-field" aria-label="应用名称" data-oauth-name placeholder="Demo App"><label class="app-field-label">类型</label><select class="app-select"><option>Confidential</option><option>Public</option></select><label class="app-field-label">Redirect URI</label><input class="app-field" aria-label="Redirect URI" placeholder="https://example.com/callback"><label class="app-field-label">Scopes</label><div class="app-radio-list"><label><input type="checkbox" checked> openid</label><label><input type="checkbox" checked> profile</label><label><input type="checkbox"> email</label></div>', '取消 ' + button('创建', 'primary', 'data-create-oauth-confirm')); q('[data-create-oauth-confirm]').addEventListener('click', function () { var name = q('[data-oauth-name]').value.trim() || 'Demo App'; closeModal(); openModal('凭据仅显示一次', '<p>请立即保存 Client Secret，关闭后不可再次查看。</p><div class="app-secret"><input readonly value="cli_demo_' + Date.now() + '">' + button('复制', 'ghost', 'data-copy-share') + '</div><div class="app-secret"><input readonly value="cs_demo_secret_once">' + button('复制', 'ghost', 'data-copy-share') + '</div>', '我已保存'); toast('客户端 ' + name + ' 已创建'); }); }); qa('[data-oauth-action]').forEach(function (b) { b.addEventListener('click', function () { openModal('确认操作', '<p>该操作会写入审计日志。</p>', '取消 ' + button('确认', 'danger', 'data-confirm-oauth-action')); q('[data-confirm-oauth-action]').addEventListener('click', function () { closeModal(); b.textContent = '已完成'; b.disabled = true; toast('OAuth 状态已更新'); }); }); });   });
}
  function renderStorageAdmin() { mountTpl('admin', 'admin-storage', function () {

    qa('[data-storage-tab]').forEach(function (tab) {
      tab.addEventListener('click', function () {
        qa('[data-storage-tab]').forEach(function (x) { x.classList.remove('is-active'); x.setAttribute('aria-selected', 'false'); });
        tab.classList.add('is-active'); tab.setAttribute('aria-selected', 'true');
        var key = tab.getAttribute('data-storage-tab');
        qa('[data-storage-panel]').forEach(function (panel) { panel.hidden = panel.getAttribute('data-storage-panel') !== key; });
      });
    });
    q('[data-storage-test]').addEventListener('click', function () { actionPending(this, '测试中…', function () { toast('连接测试成功'); logAudit('存储连接测试', 'request_' + Math.random().toString(16).slice(2, 8)); }); });
    q('[data-storage-save]').addEventListener('click', function () {
      var panel = q('[data-storage-panel]:not([hidden])');
      var backend = panel ? panel.getAttribute('data-storage-panel') : 'local';
      var fields = {};
      if (panel) qa('input', panel).forEach(function (i) { fields[i.getAttribute('aria-label') || 'field'] = (i.type === 'password' && !i.value) ? '（保持不变）' : i.value; });
      state.storageConfig = { backend: backend, savedAt: Date.now(), fields: fields };
      saveState(); logAudit('保存存储配置', backend === 's3' ? 'S3 兼容后端' : '本地磁盘后端'); toast('存储配置已保存 · 已写入审计');
    });   });
}
  function renderGenericAdmin(route) { var data = GENERIC_ADMIN[route] || ['后台列表', '按 OpenAPI 字段展示的最小管理占位页。', ['状态|正常', '最近更新|刚刚', '待处理|3 项', '失败重试|可用']]; var rows = data[2].map(function (pair) { var p = pair.split('|'); return '<div class="app-device"><div class="app-device__body"><b>' + esc(p[0]) + '</b><span>Mock projection · 对应 OpenAPI 管理操作</span></div><span class="sbadge ' + (p[1].indexOf('失败') >= 0 ? 'sb-danger' : 'sb-brand') + '">' + esc(p[1]) + '</span></div>'; }).join(''); adminShell(route, data[0], data[1], card(data[0], rows, button('刷新', 'ghost', 'data-admin-refresh') + button('保存设置', 'primary', 'data-admin-save'))); q('[data-admin-refresh]').addEventListener('click', function () { toast('已刷新 · 数据来自 Mock 投影'); }); q('[data-admin-save]').addEventListener('click', function () { toast('设置已保存 · 已追加审计记录'); }); }

  /* ============== 管理后台具体操作层（本地状态 + 审计 + 导出） ==============
   * 目标：每个后台子页面都有可用的操作按钮，而不是只有展示。
   * 约定：所有写操作调用 logAudit() 追加审计；破坏性操作经 confirmAdmin() 二次确认；
   * 状态持久化在 bblbb-prototype-state；列表数据可导出 CSV（UTF-8 BOM）。 */
  function logAudit(text, object) {
    if (!state.auditLog) state.auditLog = [];
    state.auditLog.unshift({ t: Date.now(), actor: 'Chaos', text: text, object: object || '—' });
    if (state.auditLog.length > 300) state.auditLog.length = 300;
    saveState();
  }
  function auditTime(ts) {
    var d = new Date(Number(ts) || Date.now());
    var p = function (n) { return (n < 10 ? '0' : '') + n; };
    return (d.getMonth() + 1) + '-' + d.getDate() + ' ' + p(d.getHours()) + ':' + p(d.getMinutes());
  }
  function adminDownload(filename, mime, content) {
    try {
      var blob = new Blob([content], { type: mime || 'text/plain' });
      var url = URL.createObjectURL(blob);
      var a = document.createElement('a');
      a.href = url; a.download = filename; a.style.display = 'none';
      document.body.appendChild(a); a.click();
      window.setTimeout(function () { if (a.parentNode) a.parentNode.removeChild(a); URL.revokeObjectURL(url); }, 400);
      return true;
    } catch (e) { return false; }
  }
  function adminCsv(filename, headers, rows) {
    var cell = function (v) { return '"' + String(v == null ? '' : v).replace(/"/g, '""') + '"'; };
    var content = '\uFEFF' + [headers].concat(rows).map(function (r) { return r.map(cell).join(','); }).join('\n');
    return adminDownload(filename, 'text/csv;charset=utf-8', content);
  }
  function adminTable(thead, rows) {
    return '<div class="app-table-wrap"><table class="app-table"><thead><tr>' + thead.map(function (h) { return '<th>' + h + '</th>'; }).join('') + '</tr></thead><tbody>' + (Array.isArray(rows) ? rows.join('') : rows) + '</tbody></table></div>';
  }
  var adminActsBound = false;
  function bindAdminActs(handler) {
    window.__admActs = handler;
    if (adminActsBound) return;
    adminActsBound = true;
    document.addEventListener('click', function (event) {
      var el = event.target && event.target.closest ? event.target.closest('[data-adm-act]') : null;
      if (!el || !window.__admActs) return;
      event.preventDefault();
      window.__admActs(el, el.getAttribute('data-adm-act'), el.getAttribute('data-adm-id'));
    });
  }
  function confirmAdmin(title, message, label, onConfirm) {
    openModal(title, '<p>' + message + '</p><p class="app-muted">该操作会写入审计日志，原型内不可撤销。</p>', '取消 ' + button(label, 'danger', 'data-adm-confirm'));
    q('[data-adm-confirm]').addEventListener('click', function () { closeModal(); onConfirm(); });
  }

  function renderPointsLedger() {
    var tbody = q('[data-points-ledger]');
    if (!tbody) return;
    var seed = [
      { user: 'Chaos', kind: '签到', asset: 'exp', amount: 5, source: '系统', time: '2026-09-02T09:12:00' },
      { user: 'Yuwen', kind: '发布主题', asset: 'exp', amount: 10, source: '内容', time: '2026-09-02T08:44:00' },
      { user: 'Nina', kind: '商城消费', asset: 'coin', amount: -45, source: '商城', time: '2026-09-01T21:30:00' },
      { user: 'Mark', kind: '商城消费', asset: 'coin', amount: -30, source: '商城', time: '2026-09-01T20:16:00' },
      { user: 'Alice', kind: '下载附件', asset: 'coin', amount: -10, source: '下载', time: '2026-09-01T18:03:00' },
      { user: 'Reo', kind: '每日签到', asset: 'exp', amount: 5, source: '系统', time: '2026-08-31T09:01:00' }
    ];
    var rows = seed.concat((state.ledger || []).map(function (item) {
      return { user: item.user || 'Chaos', kind: item.kind === 'admin_adjust' ? '管理员调整' : item.kind, asset: item.asset || 'coin', amount: item.amount, source: item.source || '原型演示', time: item.time || new Date().toISOString(), adjustment: item.kind === 'admin_adjust' };
    }));
    var labels = { exp: '经验', coin: 'B币', contrib: '贡献' };
    var values = { account: q('[data-ledger-account]'), asset: q('[data-ledger-asset]'), type: q('[data-ledger-type]'), from: q('[data-ledger-from]'), to: q('[data-ledger-to]'), query: q('[data-ledger-q]') };
    function dateText(value) { var d = new Date(value); return isNaN(d.getTime()) ? value : d.getFullYear() + '-' + String(d.getMonth() + 1).padStart(2, '0') + '-' + String(d.getDate()).padStart(2, '0') + ' ' + String(d.getHours()).padStart(2, '0') + ':' + String(d.getMinutes()).padStart(2, '0'); }
    function draw() {
      var qv = (values.query && values.query.value || '').trim().toLowerCase();
      var filtered = rows.filter(function (item) {
        var date = String(item.time).slice(0, 10);
        var type = item.adjustment ? 'adjust' : item.amount < 0 ? 'debit' : 'credit';
        var hay = [item.user, item.kind, item.source].join(' ').toLowerCase();
        return (!values.account || !values.account.value || item.user === values.account.value) && (!values.asset || !values.asset.value || item.asset === values.asset.value) && (!values.type || !values.type.value || type === values.type.value) && (!values.from || !values.from.value || date >= values.from.value) && (!values.to || !values.to.value || date <= values.to.value) && (!qv || hay.indexOf(qv) >= 0);
      });
      tbody.innerHTML = filtered.map(function (item) {
        var type = item.adjustment ? '管理员调整' : item.amount < 0 ? '消耗' : '获得';
        return '<tr><td>' + esc(item.user) + '</td><td>' + esc(item.kind) + '</td><td>' + esc(labels[item.asset] || item.asset) + '</td><td class="num ' + (item.amount < 0 ? 'app-text-danger' : '') + '">' + (item.amount > 0 ? '+' : '') + item.amount + '</td><td>' + type + '</td><td>' + esc(item.source) + '</td><td>' + dateText(item.time) + '</td></tr>';
      }).join('');
      var count = q('[data-points-ledger-count]'); if (count) count.textContent = '共 ' + filtered.length + ' 条';
      var emptyEl = q('[data-points-ledger-empty]'); if (emptyEl) emptyEl.hidden = filtered.length > 0;
    }
    ['account', 'asset', 'type', 'from', 'to', 'query'].forEach(function (key) { if (values[key]) { values[key].addEventListener(key === 'query' ? 'input' : 'change', draw); } });
    var filter = q('[data-ledger-filter]'); if (filter) filter.addEventListener('click', draw);
    var clear = q('[data-ledger-clear]'); if (clear) clear.addEventListener('click', function () { ['account', 'asset', 'type', 'from', 'to', 'query'].forEach(function (key) { if (values[key]) values[key].value = ''; }); draw(); });
    draw();
  }

  /* 通用后台批量操作：所有管理列表共享选择、计数、导出与二次确认。 */
  function bindAdminSelection(options) {
    var host = options.host || q('.app-admin-main');
    var table = options.table || (host && q('.app-table', host));
    if (!table || !table.tBodies.length) return;
    var tbody = table.tBodies[0];
    var rows = function () { return qa('tr[data-selectable-id]', tbody); };
    qa('tr', tbody).filter(function (row) { return !q('th', row); }).forEach(function (row, index) {
      if (row.getAttribute('data-selectable-id')) return;
      var action = q('[data-adm-id]', row);
      row.setAttribute('data-selectable-id', action ? action.getAttribute('data-adm-id') : 'row-' + (index + 1));
    });
    var headRow = table.tHead && table.tHead.rows[0];
    if (!headRow) headRow = qa('tr', tbody).filter(function (row) { return !!q('th', row); })[0];
    if (!headRow) return;
    if (!q('[data-batch-select-all]', headRow)) {
      var th = document.createElement('th');
      th.className = 'th-select';
      th.innerHTML = '<input type="checkbox" data-batch-select-all aria-label="全选当前列表">';
      headRow.insertBefore(th, headRow.firstElementChild);
    }
    rows().forEach(function (row) {
      if (q('[data-batch-select]', row)) return;
      var td = document.createElement('td');
      td.className = 'td-select';
      td.innerHTML = '<input type="checkbox" data-batch-select="' + esc(row.getAttribute('data-selectable-id')) + '" aria-label="选择此项">';
      row.insertBefore(td, row.firstElementChild);
    });
    qa('[data-batch-bar]').forEach(function (oldBar) { oldBar.remove(); });
    var bar = null;
    if (!bar) {
      bar = document.createElement('div');
      bar.className = 'admin-batch-bar batch-actions-bar';
      bar.setAttribute('data-batch-bar', '');
      bar.setAttribute('role', 'region');
      bar.setAttribute('aria-live', 'polite');
      bar.innerHTML = '<span class="admin-batch-bar__count"><b data-batch-count>0</b> 项已选中</span><div class="admin-batch-bar__actions"></div><button type="button" class="btn-close" data-batch-clear aria-label="清除选择">×</button>';
      document.body.appendChild(bar);
    }
    var actions = q('.admin-batch-bar__actions', bar);
    actions.innerHTML = (options.actions || []).map(function (a) { return button(a.label, a.cls || 'ghost', 'data-batch-action="' + esc(a.action) + '"'); }).join('');
    if (options.exportHeaders && options.exportRows) actions.insertAdjacentHTML('beforeend', button('导出 CSV', 'secondary', 'data-batch-action="export"'));
    var selected = function () { return rows().filter(function (row) { var c = q('[data-batch-select]', row); return c && c.checked && !row.hidden; }); };
    var sync = function () {
      var picked = selected();
      q('[data-batch-count]', bar).textContent = picked.length;
      bar.classList.toggle('admin-batch-bar--visible', picked.length > 0);
      bar.classList.toggle('active', picked.length > 0);
      rows().forEach(function (row) { var c = q('[data-batch-select]', row); row.classList.toggle('is-selected', !!(c && c.checked)); });
      var master = q('[data-batch-select-all]', headRow);
      var visible = rows().filter(function (row) { return !row.hidden; });
      master.checked = visible.length > 0 && visible.every(function (row) { return q('[data-batch-select]', row).checked; });
      master.indeterminate = picked.length > 0 && !master.checked;
    };
    qa('[data-batch-select]', tbody).forEach(function (c) { c.addEventListener('change', sync); });
    q('[data-batch-select-all]', headRow).addEventListener('change', function () {
      var checked = this.checked;
      rows().forEach(function (row) { if (!row.hidden) q('[data-batch-select]', row).checked = checked; });
      sync();
    });
    q('[data-batch-clear]', bar).addEventListener('click', function () { rows().forEach(function (row) { q('[data-batch-select]', row).checked = false; }); sync(); });
    qa('[data-batch-action]', bar).forEach(function (btn) {
      btn.addEventListener('click', function () {
        var picked = selected();
        var action = btn.getAttribute('data-batch-action');
        if (!picked.length) return;
        if (action === 'export') {
          if (adminCsv(options.exportName || 'bblbb-admin-export.csv', options.exportHeaders, picked.map(function (row) { return options.exportRows(row.getAttribute('data-selectable-id')); }))) { logAudit('批量导出列表', picked.length + ' 项'); toast('已导出 ' + picked.length + ' 项'); }
          return;
        }
        var meta = (options.actionMeta || {})[action] || { title: '批量操作', label: '确认操作', message: '确认对选中的 ' + picked.length + ' 项执行此操作？' };
        confirmAdmin(meta.title, meta.message, meta.label, function () {
          picked.forEach(function (row) { if (options.mutate) options.mutate(action, row.getAttribute('data-selectable-id')); });
          if (options.after) options.after(action, picked.length);
        });
      });
    });
    sync();
    window.__adminSelectionSync = sync;
  }

  var adminEnhancementsBound = false;
  function ensureAdminMenu() {
    var shell = q('.app-admin-shell');
    if (!shell) return;
    var side = q('.app-admin-side', shell);
    if (!side) return;
    if (!side.id) side.id = 'admin-navigation';
    enhanceAdminAccordion(side);
    var menu = q('[data-admin-menu]', shell);
    if (!menu) {
      menu = document.createElement('button');
      menu.type = 'button';
      menu.className = 'app-admin-menu';
      menu.setAttribute('data-admin-menu', '');
      menu.setAttribute('aria-controls', side.id);
      menu.setAttribute('aria-expanded', 'false');
      menu.innerHTML = '<span aria-hidden="true">☰</span><span>管理菜单</span>';
      shell.insertBefore(menu, shell.firstElementChild);
      menu.addEventListener('click', function () { var open = shell.classList.toggle('admin-menu-open'); menu.setAttribute('aria-expanded', String(open)); });
      qa('a', side).forEach(function (a) { a.addEventListener('click', function () { shell.classList.remove('admin-menu-open'); menu.setAttribute('aria-expanded', 'false'); }); });
    }
  }

  function enhanceAdminAccordion(side) {
    var groups = qa('.app-admin-side__group', side);
    if (!groups.length) return;

    /* 页面模板里的旧侧栏仍使用 span 标题，这里统一升级成可访问的手风琴结构。 */
    groups.forEach(function (group, index) {
      var label = q('.app-admin-side__label', group);
      if (!label) return;
      var toggle = label.matches('[data-admin-group-toggle]') ? label : null;
      var generatedFromTemplate = !!toggle;
      var items = q('.app-admin-side__items', group);
      if (!toggle) {
        toggle = document.createElement('button');
        toggle.type = 'button';
        toggle.className = 'app-admin-side__label';
        toggle.setAttribute('data-admin-group-toggle', '');
        toggle.innerHTML = '<span>' + label.textContent + '</span>' + icon('chevron-down', 'ic-12');
        label.replaceWith(toggle);
      }
      if (!items) {
        items = document.createElement('div');
        items.className = 'app-admin-side__items';
        items.id = 'admin-group-items-' + index;
        Array.prototype.slice.call(group.querySelectorAll(':scope > a')).forEach(function (link) { items.appendChild(link); });
        group.appendChild(items);
      }
      if (!items.id) items.id = 'admin-group-items-' + index;
      toggle.setAttribute('aria-controls', items.id);
      if (!generatedFromTemplate) toggle.setAttribute('aria-expanded', String(!!q('a.is-active', items)));
      if (!toggle.hasAttribute('aria-expanded')) toggle.setAttribute('aria-expanded', 'false');
      group.classList.toggle('is-expanded', toggle.getAttribute('aria-expanded') === 'true');
      if (toggle.__adminAccordionBound) return;
      toggle.__adminAccordionBound = true;
      toggle.addEventListener('click', function () {
        var open = toggle.getAttribute('aria-expanded') === 'true';
        groups.forEach(function (other) {
          var otherToggle = q('[data-admin-group-toggle]', other);
          if (!otherToggle) return;
          var isCurrent = other === group;
          otherToggle.setAttribute('aria-expanded', String(isCurrent ? !open : false));
          other.classList.toggle('is-expanded', isCurrent ? !open : false);
        });
      });
    });
  }
  function ensureAdminActions(host) {
    var main = q('.app-admin-main', host);
    if (!main || q('.admin-route-actions', main)) return;
    var actions = [];
    if (q('[data-create-achievement]', main)) actions.push(q('[data-create-achievement]', main));
    if (q('[data-create-oauth]', main)) actions.push(q('[data-create-oauth]', main));
    if (q('[data-bi-refresh]', main)) actions.push(q('[data-bi-refresh]', main), q('[data-bi-export]', main));
    if (!actions.length) return;
    var row = document.createElement('div');
    row.className = 'admin-route-actions admin-action-row';
    actions.forEach(function (action) { if (action) row.appendChild(action); });
    main.insertBefore(row, main.firstElementChild && main.firstElementChild.nextElementSibling ? main.firstElementChild.nextElementSibling : main.firstElementChild);
  }
  function setupAdminEnhancements() {
    ensureAdminMenu();
    var host = q('.app-admin-main');
    if (!host) return;
    /* 成就管理使用自己的筛选与批量操作栏，避免通用后台增强重复注入选择列。 */
    if (q('[data-achievement-app]', host) || q('[data-points-ledger-app]', host)) return;
    var table = q('.app-table', host);
    if (!table || !table.tBodies.length) return;
    var cardBody = table.closest('.app-card__body') || table.parentNode;
    var filter = q('[data-admin-filter]', cardBody);
    var legacyFilter = q('[data-users-q]', cardBody);
    if (!filter && legacyFilter) { legacyFilter.setAttribute('data-admin-filter', ''); filter = legacyFilter; }
    if (!filter) {
      var toolbar = document.createElement('div');
      toolbar.className = 'admin-unified-filter app-toolbar';
      toolbar.innerHTML = '<label class="app-search"><span class="sr-only">搜索当前列表</span><input data-admin-filter placeholder="搜索当前列表…" aria-label="搜索当前列表"></label><select class="app-select admin-unified-filter__status" data-admin-status aria-label="按状态筛选"><option value="">全部状态</option><option>待审核</option><option>公开</option><option>精华</option><option>已隐藏</option><option>已删除</option><option>正常</option><option>封禁</option><option>已验证</option><option>已处理</option><option>处理中</option><option>待处理</option></select><button type="button" class="btn ghost" data-admin-filter-clear>清除</button>';
      cardBody.insertBefore(toolbar, cardBody.firstElementChild);
      filter = q('[data-admin-filter]', toolbar);
    }
    var apply = function () {
      var term = (filter.value || '').trim().toLowerCase();
      var status = (q('[data-admin-status]', cardBody) || {}).value || '';
      var visible = 0;
      // Generic admin tables do not need batch selection, so their rows do not
      // carry data-selectable-id. Exclude only header and route-level empty rows.
      var dataRows = qa('tbody tr', table).filter(function (row) { return !q('th', row) && !q('.app-empty', row); });
      dataRows.forEach(function (row) {
        var statusText = { enabled: '启用', disabled: '停用', error: '错误' }[status] || status;
        var ok = (!term || row.textContent.toLowerCase().indexOf(term) >= 0) && (!statusText || row.textContent.indexOf(statusText) >= 0);
        row.hidden = !ok;
        if (ok) visible += 1;
      });
      // A route-level empty state is already rendered inside the table when
      // the selected tab has no records. Only show the filter empty state
      // after the user has actually applied a keyword or status filter;
      // otherwise both empty states appear at once.
      var hasActiveFilter = !!term || !!status;
      var emptyRow = q('[data-admin-filter-empty]', cardBody);
      if (hasActiveFilter) {
        if (!emptyRow) { emptyRow = document.createElement('div'); emptyRow.className = 'app-empty'; emptyRow.setAttribute('data-admin-filter-empty', ''); cardBody.appendChild(emptyRow); }
        emptyRow.innerHTML = icon('search', 'ic-16') + '<b>没有匹配结果</b><span>修改关键词或状态筛选后重试。</span>';
        emptyRow.hidden = visible > 0;
      } else if (emptyRow) {
        emptyRow.hidden = true;
      }
      if (typeof window.__adminSelectionSync === 'function') window.__adminSelectionSync();
    };
    var customFilter = q('[data-plugin-search]', cardBody);
    var customStatus = q('[data-plugin-status]', cardBody);
    if (!filter && customFilter) { customFilter.setAttribute('data-admin-filter', ''); filter = customFilter; }
    if (!q('[data-admin-status]', cardBody) && customStatus) customStatus.setAttribute('data-admin-status', '');
    var statusEl = q('[data-admin-status]', cardBody);
    if (!statusEl) { statusEl = document.createElement('select'); statusEl.className = 'app-select admin-unified-filter__status'; statusEl.setAttribute('data-admin-status', ''); statusEl.setAttribute('aria-label', '按状态筛选'); statusEl.innerHTML = '<option value="">全部状态</option><option>待审核</option><option>待审批</option><option>公开</option><option>精华</option><option>已发布</option><option>已隐藏</option><option>已删除</option><option>正常</option><option>启用</option><option>已启用</option><option>已禁用</option><option>已下架</option><option>封禁</option><option>已验证</option><option>运行中</option><option>排队中</option><option>已完成</option><option>失败</option><option>错误</option>'; (filter.closest('.app-toolbar') || cardBody).appendChild(statusEl); }
    if (!filter.__adminFilterBound) { filter.__adminFilterBound = true; filter.addEventListener('input', function () { window.clearTimeout(filter.__filterTimer); filter.__filterTimer = window.setTimeout(apply, 250); }); }
    if (statusEl && !statusEl.__adminFilterBound) { statusEl.__adminFilterBound = true; statusEl.addEventListener('change', apply); }
    var clear = q('[data-admin-filter-clear]', cardBody); if (clear && !clear.__adminFilterBound) { clear.__adminFilterBound = true; clear.addEventListener('click', function () { filter.value = ''; if (statusEl) statusEl.value = ''; apply(); }); }
    var firstId = table.closest('[data-posts-body]') ? 'posts' : table.closest('[data-users-body]') ? 'users' : 'generic';
    var common = { host: host, table: table, actions: firstId === 'posts' ? [{ label: '批量通过', action: 'approve', cls: 'primary' }, { label: '批量隐藏', action: 'hide', cls: 'ghost' }, { label: '批量删除', action: 'delete', cls: 'danger' }] : firstId === 'users' ? [{ label: '批量禁用', action: 'ban', cls: 'danger' }, { label: '批量启用', action: 'enable', cls: 'primary' }] : [{ label: '批量标记', action: 'mark', cls: 'ghost' }], exportHeaders: firstId === 'posts' ? ['标题', '作者', '板块', '状态', '时间'] : firstId === 'users' ? ['用户', '等级', 'B币', '贡献', '状态', '最近活动'] : ['列表项', '当前内容'], exportName: firstId === 'posts' ? 'bblbb-posts-selected.csv' : firstId === 'users' ? 'bblbb-users-selected.csv' : 'bblbb-admin-selected.csv', exportRows: function (id) {
      if (firstId === 'posts') { var p = ADM_POSTS.filter(function (x) { return x.id === id; })[0]; return p ? [p.title, p.author, p.board, postStatusOf(p.id), p.time] : [id, '', '', '', '']; }
      if (firstId === 'users') { var u = ADM_ADMIN_USERS.filter(function (x) { return x.name === id; })[0]; return u ? [u.name, u.level, u.coin, u.contrib, userStatusOf(u.name), u.active] : [id, '', '', '', '', '']; }
      var row = qa('tr[data-selectable-id]', table).filter(function (candidate) { return candidate.getAttribute('data-selectable-id') === id; })[0];
      return [row ? row.textContent.replace(/\s+/g, ' ').trim() : id, '已选中'];
    }, actionMeta: {
      approve: { title: '批量审核通过', label: '确认通过', message: '选中的内容将变为公开状态。' }, hide: { title: '批量隐藏内容', label: '确认隐藏', message: '选中的内容将从前台隐藏。' }, delete: { title: '批量删除内容', label: '确认删除', message: '选中的内容将进入已删除状态。' }, ban: { title: '批量禁用用户', label: '确认禁用', message: '选中的账号将被冻结，历史内容保留。' }, enable: { title: '批量启用用户', label: '确认启用', message: '选中的账号将恢复正常状态。' }, mark: { title: '批量标记', label: '确认标记', message: '确认标记选中的列表项？' }
    }, mutate: function (action, id) {
      if (firstId === 'posts') { var ps = state.postStatus || {}; ps[id] = action === 'approve' ? '公开' : action === 'delete' ? '已删除' : '已隐藏'; state.postStatus = ps; }
      else if (firstId === 'users') { var us = state.userStatus || {}; us[id] = action === 'ban' ? 'banned' : 'enabled'; state.userStatus = us; }
      else { var bm = state.bulkMarks || {}; bm[id] = action; state.bulkMarks = bm; }
    }, after: function (action, count) { saveState(); logAudit('批量操作 · ' + action, count + ' 项'); if (firstId === 'posts') renderPostsAdmin(); else if (firstId === 'users') renderUsersAdmin(); else { setupAdminEnhancements(); toast('批量操作已完成 · 已写入审计'); } } };
    bindAdminSelection(common);
    window.__adminSelectionRefresh = function () { bindAdminSelection(common); };
    if (window.MutationObserver && table.tBodies[0]) {
      var selectionObserver = new MutationObserver(function () {
        if (!q('[data-batch-select]', table)) bindAdminSelection(common);
      });
      selectionObserver.observe(table.tBodies[0], { childList: true });
    }
    apply();
    if (!adminEnhancementsBound) { adminEnhancementsBound = true; }
  }

  function openDrawer(title, body, foot) {
    closeDrawer();
    var backdrop = document.createElement('div');
    backdrop.className = 'app-drawer-backdrop';
    backdrop.innerHTML = '<aside class="app-drawer" role="dialog" aria-modal="true" aria-labelledby="app-drawer-title"><header class="app-drawer__head"><h2 id="app-drawer-title">' + esc(title) + '</h2><button type="button" class="app-modal__close" aria-label="关闭" data-drawer-close>' + icon('x') + '</button></header><div class="app-drawer__body">' + body + '</div><footer class="app-drawer__foot">' + (foot || button('关闭', 'ghost', 'data-drawer-close')) + '</footer></aside>';
    document.body.appendChild(backdrop);
    document.body.classList.add('app-drawer-open');
    qa('[data-drawer-close]', backdrop).forEach(function (b) { b.addEventListener('click', closeDrawer); });
    backdrop.addEventListener('click', function (e) { if (e.target === backdrop) closeDrawer(); });
    return backdrop;
  }
  function closeDrawer() { var d = q('.app-drawer-backdrop'); if (d) d.remove(); document.body.classList.remove('app-drawer-open'); }

  function rejectPostModal(post, onConfirm) {
    var reasons = ['证据不足', '内容不符合板块规范', '包含站外商业推广', '需要补充来源'];
    openModal('驳回帖子', '<p>请选择驳回理由，作者将收到通知。</p><div class="app-chip-row" data-reject-reasons>' + reasons.map(function (r) { return button(r, 'ghost sm', 'data-reject-reason="' + esc(r) + '"'); }).join('') + '</div><label class="app-field-label" style="margin-top:14px">处理原因 <span class="app-required">*</span></label><textarea class="app-textarea" aria-label="驳回原因" data-reject-text placeholder="必填，写入审计日志"></textarea>', '取消 ' + button('确认驳回', 'danger', 'data-reject-confirm'));
    qa('[data-reject-reason]').forEach(function (b) { b.addEventListener('click', function () { q('[data-reject-text]').value = b.getAttribute('data-reject-reason'); qa('[data-reject-reason]').forEach(function (x) { x.classList.remove('is-active'); }); b.classList.add('is-active'); }); });
    q('[data-reject-confirm]').addEventListener('click', function () { var reason = q('[data-reject-text]').value.trim(); if (!reason) { q('[data-reject-text]').classList.add('is-error'); toast('请填写驳回原因'); return; } closeModal(); onConfirm(reason); });
  }

  function showPostDiff(post) {
    var original = '这个月用 SvelteKit 写完 BBLBB 的核心页面，包括 SSR、SEO、CSRF 与表单接入。';
    var revised = '这个月用 SvelteKit 写完 BBLBB 的核心页面，包括 SSR、SEO、CSRF、表单与 OIDC 接入，整体体验比想象中好。';
    openModal('版本 Diff · ' + post.id, '<div class="app-diff"><div class="app-diff__legend"><span class="is-added">新增</span><span class="is-removed">删除</span><span class="is-changed">变更</span></div><div class="app-diff__grid"><section><h3>修改前</h3><p class="app-diff__text app-diff__text--removed">' + esc(original) + '</p></section><section><h3>修改后</h3><p class="app-diff__text app-diff__text--added">' + esc(revised) + '</p></section></div><p class="app-muted">Mock 版本对比 · 字段变更将在服务端审核记录中保留。</p></div>', '关闭');
  }

  /* ---- 各页基础数据（Mock 投影，变更写入 state 覆盖层） ---- */
  var ADM_DOWNLOADS = [
    { id: 'DL-8823', file: 'report.png', user: 'Yuwen', size: '1.2 MB', status: '已完成' },
    { id: 'DL-8822', file: 'architecture.pdf', user: 'Alice', size: '2.4 MB', status: '已授权' },
    { id: 'DL-8821', file: 'demo.zip', user: 'Mark', size: '8.1 MB', status: '已完成' },
    { id: 'DL-8820', file: 'cover.webp', user: 'Nina', size: '0.6 MB', status: '失败' }
  ];
  var ADM_AI_TASKS = [
    { id: 'T-311', name: '草稿格式修复', source: '草稿 #d-12', status: '运行中' },
    { id: 'T-310', name: '内容摘要', source: '主题 T-201', status: '排队中' },
    { id: 'T-309', name: '敏感词复核', source: '主题 T-198', status: '已完成' },
    { id: 'T-308', name: '标题翻译', source: '草稿 #d-09', status: '失败' }
  ];
  var ADM_VIDEO_JOBS = [
    { id: 'V-52', src: 'sveltekit-demo 嵌入', size: '42 MB', status: '转码中' },
    { id: 'V-51', src: 'sqlite-talk 视频', size: '88 MB', status: '排队中' },
    { id: 'V-50', src: 'rust-bench 视频', size: '61 MB', status: '已完成' },
    { id: 'V-49', src: 'cdn-wasm 嵌入', size: '12 MB', status: '失败' }
  ];
  var ADM_APPS = [
    { id: 'theme-store', name: '主题商店', kind: '应用', status: '已启用', orders: 42, version: '2.4.1', endpoint: 'https://apps.bblbb.local/theme-store' },
    { id: 'avatar-gen', name: '头像生成', kind: '工具', status: '已启用', orders: 128, version: '1.8.0', endpoint: 'https://apps.bblbb.local/avatar-gen' },
    { id: 'resume-tpl', name: '简历模板', kind: '内容', status: '待审批', orders: 0, version: '0.9.3', endpoint: 'https://apps.bblbb.local/resume-tpl' },
    { id: 'export-helper', name: '导出助手', kind: '工具', status: '已禁用', orders: 77, version: '1.1.2', endpoint: 'https://apps.bblbb.local/export-helper' }
  ];
  var ADM_BI = {
    '今日': [['活跃成员', 421, 1284, '+2.6%'], ['新增内容', 56, 200, '+8.1%'], ['积分流水', 1830, 5000, '+3.4%'], ['审核通过率', 88, 100, '+1.2%']],
    '本周': [['活跃成员', 964, 1284, '+1.8%'], ['新增内容', 342, 600, '+5.2%'], ['积分流水', 11240, 20000, '-0.6%'], ['审核通过率', 86, 100, '-0.4%']],
    '本月': [['活跃成员', 1052, 1284, '+0.9%'], ['新增内容', 1260, 2000, '+4.4%'], ['积分流水', 48210, 60000, '+6.2%'], ['审核通过率', 87, 100, '+0.3%']],
    '今年': [['活跃成员', 1284, 1284, '+12.4%'], ['新增内容', 14320, 18000, '+18.8%'], ['积分流水', 486220, 600000, '+21.6%'], ['审核通过率', 89, 100, '+2.1%']]
  };
  var ADM_ADMIN_USERS = [
    { name: 'Chaos', level: 'LV.3', replyCount: 68, postCount: 14, likesGiven: 42, likesReceived: 31, loginDays: 64, coin: 328, contrib: 146, status: '正常', active: '今天', title: 'Rust / 自托管爱好者', bio: '写 Rust 和自托管，维护这个论坛。' },
    { name: 'Yuwen', level: 'LV.4', replyCount: 146, postCount: 38, likesGiven: 118, likesReceived: 96, loginDays: 312, coin: 1200, contrib: 312, status: '已验证', active: '今天', title: '站长与产品维护者', bio: '把社区做成一个可以长期维护的地方。' },
    { name: 'Mark', level: 'LV.2', replyCount: 18, postCount: 4, likesGiven: 9, likesReceived: 7, loginDays: 21, coin: 124, contrib: 58, status: '正常', active: '昨天', title: '全栈开发者', bio: '记录工程实践与产品反馈。' },
    { name: 'Alice', level: 'LV.4', replyCount: 112, postCount: 27, likesGiven: 54, likesReceived: 44, loginDays: 126, coin: 14, contrib: 22, status: '已验证', active: '3 天前', title: '后端与数据库调优', bio: '喜欢把复杂系统拆成可验证的小步。' },
    { name: 'Nina', level: 'LV.3', replyCount: 54, postCount: 11, likesGiven: 26, likesReceived: 18, loginDays: 48, coin: 412, contrib: 88, status: '正常', active: '今天', title: '性能与可观测性', bio: '专注服务端性能优化与压测。' },
    { name: 'spam_bot_42', level: 'LV.1', replyCount: 4, postCount: 1, likesGiven: 1, likesReceived: 1, loginDays: 3, coin: 0, contrib: 0, status: '封禁', active: '30 天前', title: '—', bio: '因垃圾广告被禁言。' },
    { name: 'Lin', level: 'LV.3', replyCount: 46, postCount: 9, likesGiven: 21, likesReceived: 14, loginDays: 39, coin: 260, contrib: 96, status: '正常', active: '昨天', title: '前端工程化', bio: '喜欢把重复的事情变成工具。' },
    { name: 'Ken', level: 'LV.3', replyCount: 38, postCount: 10, likesGiven: 19, likesReceived: 12, loginDays: 35, coin: 190, contrib: 74, status: '正常', active: '2 天前', title: 'Rust 系统编程', bio: '从 Python 迁移到 Rust 的半个当事人。' },
    { name: 'Sara', level: 'LV.2', replyCount: 12, postCount: 3, likesGiven: 6, likesReceived: 4, loginDays: 14, coin: 76, contrib: 31, status: '正常', active: '4 天前', title: '设计与可用性', bio: '小团队里负责「看起来靠谱」的那个人。' },
    { name: 'Zoe', level: 'LV.2', replyCount: 10, postCount: 3, likesGiven: 5, likesReceived: 3, loginDays: 8, coin: 54, contrib: 27, status: '正常', active: '5 天前', title: '产品与设计', bio: '删代码比写代码多。' }
  ];
  var ADM_ROLES = [
    { id: 'owner', name: '站长', type: '系统角色', scope: '全站', members: 1, desc: '拥有平台所有权和紧急处置能力。', perms: ['admin.manage', 'admin.roles.manage', 'admin.settings', 'audit.read', 'audit.export', 'user.read', 'user.manage', 'user.ban', 'report.review', 'report.resolve', 'board.manage', 'board.moderate', 'post.manage', 'post.publish', 'post.pin', 'tag.manage', 'attachment.manage', 'content.feature', 'points.adjust', 'points.grant', 'market.manage', 'billing.refund', 'theme.manage', 'plugin.manage', 'notification.send', 'analytics.read', 'api.manage'] },
    { id: 'admin', name: '管理员', type: '系统角色', scope: '全站', members: 2, desc: '管理内容、成员和平台日常配置。', perms: ['admin.manage', 'admin.roles.manage', 'admin.settings', 'audit.read', 'user.read', 'user.manage', 'user.ban', 'report.review', 'report.resolve', 'board.manage', 'board.moderate', 'post.manage', 'post.publish', 'post.pin', 'tag.manage', 'attachment.manage', 'content.feature', 'points.adjust', 'points.grant', 'market.manage', 'theme.manage', 'notification.send', 'analytics.read'] },
    { id: 'content-editor', name: '内容编辑', type: '内容角色', scope: '全站内容', members: 3, desc: '负责内容编辑、发布、置顶和精选。', perms: ['user.read', 'post.manage', 'post.publish', 'post.pin', 'tag.manage', 'attachment.manage', 'content.feature'] },
    { id: 'mod', name: '版主', type: '审核角色', scope: '所在板块', members: 8, desc: '维护所负责板块的秩序和内容质量。', perms: ['user.read', 'report.review', 'report.resolve', 'board.moderate', 'post.manage', 'post.pin', 'content.hide'] },
    { id: 'community', name: '社区运营', type: '运营角色', scope: '全站社区', members: 2, desc: '负责社区活动、公告、精选和运营数据。', perms: ['user.read', 'board.manage', 'board.moderate', 'post.manage', 'post.publish', 'content.feature', 'tag.manage', 'notification.send', 'analytics.read'] },
    { id: 'finance', name: '财务管理员', type: '业务角色', scope: '积分与交易', members: 1, desc: '处理积分、B 币、订单和退款。', perms: ['user.read', 'points.adjust', 'points.grant', 'market.manage', 'billing.refund', 'analytics.read'] },
    { id: 'theme', name: '主题管理员', type: '扩展角色', scope: '主题与插件', members: 1, desc: '管理主题、插件和扩展集成。', perms: ['theme.manage', 'plugin.manage', 'api.manage'] },
    { id: 'creator', name: '创作者', type: '用户角色', scope: '个人内容', members: 86, desc: '可以发布内容并管理自己的创作。', perms: ['content.create', 'content.publish', 'content.edit.own', 'content.comment', 'content.favorite', 'content.report', 'attachment.manage'] },
    { id: 'member', name: '成员', type: '用户角色', scope: '个人', members: 1284, desc: '完成注册验证后的基础社区成员。', perms: ['content.create', 'content.comment', 'content.favorite', 'content.report'] },
    { id: 'guest', name: '访客', type: '用户角色', scope: '公开内容', members: 0, desc: '未登录用户，只能浏览公开内容。', perms: ['public.read'] }
  ];
  var ROLE_PERMISSION_META = {
    'admin.manage': ['系统与权限', '管理站点配置、管理员和角色策略'],
    'admin.roles.manage': ['系统与权限', '创建角色并配置角色授权策略'],
    'admin.settings': ['系统与权限', '修改站点、安全和全局运行设置'],
    'audit.read': ['系统与权限', '查看不可变审计记录和操作详情'],
    'audit.export': ['系统与权限', '导出审计记录和合规报告'],
    'api.manage': ['系统与权限', '管理 API Key、OAuth 客户端和 Webhook'],
    'user.read': ['成员与安全', '查看成员资料、状态和登录信息'],
    'user.manage': ['成员与安全', '编辑成员资料、角色和验证状态'],
    'user.ban': ['成员与安全', '禁用、解封和冻结成员账号'],
    'report.review': ['成员与安全', '查看待处理举报和风险线索'],
    'report.resolve': ['成员与安全', '处理举报、禁言和申诉结果'],
    'points.adjust': ['成员与安全', '调整成员积分与 B 币余额'],
    'points.grant': ['成员与安全', '发放活动奖励和补偿积分'],
    'board.manage': ['内容与社区', '创建、编辑和配置社区板块'],
    'board.moderate': ['内容与社区', '处理所在板块的举报与内容'],
    'post.manage': ['内容与社区', '编辑、隐藏和审核帖子内容'],
    'post.publish': ['内容与社区', '代表社区发布公告和内容'],
    'post.pin': ['内容与社区', '置顶、加精或推荐社区内容'],
    'content.hide': ['内容与社区', '隐藏违反规范的帖子和评论'],
    'content.feature': ['内容与社区', '将优质内容加入精选和推荐位'],
    'content.create': ['内容与社区', '发布内容'],
    'content.publish': ['内容与社区', '公开自己的草稿和创作'],
    'content.edit.own': ['内容与社区', '编辑和管理自己发布的内容'],
    'content.comment': ['内容与社区', '回复帖子和参与讨论'],
    'content.favorite': ['内容与社区', '收藏内容'],
    'content.report': ['内容与社区', '举报不当内容并补充理由'],
    'tag.manage': ['内容与社区', '创建、合并和维护内容标签'],
    'attachment.manage': ['内容与社区', '上传、替换和删除内容附件'],
    'market.manage': ['激励与交易', '管理商品、订单和交易状态'],
    'billing.refund': ['激励与交易', '处理订单退款和交易争议'],
    'theme.manage': ['扩展与集成', '发布、切换和配置站点主题'],
    'plugin.manage': ['扩展与集成', '安装、配置和停用插件'],
    'notification.send': ['扩展与集成', '发送站内信、公告和运营通知'],
    'analytics.read': ['数据与审计', '查看运营、内容和成员分析数据'],
    'public.read': ['内容与社区', '阅读公开内容和板块']
  };
  var ROLE_PERMISSION_KEYS = Object.keys(ROLE_PERMISSION_META);
  var ADM_POSTS = [
    { id: 'p-201', title: '使用 SvelteKit 构建博客与轻量论坛是否合理？', author: 'Chaos', board: 'Web 开发', status: '待审核', time: '今天 09:12' },
    { id: 'p-101', title: 'Rust 小机器上的 SQLite 并发实践', author: 'Chaos', board: '技术随笔', status: '精华', time: '昨天 16:40' },
    { id: 'p-202', title: 'OAuth Provider 应该自研到什么程度？', author: 'Alice', board: '开源项目', status: '公开', time: '2 天前' },
    { id: 'p-103', title: 'Markdown 渲染沙箱与 CSP 实战', author: 'Mark', board: 'Web 开发', status: '公开', time: '3 天前' },
    { id: 'p-210', title: '【推广】高效能程序员社群，扫码进群', author: 'spam_bot_42', board: '闲聊', status: '已隐藏', time: '3 天前' }
  ];
  var ADM_TAGS = [
    { name: 'Rust', uses: 248 }, { name: 'SvelteKit', uses: 182 }, { name: '自托管', uses: 94 },
    { name: '性能优化', uses: 86 }, { name: 'SQLite', uses: 61 }, { name: 'Markdown', uses: 33 }
  ];
  var ADM_FILES = [
    { id: 'f-11', name: 'report.png', user: 'Yuwen', size: '1.2 MB', scope: '公开', scan: '已通过' },
    { id: 'f-12', name: 'architecture.pdf', user: 'Alice', size: '2.4 MB', scope: '私有', scan: '已通过' },
    { id: 'f-13', name: 'demo.zip', user: 'Mark', size: '8.1 MB', scope: '私有', scan: '待扫描' },
    { id: 'f-14', name: 'cover.webp', user: 'Nina', size: '0.6 MB', scope: '公开', scan: '已通过' },
    { id: 'f-15', name: 'notes.md', user: 'Chaos', size: '12 KB', scope: '私有', scan: '扫描中' }
  ];
  var ADM_TEMPLATES = [
    { id: 'tpl-verify', name: '验证邮件', mode: '事件触发', queue: 1, sample: '你的 BBLBB 验证链接 24 小时内有效，点击完成邮箱验证。' },
    { id: 'tpl-digest', name: '通知摘要', mode: '每日 08:00', queue: 0, sample: '昨晚你的主题收到 3 条新回复、2 位新关注者，查看通知中心。' },
    { id: 'tpl-security', name: '安全提醒', mode: '事件触发', queue: 0, sample: '你的账号在新设备登录（Linux · 当前网络）。如非本人操作请立即修改密码。' },
    { id: 'tpl-welcome', name: '欢迎邮件', mode: '注册触发', queue: 0, sample: '欢迎加入 BBLBB 社区，完成验证后即可发帖参与讨论。' }
  ];
  var ADM_AUDIT_SEED = [
    { actor: 'Chaos', text: '积分调整 · +50 B币', object: '用户 Mark' },
    { actor: 'Chaos', text: '举报处理 · 禁言 7 天', object: 'R-1021' },
    { actor: 'Chaos', text: '主题切换 · 暗色主题', object: '站点' },
    { actor: '系统', text: '存储连接测试', object: 'request_9f2a' }
  ];

  /* ---- 各页状态读取 ---- */
  function admBoardRows() {
    var extra = state.boardNew || [];
    return BOARDS.concat(extra.map(function (b) { return { slug: b.slug, name: b.name, desc: b.desc, count: '0 个主题', today: '' }; }));
  }
  function boardModOf(slug) { return (state.boardMod || {})[slug] || {}; }
  function userStatusOf(name) {
    var m = state.userStatus || {};
    if (m[name]) return m[name] === 'banned' ? '封禁' : '正常';
    var base = ADM_ADMIN_USERS.filter(function (u) { return u.name === name; })[0];
    return base ? base.status : '正常';
  }
  function rolePermsOf(roleId) {
    var role = allAdminRoles().filter(function (r) { return r.id === roleId; })[0];
    var saved = (state.rolePerms || {})[roleId] || {};
    var out = {};
    if (!role) return out;
    ROLE_PERMISSION_KEYS.forEach(function (p) { out[p] = saved[p] == null ? role.perms.indexOf(p) >= 0 : !!saved[p]; });
    return out;
  }
  function allAdminRoles() {
    var custom = Array.isArray(state.customRoles) ? state.customRoles : [];
    var meta = state.roleMeta || {};
    return ADM_ROLES.map(function (role) { return Object.assign({}, role, meta[role.id] || {}); }).concat(custom);
  }
  function userRoleOf(name) {
    var explicit = state.userRoles || {};
    var seeded = { Chaos: 'admin', Yuwen: 'owner', Mark: 'creator', Alice: 'content-editor', Nina: 'mod', spam_bot_42: 'member' };
    return explicit[name] || seeded[name] || 'member';
  }
  function roleAssignedCount(roleId) {
    return ADM_ADMIN_USERS.filter(function (user) { return userRoleOf(user.name) === roleId; }).length;
  }
  function postStatusOf(id) {
    var m = state.postStatus || {};
    return m[id] || (ADM_POSTS.filter(function (p) { return p.id === id; })[0] || {}).status || '公开';
  }
  function tagStateOf(name) {
    return { status: (state.tagStatus || {})[name] || 'normal', mergedInto: (state.tagMergedInto || {})[name] || '' };
  }

  /* ---- 下载计费 ---- */
  function renderDownloadBilling() {
    var cfg = state.billingConfig || { on: true, price: 10, ttl: '24 小时', dailyLimit: 20 };
    var qmap = state.downloadRecords || state.downloadQueue || {};
    var rows = ADM_DOWNLOADS.map(function (d) {
      var st = qmap[d.id] || d.status;
      // 兼容旧版原型曾写入的队列状态，计费页统一按下载记录展示。
      if (st === '排队中' || st === '生成中') st = '已完成';
      var stCls = st === '已完成' || st === '已授权' ? 'sb-success' : st === '失败' ? 'sb-danger' : 'sb-gray';
      var acts = '';
      if (st === '失败') acts += button('重试', 'ghost sm', 'data-adm-act="dl-retry" data-adm-id="' + d.id + '"');
      acts += button('详情', 'ghost sm', 'data-adm-act="dl-detail" data-adm-id="' + d.id + '"');
      return '<tr><td><code>' + d.id + '</code></td><td>' + d.file + '</td><td>' + d.user + '</td><td class="num">' + d.size + '</td><td><span class="sbadge ' + stCls + '">' + st + '</span></td><td class="adm-acts">' + acts + '</td></tr>';
    }).join('');
    
    mountTpl('admin', 'admin-download-billing', function () {
      fillAdminTbody('[data-dl-table]', rows);
      var cards = qa('.app-admin-main > .app-card');
      if (cards.length > 1) {
        cards[1].parentNode.insertBefore(cards[1], cards[0]);
      }
      var recordCard = q('[data-dl-table]') ? q('[data-dl-table]').closest('.app-card') : null;
      var heading = recordCard ? q('h2', recordCard) : null;
      if (heading && heading.textContent === '下载队列') heading.textContent = '下载记录';
      var ths = qa('[data-dl-table] th');
      if (ths[4]) ths[4].textContent = '结果';
      var refresh = q('[data-dl-refresh]');
      if (refresh) refresh.textContent = '刷新记录';
      var dlOn = q('[data-dl-on]'); if (dlOn) dlOn.checked = cfg.on;
      var dlPrice = q('[data-dl-price]'); if (dlPrice) dlPrice.value = cfg.price;
      var dlTtl = q('[data-dl-ttl]'); if (dlTtl) dlTtl.value = cfg.ttl;
      var dlLimit = q('[data-dl-limit]'); if (dlLimit) dlLimit.value = cfg.dailyLimit;

    q('[data-dl-export]').addEventListener('click', function () {
      var qm = state.downloadRecords || state.downloadQueue || {};
      if (adminCsv('bblbb-download-billing.csv', ['单号', '文件', '用户', '大小', '结果'], ADM_DOWNLOADS.map(function (d) { var st = qm[d.id] || d.status; return [d.id, d.file, d.user, d.size, (st === '排队中' || st === '生成中') ? '已完成' : st]; }))) { logAudit('导出下载账单', ADM_DOWNLOADS.length + ' 条'); toast('账单已导出 · ' + ADM_DOWNLOADS.length + ' 条'); }
    });
    q('[data-dl-refresh]').addEventListener('click', function () {
      actionPending(this, '刷新中…', function () {
        logAudit('刷新下载记录', ADM_DOWNLOADS.length + ' 条'); renderDownloadBilling(); toast('下载记录已刷新');
      });
    });
    q('[data-dl-save]').addEventListener('click', function () {
      state.billingConfig = { on: q('[data-dl-on]').checked, price: Math.max(0, Number(q('[data-dl-price]').value) || 0), ttl: q('[data-dl-ttl]').value, dailyLimit: Math.max(0, Number(q('[data-dl-limit]').value) || 0) };
      saveState(); logAudit('更新下载计费策略', '价格 ' + state.billingConfig.price + ' 积分 · 有效期 ' + state.billingConfig.ttl); toast('计费策略已保存 · 已写入审计');
    });
    bindAdminActs(function (el, act, id) {
      var d = ADM_DOWNLOADS.filter(function (x) { return x.id === id; })[0]; if (!d) return;
      if (act === 'dl-detail') { var detailStatus = (state.downloadRecords || state.downloadQueue || {})[id] || d.status; if (detailStatus === '排队中' || detailStatus === '生成中') detailStatus = '已完成'; openModal('下载 ' + id, '<p><b>' + d.file + '</b> · ' + d.size + '</p><p class="app-muted">用户 ' + d.user + ' · 下载结果 ' + detailStatus + ' · 授权有效期 ' + (state.billingConfig || { ttl: '24 小时' }).ttl + '</p>', '关闭'); return; }
      if (act === 'dl-retry') { actionPending(el, '重试中…', function () { var qm = state.downloadRecords || state.downloadQueue || {}; qm[id] = '已完成'; state.downloadRecords = qm; delete state.downloadQueue; saveState(); logAudit('重试下载 ' + id, d.file); renderDownloadBilling(); toast(id + ' 下载已完成'); }); return; }
    });
    });
}

  /* ---- 大模型设置 ---- */
  function renderAiAdmin() {
    var cfg = state.aiConfig || { provider: 'gateway', model: 'bblbb-format-v1', privacy: 'masked', budget: 5000, rpd: 200 };
    var defaultChannels = [
      { id: 'gateway', name: '受控 Gateway', kind: '内部网关', endpoint: 'https://gateway.bblbb.local/v1', models: ['bblbb-format-v1', 'bblbb-summarize-v1', 'bblbb-moderation-v2'], status: '已连接', mark: 'G' },
      { id: 'openai', name: 'OpenAI API', kind: 'OpenAI 兼容', endpoint: 'https://api.openai.com/v1', models: ['gpt-4o-mini', 'gpt-4.1-mini', 'gpt-4o'], status: '已连接', mark: 'O' },
      { id: 'local', name: '本地 Ollama', kind: 'Ollama', endpoint: 'http://127.0.0.1:11434', models: ['qwen2.5:7b', 'llama3.2'], status: '已连接', mark: 'L' }
    ];
    var channels = Array.isArray(state.aiChannels) && state.aiChannels.length ? state.aiChannels : defaultChannels;
    var scenarios = [
      { id: 'format', name: '草稿格式修复', desc: '整理 Markdown 结构与排版', channel: 'gateway', model: 'bblbb-format-v1' },
      { id: 'summary', name: '内容摘要', desc: '生成主题摘要与通知预览', channel: 'openai', model: 'gpt-4o-mini' },
      { id: 'moderation', name: '敏感词复核', desc: '发布前的内容安全检查', channel: 'gateway', model: 'bblbb-moderation-v2' },
      { id: 'translation', name: '标题翻译', desc: '将标题翻译为站点默认语言', channel: 'local', model: 'qwen2.5:7b' },
      { id: 'assistant', name: '社区助手', desc: '前台问答与知识检索', channel: 'gateway', model: 'bblbb-format-v1' }
    ];
    var savedRoutes = state.aiRoutes || {};
    scenarios = scenarios.map(function (item) { return Object.assign({}, item, savedRoutes[item.id] || {}); });
    var channelById = function (id) { return channels.filter(function (x) { return x.id === id; })[0] || channels[0]; };
    function channelLabel(c) { return c.name + ' · ' + c.kind; }
    function channelOptions(selected) { return channels.map(function (c) { return '<option value="' + esc(c.id) + '"' + (c.id === selected ? ' selected' : '') + '>' + esc(channelLabel(c)) + '</option>'; }).join(''); }
    function modelOptions(channelId, selected) { var c = channelById(channelId); var list = Array.isArray(c.models) ? c.models : []; return list.length ? list.map(function (m) { return '<option value="' + esc(m) + '"' + (m === selected ? ' selected' : '') + '>' + esc(m) + '</option>'; }).join('') : '<option value="">请先获取模型</option>'; }
    function renderChannels() {
      var host = q('[data-ai-channels]'); if (!host) return;
      host.innerHTML = channels.map(function (c) { var isDefault = c.id === cfg.provider; return '<article class="ai-channel ' + (isDefault ? 'is-default' : '') + '" data-ai-channel="' + esc(c.id) + '"><div class="ai-channel__head"><span class="ai-channel__mark">' + esc(c.mark || c.name.charAt(0)) + '</span><div class="ai-channel__title"><b>' + esc(c.name) + '</b><small title="' + esc(c.endpoint) + '">' + esc(c.endpoint) + '</small></div><span class="sbadge sb-success">' + esc(c.status || '未测试') + '</span></div><div class="ai-channel__models">' + ((c.models || []).map(function (m) { var used = scenarios.some(function (s) { return s.channel === c.id && s.model === m; }); return '<span class="ai-model-chip ' + (used ? 'is-selected' : '') + '">' + esc(m) + '</span>'; }).join('') || '<span class="app-muted">尚未同步模型</span>') + '</div><div class="ai-channel__actions">' + button('获取模型', 'secondary sm', 'data-ai-fetch="' + esc(c.id) + '"') + button('编辑', 'ghost sm', 'data-ai-edit="' + esc(c.id) + '"') + (isDefault ? '<span class="app-field-help" style="margin-left:auto">默认渠道</span>' : '') + '</div></article>'; }).join('');
      var count = q('[data-ai-channel-count]'); if (count) count.textContent = channels.length + ' 个渠道';
      var statC = q('[data-ai-stat-channels]'); if (statC) statC.textContent = channels.length;
      var statM = q('[data-ai-stat-models]'); if (statM) statM.textContent = channels.reduce(function (n, c) { return n + (Array.isArray(c.models) ? c.models.length : 0); }, 0);
      qa('[data-ai-fetch]').forEach(function (el) { el.addEventListener('click', function () { fetchAiModels(el.getAttribute('data-ai-fetch'), el); }); });
      qa('[data-ai-edit]').forEach(function (el) { el.addEventListener('click', function () { editAiChannel(el.getAttribute('data-ai-edit')); }); });
    }
    function renderRouting() {
      var host = q('[data-ai-routing]'); if (!host) return;
      host.innerHTML = scenarios.map(function (s) { var c = channelById(s.channel); var selectedModel = (c.models || []).indexOf(s.model) >= 0 ? s.model : (c.models || [])[0] || ''; return '<div class="ai-route-row" data-ai-route="' + esc(s.id) + '"><div class="ai-route-row__name"><b>' + esc(s.name) + '</b><span>' + esc(s.desc) + '</span></div><label class="app-form-field"><span class="app-field-label">渠道</span><select class="app-select" data-ai-route-channel="' + esc(s.id) + '">' + channelOptions(s.channel) + '</select></label><label class="app-form-field"><span class="app-field-label">模型</span><select class="app-select" data-ai-route-model="' + esc(s.id) + '">' + modelOptions(s.channel, selectedModel) + '</select></label></div>'; }).join('');
      qa('[data-ai-route-channel]').forEach(function (el) { el.addEventListener('change', function () { var id = el.getAttribute('data-ai-route-channel'); var target = q('[data-ai-route-model="' + id + '"]'); if (target) target.innerHTML = modelOptions(el.value, ''); updateAiCurrentModel(); }); });
      qa('[data-ai-route-model]').forEach(function (el) { el.addEventListener('change', updateAiCurrentModel); });
      updateAiCurrentModel();
    }
    function updateAiCurrentModel() { var first = q('[data-ai-route-model="assistant"]'); var row = q('[data-ai-route-channel="assistant"]'); var out = q('[data-ai-current-model]'); if (out && first && row) out.textContent = (channelById(row.value).name || '') + ' / ' + (first.value || '未选择'); }
    function readRoutes() { var out = {}; scenarios.forEach(function (s) { var ch = q('[data-ai-route-channel="' + s.id + '"]'); var mo = q('[data-ai-route-model="' + s.id + '"]'); out[s.id] = { channel: ch ? ch.value : s.channel, model: mo ? mo.value : s.model }; }); return out; }
    function fetchAiModels(id, el) {
      var c = channelById(id); if (!c) return;
      actionPending(el, '获取中…', function () {
        var url = c.kind === 'Ollama' ? c.endpoint.replace(/\/$/, '') + '/api/tags' : c.endpoint.replace(/\/$/, '') + (c.endpoint.indexOf('/models') >= 0 ? '' : '/models');
        fetch(url, { method: 'GET', headers: c.apiKey ? { Authorization: 'Bearer ' + c.apiKey } : {} }).then(function (res) { if (!res.ok) throw new Error('HTTP ' + res.status); return res.json(); }).then(function (data) {
          var list = Array.isArray(data) ? data : (Array.isArray(data.data) ? data.data : (Array.isArray(data.models) ? data.models : []));
          list = list.map(function (x) { return typeof x === 'string' ? x : (x.id || x.name || x.model); }).filter(Boolean);
          if (!list.length) throw new Error('接口没有返回模型');
          c.models = list; c.status = '已连接'; state.aiChannels = channels; saveState(); logAudit('同步大模型渠道', c.name + ' · ' + list.length + ' 个模型'); renderAiAdmin(); toast(c.name + ' 已同步 ' + list.length + ' 个模型');
        }).catch(function (err) { c.status = '连接失败'; state.aiChannels = channels; saveState(); renderChannels(); toast('获取失败：' + (err.message || '渠道接口不可用')); });
      });
    }
    function editAiChannel(id) { var c = channelById(id); if (!c) return; var body = '<div class="adm-form-grid"><div class="app-form-field"><label class="app-field-label" for="ai-channel-name">渠道名称</label><input id="ai-channel-name" class="app-field" data-ai-channel-field="name" value="' + esc(c.name) + '"></div><div class="app-form-field"><label class="app-field-label" for="ai-channel-kind">接口类型</label><select id="ai-channel-kind" class="app-select" data-ai-channel-field="kind"><option ' + (c.kind === 'OpenAI 兼容' ? 'selected' : '') + '>OpenAI 兼容</option><option ' + (c.kind === 'Ollama' ? 'selected' : '') + '>Ollama</option><option ' + (c.kind === '内部网关' ? 'selected' : '') + '>内部网关</option></select></div><div class="app-form-field app-form-field--full"><label class="app-field-label" for="ai-channel-endpoint">Base URL</label><input id="ai-channel-endpoint" class="app-field" data-ai-channel-field="endpoint" value="' + esc(c.endpoint) + '" placeholder="https://api.example.com/v1"></div><div class="app-form-field app-form-field--full"><label class="app-field-label" for="ai-channel-key">API Key <span class="app-muted">保存后仅显示末四位</span></label><input id="ai-channel-key" class="app-field" type="password" data-ai-channel-field="apiKey" value="' + esc(c.apiKey || '') + '" placeholder="留空表示不修改"></div></div><p class="app-field-help">保存后可点击“获取模型”请求该渠道的模型列表。密钥仅用于服务端请求，不会展示给前台。</p>'; openModal((id ? '编辑渠道' : '添加渠道'), body, '取消 ' + button('保存渠道', 'primary', 'data-ai-channel-save')); q('[data-ai-channel-save]').addEventListener('click', function () { var name = q('[data-ai-channel-field="name"]').value.trim(); var endpoint = q('[data-ai-channel-field="endpoint"]').value.trim().replace(/\/$/, ''); if (!name || !/^https?:\/\//i.test(endpoint)) { toast('请填写名称和有效的 http(s) 地址'); return; } c.name = name; c.kind = q('[data-ai-channel-field="kind"]').value; c.endpoint = endpoint; var key = q('[data-ai-channel-field="apiKey"]').value.trim(); if (key) c.apiKey = key; state.aiChannels = channels; saveState(); logAudit(id ? '更新大模型渠道' : '新增大模型渠道', c.name); closeModal(); renderAiAdmin(); toast('渠道已保存 · 请获取模型'); }); }
    var tmap = state.aiTasks || {};
    var rows = ADM_AI_TASKS.map(function (t) {
      var st = tmap[t.id] || t.status;
      var stCls = st === '已完成' ? 'sb-success' : st === '失败' ? 'sb-danger' : st === '已中止' ? 'sb-gray' : st === '运行中' ? 'sb-brand' : 'sb-hot';
      var acts = '';
      if (st === '失败' || st === '已中止' || st === '排队中') acts += button('重试', 'ghost sm', 'data-adm-act="ai-retry" data-adm-id="' + t.id + '"');
      if (st === '运行中') acts += button('中止', 'ghost sm', 'data-adm-act="ai-cancel" data-adm-id="' + t.id + '"');
      acts += button('详情', 'ghost sm', 'data-adm-act="ai-detail" data-adm-id="' + t.id + '"');
      return '<tr><td><code>' + t.id + '</code></td><td>' + t.name + '</td><td>' + t.source + '</td><td><span class="sbadge ' + stCls + '">' + st + '</span></td><td class="adm-acts">' + acts + '</td></tr>';
    }).join('');
    
    mountTpl('admin', 'admin-ai', function () {
      fillAdminTbody('[data-ai-table]', rows);
      renderChannels(); renderRouting();
      var aiPrivacy = q('[data-ai-privacy]'); if (aiPrivacy) aiPrivacy.value = cfg.privacy;
      var aiBudget = q('[data-ai-budget]'); if (aiBudget) aiBudget.value = cfg.budget;
      var aiRpd = q('[data-ai-rpd]'); if (aiRpd) aiRpd.value = cfg.rpd;

    q('[data-ai-add-channel]').addEventListener('click', function () { var id = 'channel-' + Date.now(); channels.push({ id: id, name: '新模型渠道', kind: 'OpenAI 兼容', endpoint: 'https://api.example.com/v1', models: [], status: '未测试', mark: 'N' }); editAiChannel(id); });
    q('[data-ai-test]').addEventListener('click', function () { var current = q('[data-ai-current-model]'); actionPending(this, '测试中…', function () { toast('连接正常 · 延迟 86ms · ' + (current ? current.textContent : '当前模型')); logAudit('大模型连接测试', current ? current.textContent : '当前模型'); }); });
    q('[data-ai-save-routing]').addEventListener('click', function () { state.aiRoutes = readRoutes(); var first = state.aiRoutes.assistant || {}; state.aiConfig = Object.assign({}, cfg, { provider: first.channel, model: first.model }); saveState(); logAudit('保存大模型场景路由', Object.keys(state.aiRoutes).length + ' 个场景'); renderAiAdmin(); toast('场景配置已保存 · 已写入审计'); });
    q('[data-ai-save]').addEventListener('click', function () {
      state.aiConfig = Object.assign({}, cfg, { provider: (state.aiRoutes || {}).assistant ? state.aiRoutes.assistant.channel : cfg.provider, model: (state.aiRoutes || {}).assistant ? state.aiRoutes.assistant.model : cfg.model, privacy: q('[data-ai-privacy]').value, budget: Math.max(0, Number(q('[data-ai-budget]').value) || 0), rpd: Math.max(0, Number(q('[data-ai-rpd]').value) || 0) });
      saveState(); logAudit('保存大模型配置', state.aiConfig.provider + ' / ' + state.aiConfig.model); toast('配置已保存 · 已写入审计');
    });
    q('[data-ai-clean]').addEventListener('click', function () {
      var m = state.aiTasks || {}; var n = 0;
      ADM_AI_TASKS.forEach(function (t) { if ((m[t.id] || t.status) === '已完成') { delete m[t.id]; n += 1; } });
      state.aiTasks = m; saveState(); logAudit('清理大模型任务', n + ' 条已完成'); renderAiAdmin(); toast(n ? '已清理 ' + n + ' 条已完成任务' : '没有可清理的任务');
    });
    bindAdminActs(function (el, act, id) {
      var t = ADM_AI_TASKS.filter(function (x) { return x.id === id; })[0]; if (!t) return;
      if (act === 'ai-detail') { openModal('任务 ' + id, '<p><b>' + t.name + '</b> · ' + t.source + '</p><p class="app-muted">当前状态 ' + ((state.aiTasks || {})[id] || t.status) + ' · 超时策略 120 秒，失败自动重试 1 次</p>', '关闭'); return; }
      if (act === 'ai-retry') { actionPending(el, '提交中…', function () { var m = state.aiTasks || {}; m[id] = '排队中'; state.aiTasks = m; saveState(); logAudit('重试大模型任务 ' + id, t.name); renderAiAdmin(); toast(id + ' 已加入队列'); }); return; }
      if (act === 'ai-cancel') confirmAdmin('中止任务', '中止 ' + id + '（' + t.name + '）？已生成的部分结果会被丢弃。', '中止任务', function () { var m = state.aiTasks || {}; m[id] = '已中止'; state.aiTasks = m; saveState(); logAudit('中止大模型任务 ' + id, t.name); renderAiAdmin(); toast('任务已中止 · 已写入审计'); });
    });
    });
}

  /* ---- 视频插件 ---- */
  function renderVideoAdmin() {
    var cfg = state.videoConfig || { enabled: true, whitelist: 'youtube.com, bilibili.com', csp: 'strict', fallback: 'safe-link' };
    var jmap = state.videoJobs || {};
    var rows = ADM_VIDEO_JOBS.map(function (v) {
      var st = jmap[v.id] || v.status;
      var stCls = st === '已完成' ? 'sb-success' : st === '失败' ? 'sb-danger' : st === '已取消' ? 'sb-gray' : st === '排队中' ? 'sb-hot' : 'sb-brand';
      var acts = '';
      if (st === '失败' || st === '已取消') acts += button('重试', 'ghost sm', 'data-adm-act="vd-retry" data-adm-id="' + v.id + '"');
      if (st === '排队中' || st === '转码中') acts += button('取消', 'ghost sm', 'data-adm-act="vd-cancel" data-adm-id="' + v.id + '"');
      acts += button('删除', 'danger sm', 'data-adm-act="vd-del" data-adm-id="' + v.id + '"');
      return '<tr><td><code>' + v.id + '</code></td><td>' + v.src + '</td><td class="num">' + v.size + '</td><td><span class="sbadge ' + stCls + '">' + st + '</span></td><td class="adm-acts">' + acts + '</td></tr>';
    }).join('');
    
    mountTpl('admin', 'admin-video', function () {
      fillAdminTbody('[data-vd-table]', rows);
      var vdOn = q('[data-vd-on]'); if (vdOn) vdOn.checked = cfg.enabled;
      var vdWl = q('[data-vd-wl]'); if (vdWl) vdWl.value = cfg.whitelist;
      var vdCsp = q('[data-vd-csp]'); if (vdCsp) vdCsp.value = cfg.csp;
      var vdFb = q('[data-vd-fb]'); if (vdFb) vdFb.value = cfg.fallback;

    q('[data-vd-refresh]').addEventListener('click', function () {
      actionPending(this, '刷新中…', function () {
        var jm = state.videoJobs || {};
        ADM_VIDEO_JOBS.forEach(function (v) { var st = jm[v.id] || v.status; if (st === '排队中') jm[v.id] = '转码中'; else if (st === '转码中') jm[v.id] = '已完成'; });
        state.videoJobs = jm; saveState(); logAudit('刷新视频转码队列', '队列推进一档'); renderVideoAdmin(); toast('队列已刷新 · 任务推进一档');
      });
    });
    q('[data-vd-export]').addEventListener('click', function () {
      var jm = state.videoJobs || {};
      if (adminCsv('bblbb-video-jobs.csv', ['任务号', '来源', '大小', '状态'], ADM_VIDEO_JOBS.map(function (v) { return [v.id, v.src, v.size, jm[v.id] || v.status]; }))) { logAudit('导出视频任务', ADM_VIDEO_JOBS.length + ' 条'); toast('任务已导出'); }
    });
    q('[data-vd-save]').addEventListener('click', function () {
      state.videoConfig = { enabled: q('[data-vd-on]').checked, whitelist: q('[data-vd-wl]').value.trim(), csp: q('[data-vd-csp]').value, fallback: q('[data-vd-fb]').value };
      saveState(); logAudit('保存视频插件配置', '白名单 ' + state.videoConfig.whitelist); toast('白名单与安全配置已保存 · 已写入审计');
    });
    bindAdminActs(function (el, act, id) {
      var v = ADM_VIDEO_JOBS.filter(function (x) { return x.id === id; })[0]; if (!v) return;
      if (act === 'vd-retry') { actionPending(el, '提交中…', function () { var jm = state.videoJobs || {}; jm[id] = '排队中'; state.videoJobs = jm; saveState(); logAudit('重试视频任务 ' + id, v.src); renderVideoAdmin(); toast(id + ' 已重新排队'); }); return; }
      if (act === 'vd-cancel') { var jm1 = state.videoJobs || {}; jm1[id] = '已取消'; state.videoJobs = jm1; saveState(); logAudit('取消视频任务 ' + id, v.src); renderVideoAdmin(); toast('任务已取消'); return; }
      if (act === 'vd-del') confirmAdmin('删除转码任务', '删除 ' + id + '（' + v.src + '）？已生成的产物会一并移除。', '删除任务', function () { var jm2 = state.videoJobs || {}; jm2[id] = '已删除'; state.videoJobs = jm2; saveState(); logAudit('删除视频任务 ' + id, v.src); renderVideoAdmin(); toast('任务已删除 · 已写入审计'); });
    });
    });
}

  /* ---- 市场与交易 ---- */
  function openMarketplaceAppConfig(app) {
    var saved = (state.marketAppConfigs || {})[app.id] || {};
    var config = {
      status: saved.status || ((state.appsStatus || {})[app.id] || app.status),
      version: saved.version || app.version,
      endpoint: saved.endpoint || app.endpoint,
      webhook: saved.webhook || 'https://hooks.bblbb.local/marketplace',
      quota: saved.quota == null ? 1000 : saved.quota,
      commission: saved.commission == null ? 10 : saved.commission,
      settlement: saved.settlement || 'T+1',
      sandbox: saved.sandbox !== false
    };
    var body = '<div class="app-config-summary"><span class="app-config-summary__icon">' + icon('puzzle', 'ic-20') + '</span><div><b>' + esc(app.name) + '</b><span>' + esc(app.kind) + ' · ' + esc(app.id) + '</span></div></div>'
      + '<div class="app-form-grid app-config-grid">'
      + '<div class="app-form-field"><label class="app-field-label">运行状态</label><select class="app-select" data-mp-config-status><option>已启用</option><option>待审批</option><option>已禁用</option><option>已下架</option><option>已驳回</option></select></div>'
      + '<div class="app-form-field"><label class="app-field-label">版本</label><input class="app-field" data-mp-config-version value="' + esc(config.version) + '"></div>'
      + '<div class="app-form-field app-form-field--full"><label class="app-field-label">应用端点</label><input class="app-field" type="url" data-mp-config-endpoint value="' + esc(config.endpoint) + '"></div>'
      + '<div class="app-form-field app-form-field--full"><label class="app-field-label">Webhook 地址</label><input class="app-field" type="url" data-mp-config-webhook value="' + esc(config.webhook) + '"></div>'
      + '<div class="app-form-field"><label class="app-field-label">每日订单上限</label><input class="app-field" type="number" min="0" data-mp-config-quota value="' + esc(config.quota) + '"></div>'
      + '<div class="app-form-field"><label class="app-field-label">平台服务费（%）</label><input class="app-field" type="number" min="0" max="100" data-mp-config-commission value="' + esc(config.commission) + '"></div>'
      + '<div class="app-form-field"><label class="app-field-label">结算周期</label><select class="app-select" data-mp-config-settlement><option>T+1</option><option>T+7</option><option>月结</option></select></div>'
      + '<label class="app-check app-form-field--full"><input type="checkbox" data-mp-config-sandbox>启用沙箱验证</label></div>'
      + '<p class="app-muted">保存后将更新应用状态、交易限额与回调配置，并写入管理员审计日志。</p>';
    var drawer = openDrawer('应用详细配置', body, button('取消', 'ghost', 'data-drawer-close') + button('保存配置', 'primary', 'data-mp-config-save'));
    q('[data-mp-config-status]', drawer).value = config.status;
    q('[data-mp-config-settlement]', drawer).value = config.settlement;
    q('[data-mp-config-sandbox]', drawer).checked = config.sandbox;
    q('[data-mp-config-save]', drawer).addEventListener('click', function () {
      var configs = state.marketAppConfigs || {};
      var nextStatus = q('[data-mp-config-status]', drawer).value;
      configs[app.id] = {
        status: nextStatus,
        version: q('[data-mp-config-version]', drawer).value.trim() || app.version,
        endpoint: q('[data-mp-config-endpoint]', drawer).value.trim(),
        webhook: q('[data-mp-config-webhook]', drawer).value.trim(),
        quota: Math.max(0, Number(q('[data-mp-config-quota]', drawer).value) || 0),
        commission: Math.min(100, Math.max(0, Number(q('[data-mp-config-commission]', drawer).value) || 0)),
        settlement: q('[data-mp-config-settlement]', drawer).value,
        sandbox: q('[data-mp-config-sandbox]', drawer).checked
      };
      state.marketAppConfigs = configs;
      var statuses = state.appsStatus || {};
      statuses[app.id] = nextStatus;
      state.appsStatus = statuses;
      saveState();
      logAudit('保存应用配置 ' + app.name, nextStatus + ' · ' + configs[app.id].version);
      closeDrawer();
      renderMarketplaceAdmin();
      toast(app.name + ' 配置已保存 · 已写入审计');
    });
  }

  function renderMarketplaceAdmin() {
    var amap = state.appsStatus || {};
    var rows = ADM_APPS.map(function (a) {
      var st = amap[a.id] || a.status;
      var stCls = st === '已启用' ? 'sb-success' : st === '待审批' ? 'sb-hot' : 'sb-gray';
      var acts = '';
      if (st === '待审批') acts += button('审批通过', 'primary sm', 'data-adm-act="mp-approve" data-adm-id="' + a.id + '"') + button('驳回', 'ghost sm', 'data-adm-act="mp-reject" data-adm-id="' + a.id + '"');
      else if (st === '已启用') acts += button('下架', 'ghost sm', 'data-adm-act="mp-offline" data-adm-id="' + a.id + '"');
      else acts += button('重新上架', 'ghost sm', 'data-adm-act="mp-online" data-adm-id="' + a.id + '"');
      acts += button('管理', 'ghost sm', 'data-adm-act="mp-view" data-adm-id="' + a.id + '"');
      return '<tr><td><b>' + a.name + '</b><span class="sub">' + a.kind + ' · ' + a.id + '</span></td><td><span class="sbadge ' + stCls + '">' + st + '</span></td><td class="num">' + a.orders + '</td><td class="adm-acts">' + acts + '</td></tr>';
    }).join('');
    var enabled = ADM_APPS.filter(function (a) { return (amap[a.id] || a.status) === '已启用'; }).length;
    var pending = ADM_APPS.filter(function (a) { return (amap[a.id] || a.status) === '待审批'; }).length;
    var recon = state.marketRecon == null ? 3 : state.marketRecon;
    var emergency = !!state.marketEmergency;
    
    mountTpl('admin', 'admin-marketplace', function () {
      fillAdminTbody('[data-mp-table]', rows);
      var mpNotice = q('[data-mp-notice]'); if (mpNotice) mpNotice.hidden = !emergency;
      var mpStats = qa('.app-stat b');
      if (mpStats[0]) mpStats[0].textContent = enabled;
      if (mpStats[1]) mpStats[1].textContent = pending;
      if (mpStats[2]) mpStats[2].textContent = recon;
      if (mpStats[3]) mpStats[3].textContent = emergency ? '已触发' : '未触发';
      var mpNotes = qa('.app-stat em');
      if (mpNotes[3]) mpNotes[3].textContent = emergency ? '需要解除' : '正常';
      var mpBtn = q('[data-mp-emergency]');
      if (mpBtn) { mpBtn.textContent = emergency ? '解除紧急禁用' : '紧急禁用'; mpBtn.className = 'btn ' + (emergency ? 'primary' : 'danger'); }

    q('[data-mp-recon]').addEventListener('click', function () {
      if (recon <= 0) { toast('没有待对账交易'); return; }
      actionPending(this, '对账中…', function () { state.marketRecon = 0; saveState(); logAudit('市场对账完成', recon + ' 笔已对平'); renderMarketplaceAdmin(); toast('对账完成 · ' + recon + ' 笔已对平'); });
    });
    q('[data-mp-export]').addEventListener('click', function () {
      var am = state.appsStatus || {};
      if (adminCsv('bblbb-marketplace.csv', ['应用', '类型', '状态', '累计订单'], ADM_APPS.map(function (a) { return [a.name, a.kind, am[a.id] || a.status, a.orders]; }))) { logAudit('导出市场交易', ADM_APPS.length + ' 个应用'); toast('交易已导出 · 已写入审计'); }
    });
    q('[data-mp-emergency]').addEventListener('click', function () {
      if (!state.marketEmergency) confirmAdmin('紧急禁用', '开启后应用市场的全部交易立即暂停，用户已购内容不受影响。', '确认紧急禁用', function () { state.marketEmergency = true; saveState(); logAudit('市场紧急禁用', '全部交易暂停'); renderMarketplaceAdmin(); toast('紧急禁用已触发'); });
      else confirmAdmin('解除紧急禁用', '解除后应用市场交易立即恢复。', '解除紧急禁用', function () { state.marketEmergency = false; saveState(); logAudit('解除市场紧急禁用', '交易恢复'); renderMarketplaceAdmin(); toast('市场交易已恢复'); });
    });
    bindAdminActs(function (el, act, id) {
      var a = ADM_APPS.filter(function (x) { return x.id === id; })[0]; if (!a) return;
      var set = function (st) { var m = state.appsStatus || {}; m[id] = st; state.appsStatus = m; saveState(); };
      if (act === 'mp-view') { openMarketplaceAppConfig(a); return; }
      if (act === 'mp-approve') { confirmAdmin('审批通过', '批准「' + a.name + '」上架？上架后立即出现在应用市场。', '批准上架', function () { set('已启用'); logAudit('审批应用 ' + a.name, '准予上架'); renderMarketplaceAdmin(); toast(a.name + ' 已批准上架'); }); return; }
      if (act === 'mp-reject') { confirmAdmin('驳回应用', '驳回「' + a.name + '」？开发者会收到通知并可修改后重新提交。', '驳回', function () { set('已驳回'); logAudit('驳回应用 ' + a.name, '审核未通过'); renderMarketplaceAdmin(); toast('已驳回 · 已写入审计'); }); return; }
      if (act === 'mp-offline') { confirmAdmin('下架应用', '下架「' + a.name + '」？新用户无法购买，已购用户不受影响。', '下架', function () { set('已下架'); logAudit('下架应用 ' + a.name, '暂停售卖'); renderMarketplaceAdmin(); toast('已下架 · 已写入审计'); }); return; }
      if (act === 'mp-online') { set('已启用'); logAudit('重新上架应用 ' + a.name, '恢复售卖'); renderMarketplaceAdmin(); toast(a.name + ' 已重新上架'); }
    });
    });
}

  /* ---- BI 看板 ---- */
  function renderBiAdmin() {
    var period = state.biPeriod || '今日';
    var data = ADM_BI[period] || ADM_BI['今日'];
    var metrics = data.map(function (m) {
      var pct = m[2] ? Math.round(m[1] / m[2] * 100) : 100;
      var down = String(m[3]).indexOf('-') === 0;
      return '<div class="app-bi-metric"><div class="app-bi-metric__head"><b>' + m[0] + '</b><span>' + m[1].toLocaleString('en-US') + (m[2] ? ' / ' + m[2].toLocaleString('en-US') : '') + '</span></div><div class="app-bi-metric__track" role="img" aria-label="' + m[0] + ' 完成度 ' + Math.min(100, pct) + '%"><i style="width:' + Math.min(100, pct) + '%"></i></div><em class="app-bi-metric__delta' + (down ? ' is-down' : '') + '">' + m[3] + ' 环比</em></div>';
    }).join('');
    
    mountTpl('admin', 'admin-bi', function () {
      var biMetrics = q('[data-bi-metrics]'); if (biMetrics) biMetrics.innerHTML = metrics;
       var biTabs = q('.app-admin-main .app-filter-tabs') || q('.app-filter-tabs');
       if (!biTabs) {
         biTabs = document.createElement('div');
         biTabs.className = 'app-filter-tabs';
         biTabs.setAttribute('role', 'tablist');
         biTabs.setAttribute('aria-label', '统计周期');
         var metricHost = q('[data-bi-metrics]');
         if (metricHost && metricHost.parentNode) metricHost.parentNode.insertBefore(biTabs, metricHost);
       }
       if (!q('[data-bi-tab="今年"]', biTabs)) biTabs.insertAdjacentHTML('beforeend', '<button role="tab" aria-selected="false" data-bi-tab="今年">今年</button>');
      qa('[data-bi-tab]').forEach(function (tab) { var on = tab.getAttribute('data-bi-tab') === period; tab.classList.toggle('is-active', on); tab.setAttribute('aria-selected', String(on)); });
      qa('.app-card p.app-muted').forEach(function (p) { if (p.textContent.indexOf('数据时间戳') === 0) p.textContent = '数据时间戳：' + (state.biStamp || '初始快照') + ' · 指标为 Mock 投影'; });

    qa('[data-bi-tab]').forEach(function (tab) { tab.addEventListener('click', function () { state.biPeriod = tab.getAttribute('data-bi-tab'); saveState(); renderBiAdmin(); }); });
    var biRefresh = q('[data-bi-refresh]');
    if (biRefresh) biRefresh.addEventListener('click', function () {
       actionPending(this, '刷新中…', function () { state.biStamp = auditTime(Date.now()); saveState(); logAudit('刷新 BI 数据', period + ' 视角'); renderBiAdmin(); toast('数据已刷新 · ' + period + ' 视角'); });
     });
    var biExport = q('[data-bi-export]');
    if (biExport) biExport.addEventListener('click', function () {
       if (adminCsv('bblbb-bi-' + period + '.csv', ['周期', '指标', '数值', '参考值', '环比'], data.map(function (m) { return [period, m[0], m[1], m[2] || '', m[3]]; }))) { logAudit('导出 BI 报表', period + ' · ' + data.length + ' 项'); toast('报表已导出 · ' + period); }
     });
    });
}

  /* ---- 用户管理 ---- */
  function usersBody() {
    var term = '';
    var input = q('[data-users-q]');
    if (input) term = input.value.trim().toLowerCase();
    var levelFilter = (q('[data-users-body]') || {}).getAttribute ? (q('[data-users-body]').getAttribute('data-users-level') || '') : '';
    var list = ADM_ADMIN_USERS.filter(function (u) { return (!term || u.name.toLowerCase().indexOf(term) >= 0) && (!levelFilter || userLevelCodeOf(u) === levelFilter.toLowerCase()); });
    if (!list.length) return '<tr><td colspan="7">' + empty('search', '没有匹配的用户', '检查用户名后重试。') + '</td></tr>';
    return list.map(function (u) {
      var st = userStatusOf(u.name);
      var stCls = st === '封禁' ? 'sb-danger' : st === '已验证' ? 'sb-success' : 'sb-brand';
      var acts = button('详情', 'ghost sm', 'data-adm-act="us-view" data-adm-id="' + esc(u.name) + '"')
        + button('调整积分', 'ghost sm', 'data-adm-act="us-adjust" data-adm-id="' + esc(u.name) + '"')
        + (st === '封禁' ? button('解封', 'primary sm', 'data-adm-act="us-enable" data-adm-id="' + esc(u.name) + '"') : button('禁用', 'danger sm', 'data-adm-act="us-ban" data-adm-id="' + esc(u.name) + '"'));
      return '<tr data-user-row="' + esc(u.name) + '" data-selectable-id="' + esc(u.name) + '"><td><div class="app-user-cell">' + profileBadge(u.name, u.name.charAt(0)) + '<span><b>' + esc(u.name) + '</b>' + (u.name === 'Chaos' ? ' <span class="sbadge sb-brand">你</span>' : '') + '<small class="sub">' + esc(u.title) + '</small></span></div></td><td>LV.' + userLevelCodeOf(u).slice(2) + '</td><td class="num">' + u.coin + '</td><td class="num">' + u.contrib + '</td><td><span class="sbadge ' + stCls + '">' + st + '</span></td><td>' + u.active + '</td><td class="adm-acts">' + acts + '</td></tr>';
    }).join('');
  }
  function renderUsersAdmin(route) {
    
    mountTpl('admin', 'admin-users', function () {
      var usersHost = q('[data-users-body]');
      var levelFilter = route && route.indexOf(':') >= 0 ? route.split(':').slice(1).join(':') : '';
      if (usersHost) { usersHost.setAttribute('data-users-level', levelFilter); usersHost.innerHTML = adminTable(['用户', '等级', 'B币', '贡献', '状态', '最近活动', '操作'], usersBody()); }
      var usersHeading = q('#page-admin .app-card__head h2'); if (usersHeading && levelFilter) usersHeading.textContent = levelFilter.replace('Lv', 'Lv.') + ' 用户列表';

    q('[data-users-q]').addEventListener('input', function () { var b = q('[data-users-body]'); if (b) { b.innerHTML = usersBody(); setupAdminEnhancements(); } });
    q('[data-us-export]').addEventListener('click', function () {
      if (adminCsv('bblbb-users.csv', ['用户', '等级', 'B币', '贡献', '状态', '最近活动'], ADM_ADMIN_USERS.map(function (u) { return [u.name, u.level, u.coin, u.contrib, userStatusOf(u.name), u.active]; }))) { logAudit('导出用户列表', ADM_ADMIN_USERS.length + ' 名'); toast('用户列表已导出 · 已写入审计'); }
    });
    bindAdminActs(function (el, act, id) {
      var u = ADM_ADMIN_USERS.filter(function (x) { return x.name === id; })[0]; if (!u) return;
      if (act === 'us-view') {
        openDrawer(u.name + ' · 用户详情', '<p>' + profileBadge(u.name, u.name.charAt(0)) + '</p><p class="app-muted">' + u.level + ' · ' + esc(u.title) + '</p><p>' + esc(u.bio) + '</p><p class="app-muted">B币 ' + u.coin + ' · 贡献 ' + u.contrib + ' · 最近活动 ' + u.active + '</p><p>状态：<span class="sbadge ' + (userStatusOf(u.name) === '封禁' ? 'sb-danger' : 'sb-success') + '">' + userStatusOf(u.name) + '</span></p><p class="app-muted">积分流水：' + ((state.userLedger || {})[u.name] || []).length + ' 条调整记录</p>', button('关闭', 'ghost', 'data-drawer-close') + ' ' + button('查看主页', 'primary', 'data-us-goto'));
        q('[data-us-goto]').addEventListener('click', function () { closeDrawer(); go('#user:' + encodeURIComponent(u.name)); });
        return;
      }
      if (act === 'us-ban') { confirmAdmin('禁用用户', '禁用 <b>' + esc(u.name) + '</b>？该用户会被立即登出并冻结发帖，历史内容保留。', '确认禁用', function () { var m = state.userStatus || {}; m[u.name] = 'banned'; state.userStatus = m; saveState(); logAudit('禁用用户 ' + u.name, u.level); renderUsersAdmin(); toast(u.name + ' 已禁用 · 已写入审计'); }); return; }
      if (act === 'us-enable') { var m2 = state.userStatus || {}; m2[u.name] = 'enabled'; state.userStatus = m2; saveState(); logAudit('解封用户 ' + u.name, '状态恢复正常'); renderUsersAdmin(); toast(u.name + ' 已解封'); return; }
      if (act === 'us-adjust') {
        openModal('调整积分 · ' + u.name, '<label class="app-field-label">币种</label><select class="app-select" aria-label="币种" data-us-kind><option value="coin">B币</option><option value="exp">经验</option><option value="contrib">贡献</option></select><label class="app-field-label" style="margin-top:12px">数额（正数为增加，负数为扣除）</label><input class="app-field" type="number" aria-label="调整数额" data-us-amount value="50"><label class="app-field-label" style="margin-top:12px">原因 <span class="app-required">*</span></label><textarea class="app-textarea" aria-label="调整原因" data-us-reason placeholder="必填，写入流水与审计"></textarea>', '取消 ' + button('确认调整', 'primary', 'data-us-confirm'));
        q('[data-us-confirm]').addEventListener('click', function () {
          var amount = Number(q('[data-us-amount]').value) || 0;
          var reason = q('[data-us-reason]').value.trim();
          if (!amount) { toast('请输入非零数额'); return; }
          if (!reason) { q('[data-us-reason]').classList.add('is-error'); toast('请填写调整原因'); return; }
          var kind = q('[data-us-kind]').value;
          var kindName = kind === 'coin' ? 'B币' : kind === 'exp' ? '经验' : '贡献';
          var ul = state.userLedger || {}; if (!ul[u.name]) ul[u.name] = [];
          ul[u.name].unshift({ t: Date.now(), kind: kindName, amount: amount, reason: reason });
          state.userLedger = ul;
          if (u.name === 'Chaos' && state.balances) state.balances[kind] = Math.max(0, (state.balances[kind] || 0) + amount);
          saveState(); logAudit('调整积分 ' + u.name, kindName + ' ' + (amount > 0 ? '+' : '') + amount + ' · ' + reason);
          closeModal(); renderUsersAdmin(); toast('调整已生效 · 流水已追加');
        });
      }
    });
    });
}

  /* ---- 角色与权限 ---- */
  function renderRolesAdmin() {
    mountTpl('admin', 'admin-roles', function () {
      var main = q('.app-admin-main');
      if (!main) return;
      var selected = state.roleEditorRole || 'admin';
      var detailOpen = state.roleDetailOpen === true;
      var roles = allAdminRoles();
      if (!roles.some(function (r) { return r.id === selected; })) selected = 'admin';
      state.roleEditorRole = selected;

      function roleSummary(role) {
        var enabled = ROLE_PERMISSION_KEYS.filter(function (p) { return rolePermsOf(role.id)[p]; }).length;
        var iconName = role.id === 'owner' || role.id === 'admin' ? 'shield' : role.id === 'mod' ? 'flag' : role.id === 'guest' ? 'globe' : role.id === 'finance' ? 'coins' : role.id === 'theme' ? 'palette' : 'users';
        return '<button type="button" class="admin-role-select' + (role.id === selected ? ' is-active' : '') + '" data-role-select="' + esc(role.id) + '" aria-current="' + String(role.id === selected) + '"><span class="admin-role-select__icon">' + icon(iconName) + '</span><span class="admin-role-select__copy"><span class="admin-role-select__name"><b>' + esc(role.name) + '</b><em>' + esc(role.type || '自定义角色') + '</em></span><small>' + esc(role.scope) + ' · ' + roleAssignedCount(role.id) + ' 位成员</small></span><span class="admin-role-select__count"><b>' + enabled + '</b>/' + ROLE_PERMISSION_KEYS.length + '</span></button>';
      }
      function permissionGroups(role) {
        var perms = rolePermsOf(role.id);
        var groups = {};
        ROLE_PERMISSION_KEYS.forEach(function (perm) { var meta = ROLE_PERMISSION_META[perm] || ['其他权限', '']; (groups[meta[0]] || (groups[meta[0]] = [])).push([perm, meta[1]]); });
        return Object.keys(groups).map(function (group) {
          return '<details class="admin-permission-group" data-permission-group="' + esc(group) + '"' + (Object.keys(groups).indexOf(group) === 0 ? ' open' : '') + '><summary><div><h3>' + esc(group) + '</h3><span data-role-group-count="' + esc(group) + '">' + groups[group].filter(function (x) { return perms[x[0]]; }).length + ' / ' + groups[group].length + ' 已启用</span></div><div class="admin-permission-group__actions"><button type="button" class="btn ghost xs" data-role-group-all="' + esc(group) + '">全选</button><button type="button" class="btn ghost xs" data-role-group-none="' + esc(group) + '">清空</button></div></summary><div class="admin-permission-list">' + groups[group].map(function (item) { return '<label class="admin-permission" data-permission-item="' + esc(item[0] + ' ' + item[1]) + '"><input type="checkbox" data-role-perm="' + esc(role.id + ':' + item[0]) + '"' + (perms[item[0]] ? ' checked' : '') + '><span><b>' + esc(item[0]) + '</b><small>' + esc(item[1]) + '</small></span></label>'; }).join('') + '</div></details>';
        }).join('');
      }
      function panel(role) {
        var isCustom = role.id.indexOf('custom-') === 0;
        if (!detailOpen) return '<section class="admin-role-directory"><header class="admin-role-directory__head"><div><div class="admin-role-kicker">ACCESS CONTROL</div><h2>角色管理</h2><p>管理角色资料、成员分配和可执行权限。点击角色进入详情设置。</p></div><div class="admin-role-directory__head-actions"><span class="sbadge sb-gray">' + roles.length + ' 个角色</span><button type="button" class="btn primary" data-adm-act="rl-create">+ 新建角色</button></div></header><div class="admin-role-directory__toolbar"><label class="app-search"><span class="sr-only">搜索角色</span><input type="search" data-role-search placeholder="搜索角色名称、类型或范围…" aria-label="搜索角色"></label><span class="admin-role-directory__hint">系统角色受保护 · 自定义角色可编辑和删除</span></div><div class="admin-role-directory__rows">' + roles.map(function (item) { var itemEnabled = ROLE_PERMISSION_KEYS.filter(function (p) { return rolePermsOf(item.id)[p]; }).length; var custom = item.id.indexOf('custom-') === 0; return '<button type="button" class="admin-role-row" data-role-open="' + esc(item.id) + '"><span class="admin-role-row__icon">' + icon(item.id === 'owner' || item.id === 'admin' ? 'shield' : item.id === 'mod' ? 'flag' : item.id === 'guest' ? 'globe' : 'users') + '</span><span class="admin-role-row__main"><b>' + esc(item.name) + (custom ? ' <em>自定义</em>' : '') + '</b><small>' + esc(item.desc || '—') + '</small></span><span class="admin-role-row__meta"><b>' + roleAssignedCount(item.id) + '</b><small>成员</small></span><span class="admin-role-row__meta"><b>' + itemEnabled + '/' + ROLE_PERMISSION_KEYS.length + '</b><small>权限</small></span><span class="admin-role-row__scope">' + esc(item.scope) + '</span><span class="admin-role-row__arrow">进入详情 ' + icon('chevron-right', 'ic-14') + '</span></button>'; }).join('') + '</div><footer class="admin-role-directory__foot"><span class="admin-role-dot"></span>角色详情内可编辑资料、管理成员和设置权限</footer></section>';
        return '<section class="admin-role-detail"><div class="admin-role-detail__back"><button type="button" class="btn ghost" data-role-back>' + icon('arrow-left', 'ic-14') + ' 返回角色列表</button><span>角色详情 / ' + esc(role.name) + '</span></div><div class="admin-role-manager"><section class="admin-role-editor"><header class="admin-role-editor__head"><div><div class="admin-role-kicker">ROLE POLICY · ' + esc(role.type || '自定义角色') + '</div><h2>' + esc(role.name) + '</h2><p>' + esc(role.desc || '配置该角色可以执行的操作。') + '</p><div class="admin-role-scope">权限范围：<b>' + esc(role.scope) + '</b> · 已分配 ' + roleAssignedCount(role.id) + ' 位成员</div></div><div class="admin-role-editor__head-actions"><span class="admin-role-editor__badge" data-role-enabled>' + ROLE_PERMISSION_KEYS.filter(function (p) { return rolePermsOf(role.id)[p]; }).length + ' / ' + ROLE_PERMISSION_KEYS.length + ' 已启用</span><button type="button" class="btn ghost sm" data-adm-act="rl-edit" data-adm-id="' + esc(role.id) + '">编辑资料</button><button type="button" class="btn ghost sm" data-adm-act="rl-members" data-adm-id="' + esc(role.id) + '">管理成员</button></div></header><div class="admin-role-editor__tools"><label class="app-search admin-permission-search"><span class="sr-only">搜索权限</span><input type="search" data-permission-search placeholder="搜索权限名称或说明…" aria-label="搜索权限"></label><button type="button" class="btn ghost sm" data-role-all>全部启用</button><button type="button" class="btn ghost sm" data-role-none>全部停用</button><span class="app-spacer"></span><button type="button" class="btn ghost sm" data-adm-act="rl-preview" data-adm-id="' + esc(role.id) + '">预览生效权限</button><button type="button" class="btn ghost sm" data-adm-act="rl-reset" data-adm-id="' + esc(role.id) + '">恢复默认</button>' + (isCustom ? '<button type="button" class="btn ghost sm danger" data-adm-act="rl-delete" data-adm-id="' + esc(role.id) + '">删除角色</button>' : '') + '</div><div class="admin-permission-groups">' + permissionGroups(role) + '</div><footer class="admin-role-editor__foot"><span class="app-muted" data-role-status>未保存的修改只保留在当前页面</span><button type="button" class="btn primary" data-adm-act="rl-save" data-adm-id="' + esc(role.id) + '">保存 ' + esc(role.name) + ' 权限</button></footer></section></div></section>';
      }
      function renderEditor() {
        roles = allAdminRoles();
        var role = roles.filter(function (r) { return r.id === selected; })[0];
        main.innerHTML = '<h1 class="sr-only" tabindex="-1">角色与权限</h1>' + panel(role);
        qa('[data-role-open]', main).forEach(function (b) { b.addEventListener('click', function () { selected = b.getAttribute('data-role-open'); state.roleEditorRole = selected; state.roleDetailOpen = true; detailOpen = true; renderEditor(); }); });
        var back = q('[data-role-back]', main); if (back) back.addEventListener('click', function () { state.roleDetailOpen = false; detailOpen = false; renderEditor(); });
        qa('[data-role-select]', main).forEach(function (b) { b.addEventListener('click', function () { selected = b.getAttribute('data-role-select'); state.roleEditorRole = selected; renderEditor(); }); });
        var sync = function () { var all = qa('[data-role-perm]', main); var on = all.filter(function (x) { return x.checked; }).length; q('[data-role-enabled]', main).textContent = on + ' / ' + all.length + ' 已启用'; q('[data-role-status]', main).textContent = '有未保存的修改'; qa('[data-permission-group]', main).forEach(function (group) { var boxes = qa('[data-role-perm]', group); var label = q('[data-role-group-count]', group); if (label) label.textContent = boxes.filter(function (x) { return x.checked; }).length + ' / ' + boxes.length + ' 已启用'; }); };
        qa('[data-role-perm]', main).forEach(function (cb) { cb.addEventListener('change', sync); });
        var roleSearch = q('[data-role-search]', main); if (roleSearch) roleSearch.addEventListener('input', function () { var term = roleSearch.value.trim().toLowerCase(); qa('[data-role-select]', main).forEach(function (b) { b.hidden = term && b.textContent.toLowerCase().indexOf(term) < 0; }); });
        var permSearch = q('[data-permission-search]', main); if (permSearch) permSearch.addEventListener('input', function () { var term = permSearch.value.trim().toLowerCase(); qa('[data-permission-group]', main).forEach(function (g) { var found = 0; qa('[data-permission-item]', g).forEach(function (item) { var ok = !term || item.textContent.toLowerCase().indexOf(term) >= 0; item.hidden = !ok; if (ok) found += 1; }); g.hidden = found === 0; }); });
        var setAll = function (value, root) { qa('[data-role-perm]', root || main).forEach(function (cb) { cb.checked = value; }); sync(); };
        var allBtn = q('[data-role-all]', main); if (allBtn) allBtn.addEventListener('click', function () { setAll(true); });
        var noneBtn = q('[data-role-none]', main); if (noneBtn) noneBtn.addEventListener('click', function () { setAll(false); });
        qa('[data-role-group-all]', main).forEach(function (btn) { btn.addEventListener('click', function () { setAll(true, btn.closest('[data-permission-group]')); }); });
        qa('[data-role-group-none]', main).forEach(function (btn) { btn.addEventListener('click', function () { setAll(false, btn.closest('[data-permission-group]')); }); });
      }
      renderEditor();
      bindAdminActs(function (el, act, id) {
        roles = allAdminRoles();
        if (act === 'rl-create') {
          openModal('新建自定义角色', '<label class="app-field-label">角色名称 <span class="app-required">*</span></label><input class="app-field" data-new-role-name placeholder="例如：活动审核员"><label class="app-field-label" style="margin-top:12px">权限范围</label><select class="app-select" data-new-role-scope><option>全站社区</option><option>所在板块</option><option>积分与交易</option><option>主题与插件</option><option>个人内容</option></select><label class="app-field-label" style="margin-top:12px">角色说明</label><textarea class="app-textarea" data-new-role-desc placeholder="说明该角色负责什么"></textarea>', '取消 ' + button('创建角色', 'primary', 'data-create-role')); q('[data-create-role]').addEventListener('click', function () { var name = q('[data-new-role-name]').value.trim(); if (!name) { toast('请输入角色名称'); return; } var custom = Array.isArray(state.customRoles) ? state.customRoles : []; var id = 'custom-' + Date.now(); custom.push({ id: id, name: name, type: '自定义角色', scope: q('[data-new-role-scope]').value, members: 0, desc: q('[data-new-role-desc]').value.trim() || '自定义授权角色。', perms: [] }); state.customRoles = custom; state.roleEditorRole = id; state.roleDetailOpen = true; saveState(); logAudit('创建自定义角色', name); closeModal(); renderRolesAdmin(); toast(name + ' 已创建 · 请继续配置权限'); });
          return;
        }
        var r = roles.filter(function (x) { return x.id === id; })[0]; if (!r) return;
        if (act === 'rl-edit') {
          var isBuiltin = ADM_ROLES.some(function (x) { return x.id === id; });
          openModal('编辑角色资料', '<label class="app-field-label">角色名称 <span class="app-required">*</span></label><input class="app-field" data-edit-role-name value="' + esc(r.name) + '"' + (isBuiltin ? ' disabled' : '') + '><label class="app-field-label" style="margin-top:12px">权限范围</label><select class="app-select" data-edit-role-scope><option' + (r.scope === '全站社区' ? ' selected' : '') + '>全站社区</option><option' + (r.scope === '全站' ? ' selected' : '') + '>全站</option><option' + (r.scope === '所在板块' ? ' selected' : '') + '>所在板块</option><option' + (r.scope === '积分与交易' ? ' selected' : '') + '>积分与交易</option><option' + (r.scope === '主题与插件' ? ' selected' : '') + '>主题与插件</option><option' + (r.scope === '个人内容' ? ' selected' : '') + '>个人内容</option></select><label class="app-field-label" style="margin-top:12px">角色说明</label><textarea class="app-textarea" data-edit-role-desc>' + esc(r.desc || '') + '</textarea>', '取消 ' + button('保存资料', 'primary', 'data-save-role-meta')); q('[data-save-role-meta]').addEventListener('click', function () { var name = q('[data-edit-role-name]').value.trim(); if (!name) { toast('请输入角色名称'); return; } var next = { name: name, scope: q('[data-edit-role-scope]').value, desc: q('[data-edit-role-desc]').value.trim() || '自定义授权角色。' }; if (isBuiltin) { var meta = state.roleMeta || {}; meta[id] = Object.assign(meta[id] || {}, next, { name: r.name }); state.roleMeta = meta; } else { (state.customRoles || []).forEach(function (x) { if (x.id === id) Object.assign(x, next); }); } saveState(); logAudit('编辑角色资料', name); closeModal(); renderRolesAdmin(); toast('角色资料已保存 · 已写入审计'); });
          return;
        }
        if (act === 'rl-members') {
          var selectedNames = {}; ADM_ADMIN_USERS.forEach(function (u) { if (userRoleOf(u.name) === id) selectedNames[u.name] = true; });
          openModal(r.name + ' · 管理成员', '<p class="app-muted">勾选成员后保存，成员将立即切换到该角色。每个成员当前仅支持一个主角色。</p><div class="admin-member-picker">' + ADM_ADMIN_USERS.map(function (u) { return '<label class="admin-member-picker__item"><input type="checkbox" data-role-member="' + esc(u.name) + '"' + (selectedNames[u.name] ? ' checked' : '') + '><span><b>' + esc(u.name) + '</b><small>当前角色：' + esc(allAdminRoles().filter(function (x) { return x.id === userRoleOf(u.name); })[0].name) + '</small></span></label>'; }).join('') + '</div>', '取消 ' + button('保存成员分配', 'primary', 'data-save-role-members')); q('[data-save-role-members]').addEventListener('click', function () { var map = state.userRoles || {}; qa('[data-role-member]').forEach(function (cb) { var name = cb.getAttribute('data-role-member'); if (cb.checked) map[name] = id; else if (userRoleOf(name) === id) map[name] = 'member'; }); state.userRoles = map; saveState(); logAudit('调整角色成员', r.name + ' · ' + qa('[data-role-member]:checked').length + ' 位'); closeModal(); renderRolesAdmin(); toast('成员分配已保存 · 已写入审计'); });
          return;
        }
        if (act === 'rl-preview') {
          var m = rolePermsOf(id); if (id === selected) qa('[data-role-perm]', main).forEach(function (cb) { m[cb.getAttribute('data-role-perm').split(':').slice(1).join(':')] = cb.checked; });
          var list = ROLE_PERMISSION_KEYS.filter(function (p) { return m[p]; });
          openModal(r.name + ' · 生效权限', '<ul class="app-perm-list">' + (list.length ? list.map(function (p) { return '<li><code>' + p + '</code><span>' + esc((ROLE_PERMISSION_META[p] || ['', ''])[1]) + '</span></li>'; }).join('') : '<li class="app-muted">无生效权限</li>') + '</ul><p class="app-muted">' + list.length + ' / ' + ROLE_PERMISSION_KEYS.length + ' 项权限开启 · 范围：' + r.scope + '</p>', '关闭');
          return;
        }
        if (act === 'rl-reset') { var m2 = state.rolePerms || {}; var def = {}; ROLE_PERMISSION_KEYS.forEach(function (p) { def[p] = r.perms.indexOf(p) >= 0; }); m2[id] = def; state.rolePerms = m2; saveState(); logAudit('恢复角色默认权限', r.name); renderRolesAdmin(); toast(r.name + ' 权限已恢复默认'); return; }
        if (act === 'rl-delete') { confirmAdmin('删除自定义角色', '确定删除「' + esc(r.name) + '」？已分配成员将回到成员角色。', '确认删除', function () { state.customRoles = (state.customRoles || []).filter(function (x) { return x.id !== id; }); if (state.rolePerms) delete state.rolePerms[id]; var users = state.userRoles || {}; Object.keys(users).forEach(function (name) { if (users[name] === id) delete users[name]; }); state.userRoles = users; state.roleEditorRole = 'admin'; saveState(); logAudit('删除自定义角色', r.name); renderRolesAdmin(); toast('角色已删除 · 已写入审计'); }); return; }
        if (act === 'rl-save') {
          var m3 = state.rolePerms || {}; var next = {};
          qa('[data-role-perm]', main).forEach(function (cb) { var key = cb.getAttribute('data-role-perm').split(':'); if (key[0] === id) next[key.slice(1).join(':')] = cb.checked; });
          m3[id] = next; state.rolePerms = m3; saveState();
          var off = ROLE_PERMISSION_KEYS.filter(function (p) { return !next[p]; });
          logAudit('保存角色 ' + r.name, off.length ? '停用 ' + off.join(', ') : '保留全部权限');
          renderRolesAdmin(); toast(r.name + ' 已保存 · 已写入审计');
        }
      });
    });
}

  /* ---- 板块管理 ---- */
  function renderBoardsAdmin() {
    var rows = admBoardRows().map(function (b) {
      var mod = boardModOf(b.slug);
      var name = mod.name || b.name;
      var vis = mod.visibility || '公开';
      var policy = mod.policy || '登录即可';
      var pinned = !!mod.pinned;
      return '<tr><td><b>' + esc(name) + '</b>' + (pinned ? ' <span class="sbadge sb-success">置顶</span>' : '') + '<span class="sub">' + esc(mod.desc || b.desc) + '</span></td><td class="num">' + b.count + '</td><td>' + vis + '</td><td>' + policy + '</td><td>Chaos</td><td class="adm-acts">' + button('编辑', 'ghost sm', 'data-adm-act="bd-edit" data-adm-id="' + esc(b.slug) + '"') + button(pinned ? '取消置顶' : '置顶', 'ghost sm', 'data-adm-act="bd-pin" data-adm-id="' + esc(b.slug) + '"') + '</td></tr>';
    }).join('');
    
    mountTpl('admin', 'admin-boards', function () {
      fillAdminTbody('[data-bd-table]', rows);

    q('[data-bd-export]').addEventListener('click', function () {
      if (adminCsv('bblbb-boards.csv', ['板块', '描述', '主题数', '可见性', '发帖策略'], admBoardRows().map(function (b) { var m = boardModOf(b.slug); return [m.name || b.name, m.desc || b.desc, b.count, m.visibility || '公开', m.policy || '登录即可']; }))) { logAudit('导出板块列表', admBoardRows().length + ' 个'); toast('板块列表已导出'); }
    });
    q('[data-bd-create]').addEventListener('click', function () {
      openModal('新建板块', '<label class="app-field-label">板块名称 <span class="app-required">*</span></label><input class="app-field" aria-label="板块名称" data-bd-name placeholder="例如：数据库"><label class="app-field-label" style="margin-top:12px">Slug（URL 标识）</label><input class="app-field" aria-label="板块 slug" data-bd-slug placeholder="例如：database"><label class="app-field-label" style="margin-top:12px">描述</label><textarea class="app-textarea" aria-label="板块描述" data-bd-desc placeholder="一句话说明板块定位"></textarea>', '取消 ' + button('创建板块', 'primary', 'data-bd-confirm'));
      q('[data-bd-confirm]').addEventListener('click', function () {
        var name = q('[data-bd-name]').value.trim();
        var slug = (q('[data-bd-slug]').value.trim() || name).toLowerCase().replace(/\s+/g, '-');
        if (!name) { q('[data-bd-name]').classList.add('is-error'); toast('请填写板块名称'); return; }
        if (admBoardRows().some(function (b) { return b.slug === slug; })) { toast('已存在相同 Slug 的板块'); return; }
        state.boardNew = [{ slug: slug, name: name, desc: q('[data-bd-desc]').value.trim() || '新板块，等待补充描述。' }].concat(state.boardNew || []);
        saveState(); logAudit('新建板块 ' + name, '/' + slug); closeModal(); renderBoardsAdmin(); toast('板块已创建 · 已写入审计');
      });
    });
    bindAdminActs(function (el, act, id) {
      var b = admBoardRows().filter(function (x) { return x.slug === id; })[0]; if (!b) return;
      if (act === 'bd-pin') {
        var m = state.boardMod || {}; var cur = m[id] || {};
        cur.pinned = !cur.pinned; m[id] = cur; state.boardMod = m; saveState();
        logAudit(cur.pinned ? '置顶板块' : '取消置顶板块', b.name); renderBoardsAdmin(); toast(cur.pinned ? '已置顶' : '已取消置顶');
        return;
      }
      if (act === 'bd-edit') {
        var mod = boardModOf(id);
        openModal('编辑板块 · ' + (mod.name || b.name), '<label class="app-field-label">板块名称</label><input class="app-field" aria-label="板块名称" data-bd-e-name value="' + esc(mod.name || b.name) + '"><label class="app-field-label" style="margin-top:12px">描述</label><textarea class="app-textarea" aria-label="板块描述" data-bd-e-desc>' + esc(mod.desc || b.desc) + '</textarea><label class="app-field-label" style="margin-top:12px">可见性</label><select class="app-select" aria-label="可见性" data-bd-e-vis>' + ['公开', '隐藏', '关闭发帖'].map(function (o) { return '<option' + ((mod.visibility || '公开') === o ? ' selected' : '') + '>' + o + '</option>'; }).join('') + '</select><label class="app-field-label" style="margin-top:12px">发帖策略</label><select class="app-select" aria-label="发帖策略" data-bd-e-policy>' + ['登录即可', '需 LV.2 及以上', '邀请制'].map(function (o) { return '<option' + ((mod.policy || '登录即可') === o ? ' selected' : '') + '>' + o + '</option>'; }).join('') + '</select>', '取消 ' + button('保存板块', 'primary', 'data-bd-e-confirm'));
        q('[data-bd-e-confirm]').addEventListener('click', function () {
          var nm = q('[data-bd-e-name]').value.trim();
          if (!nm) { toast('板块名称不能为空'); return; }
          var m2 = state.boardMod || {};
          m2[id] = { name: nm, desc: q('[data-bd-e-desc]').value.trim(), visibility: q('[data-bd-e-vis]').value, policy: q('[data-bd-e-policy]').value, pinned: !!(m2[id] && m2[id].pinned) };
          state.boardMod = m2; saveState(); logAudit('编辑板块 ' + nm, m2[id].visibility + ' · ' + m2[id].policy); closeModal(); renderBoardsAdmin(); toast('板块已保存 · 已写入审计');
        });
      }
    });
    });
}

  /* ---- 帖子与文章 ---- */
  function postsBody(key) {
    var list = ADM_POSTS.filter(function (p) { return key === 'all' || postStatusOf(p.id) === key; });
    // The admin enhancement adds a leading selection column, so the empty
    // state must span the full seven-column table as well.
    if (!list.length) return '<tr><td colspan="7">' + empty('file-text', '该状态下没有内容', '切换其他状态查看。') + '</td></tr>';
    return list.map(function (p) {
      var st = postStatusOf(p.id);
      var stCls = st === '待审核' ? 'sb-hot' : st === '精华' ? 'sb-brand' : st === '已隐藏' || st === '已删除' ? 'sb-gray' : 'sb-success';
      var acts = '';
      if (st === '待审核') acts = button('通过', 'primary sm', 'data-adm-act="ps-approve" data-adm-id="' + p.id + '"') + button('驳回', 'danger sm', 'data-adm-act="ps-reject" data-adm-id="' + p.id + '"');
      else if (st === '公开' || st === '已发布') acts = button('设为精华', 'ghost sm', 'data-adm-act="ps-featured" data-adm-id="' + p.id + '"') + button('隐藏', 'ghost sm', 'data-adm-act="ps-hide" data-adm-id="' + p.id + '"');
      else if (st === '精华') acts = button('取消精华', 'ghost sm', 'data-adm-act="ps-unfeature" data-adm-id="' + p.id + '"') + button('隐藏', 'ghost sm', 'data-adm-act="ps-hide" data-adm-id="' + p.id + '"');
      else if (st === '已隐藏') acts = button('恢复', 'ghost sm', 'data-adm-act="ps-restore" data-adm-id="' + p.id + '"') + button('删除', 'danger sm', 'data-adm-act="ps-delete" data-adm-id="' + p.id + '"');
      else if (st === '已删除') acts = button('恢复', 'ghost sm', 'data-adm-act="ps-restore" data-adm-id="' + p.id + '"');
      var tid = p.id.replace(/^p-/, '');
      var sub = TOPICS[tid] ? link('查看原帖', '#topic:' + tid) : '<span class="app-muted">已被隐藏</span>';
      return '<tr data-selectable-id="' + esc(p.id) + '"><td><b>' + esc(p.title) + '</b><span class="sub">' + sub + '</span></td><td>' + p.author + '</td><td>' + p.board + '</td><td><span class="sbadge ' + stCls + '">' + st + '</span></td><td>' + p.time + '</td><td class="adm-acts">' + acts + button('Diff', 'ghost sm', 'data-adm-act="ps-diff" data-adm-id="' + p.id + '"') + '</td></tr>';
    }).join('');
  }
  function renderPostsAdmin() {
    var key = state.postFilter || 'all';
    var tabs = [['all', '全部'], ['待审核', '待审核'], ['公开', '公开'], ['精华', '精华'], ['已隐藏', '已隐藏'], ['已删除', '已删除']];
    var tabsHtml = tabs.map(function (t) {
      var n = t[0] === 'all' ? ADM_POSTS.length : ADM_POSTS.filter(function (p) { return postStatusOf(p.id) === t[0]; }).length;
      return '<button class="' + (key === t[0] ? 'is-active' : '') + '" role="tab" aria-selected="' + (key === t[0] ? 'true' : 'false') + '" data-ps-tab="' + t[0] + '">' + t[1] + ' ' + n + '</button>';
    }).join('');
    mountTpl('admin', 'admin-posts', function () {
      var postsTabs = q('.app-filter-tabs'); if (postsTabs) postsTabs.innerHTML = tabsHtml;
      fillAdminTbody('[data-posts-body]', postsBody(key));
      qa('[data-ps-tab]').forEach(function (tab) { tab.addEventListener('click', function () { state.postFilter = tab.getAttribute('data-ps-tab'); saveState(); renderPostsAdmin(); }); });
      q('[data-ps-export]').addEventListener('click', function () {
        if (adminCsv('bblbb-posts.csv', ['标题', '作者', '板块', '状态', '时间'], ADM_POSTS.map(function (p) { return [p.title, p.author, p.board, postStatusOf(p.id), p.time]; }))) { logAudit('导出内容清单', ADM_POSTS.length + ' 条'); toast('内容清单已导出'); }
      });
      bindAdminActs(function (el, act, id) {
        var p = ADM_POSTS.filter(function (x) { return x.id === id; })[0]; if (!p) return;
        var set = function (st, note) { var m = state.postStatus || {}; m[id] = st; state.postStatus = m; saveState(); logAudit(note, p.title.slice(0, 20) + ' → ' + st); renderPostsAdmin(); toast(note + ' · 已写入审计'); };
        if (act === 'ps-approve') { set('公开', '审核通过'); return; }
        if (act === 'ps-featured') { set('精华', '设为精华'); return; }
        if (act === 'ps-unfeature') { set('公开', '取消精华'); return; }
        if (act === 'ps-restore') { set('公开', '内容恢复'); return; }
        if (act === 'ps-reject') { rejectPostModal(p, function (reason) { var m = state.postRejections || {}; m[id] = { reason: reason, at: Date.now() }; state.postRejections = m; set('已隐藏', '审核驳回 · ' + reason); }); return; }
        if (act === 'ps-hide') { confirmAdmin('隐藏帖子', '隐藏「' + esc(p.title) + '」？前台与搜索不再展示，作者会收到通知。', '确认隐藏', function () { set('已隐藏', '内容隐藏'); }); return; }
        if (act === 'ps-delete') { confirmAdmin('删除帖子', '删除「' + esc(p.title) + '」？删除动作只能由管理员从审计日志追溯。', '确认删除', function () { set('已删除', '内容删除'); }); }
        if (act === 'ps-diff') { showPostDiff(p); }
      });
    });
  }


  /* ---- 标签管理 ---- */
  function renderTagsAdmin() {
    var allTags = ADM_TAGS.concat((state.tagNew || []).map(function (n) { return { name: n, uses: 0 }; }));
    var rows = allTags.map(function (t) {
      var ts = tagStateOf(t.name);
      var st = ts.status === 'merged' ? '已合并' : ts.status === 'disabled' ? '已停用' : '正常';
      var stCls = st === '正常' ? 'sb-success' : 'sb-gray';
      var acts = '';
      if (st === '正常') acts = button('停用', 'ghost sm', 'data-adm-act="tg-disable" data-adm-id="' + esc(t.name) + '"') + button('合并', 'ghost sm', 'data-adm-act="tg-merge" data-adm-id="' + esc(t.name) + '"');
      else if (st === '已停用') acts = button('启用', 'primary sm', 'data-adm-act="tg-enable" data-adm-id="' + esc(t.name) + '"');
      return '<tr><td><b>' + esc(t.name) + '</b>' + (ts.mergedInto ? '<span class="sub">已并入 ' + esc(ts.mergedInto) + '</span>' : '') + '</td><td class="num">' + t.uses + '</td><td><span class="sbadge ' + stCls + '">' + st + '</span></td><td class="adm-acts">' + acts + '</td></tr>';
    }).join('');
    
    mountTpl('admin', 'admin-tags', function () {
      fillAdminTbody('[data-tg-table]', rows);

    q('[data-tg-export]').addEventListener('click', function () {
      if (adminCsv('bblbb-tags.csv', ['标签', '使用次数', '状态'], allTags.map(function (t) { var s = tagStateOf(t.name); return [t.name, t.uses, s.status === 'merged' ? '已合并 → ' + s.mergedInto : s.status === 'disabled' ? '已停用' : '正常']; }))) { logAudit('导出标签列表', allTags.length + ' 个'); toast('标签列表已导出'); }
    });
    q('[data-tg-create]').addEventListener('click', function () {
      openModal('新建标签', '<label class="app-field-label">标签名称 <span class="app-required">*</span></label><input class="app-field" aria-label="标签名称" data-tg-name placeholder="例如：Wasm"><p class="app-field-help">创建后立即出现在发帖标签选择器中。</p>', '取消 ' + button('创建标签', 'primary', 'data-tg-confirm'));
      q('[data-tg-confirm]').addEventListener('click', function () {
        var name = q('[data-tg-name]').value.trim();
        if (!name) { q('[data-tg-name]').classList.add('is-error'); toast('请填写标签名称'); return; }
        if (allTags.some(function (t) { return t.name === name; })) { toast('标签已存在'); return; }
        state.tagNew = [name].concat(state.tagNew || []);
        saveState(); logAudit('新建标签 ' + name, '初始使用 0 次'); closeModal(); renderTagsAdmin(); toast('标签已创建 · 已写入审计');
      });
    });
    bindAdminActs(function (el, act, id) {
      var t = allTags.filter(function (x) { return x.name === id; })[0]; if (!t) return;
      var ts = tagStateOf(id);
      if (act === 'tg-enable') { var m1 = state.tagStatus || {}; delete m1[id]; state.tagStatus = m1; saveState(); logAudit('启用标签 ' + id, '恢复正常'); renderTagsAdmin(); toast('标签已启用'); return; }
      if (act === 'tg-disable') { confirmAdmin('停用标签', '停用「' + esc(id) + '」？已有 ' + t.uses + ' 次使用，历史内容不受影响，仅新帖子无法选择。', '停用标签', function () { var m2 = state.tagStatus || {}; m2[id] = 'disabled'; state.tagStatus = m2; saveState(); logAudit('停用标签 ' + id, t.uses + ' 次使用'); renderTagsAdmin(); toast('标签已停用 · 已写入审计'); }); return; }
      if (act === 'tg-merge') {
        var targets = allTags.filter(function (x) { return x.name !== id && tagStateOf(x.name).status === 'normal'; });
        if (!targets.length) { toast('没有可合并的目标标签'); return; }
        openModal('合并标签 · ' + id, '<p class="app-muted">「' + esc(id) + '」的 ' + t.uses + ' 次使用会全部计入目标标签。</p><label class="app-field-label">合并到</label><select class="app-select" aria-label="合并目标" data-tg-target>' + targets.map(function (x) { return '<option>' + esc(x.name) + '</option>'; }).join('') + '</select>', '取消 ' + button('确认合并', 'danger', 'data-tg-merge-confirm'));
        q('[data-tg-merge-confirm]').addEventListener('click', function () {
          var target = q('[data-tg-target]').value;
          var ms = state.tagStatus || {}; ms[id] = 'merged'; state.tagStatus = ms;
          var mm = state.tagMergedInto || {}; mm[id] = target; state.tagMergedInto = mm;
          saveState(); logAudit('合并标签 ' + id + ' → ' + target, t.uses + ' 次使用转移'); closeModal(); renderTagsAdmin(); toast('已合并到 ' + target + ' · 已写入审计');
        });
      }
    });
    });
}

  /* ---- 附件管理 ---- */
  function renderAttachmentsAdmin() {
    var scan = state.fileScan || {};
    var deleted = state.filesDeleted || {};
    var rows = ADM_FILES.map(function (f) {
      var del = !!deleted[f.id];
      var st = scan[f.id] || f.scan;
      var stCls = st === '已通过' ? 'sb-success' : st === '待扫描' ? 'sb-hot' : 'sb-brand';
      var acts = '';
      if (del) acts = '<span class="app-muted">已移除</span>';
      else {
        if (st !== '已通过') acts += '<button type="button" class="btn ghost sm adm-file-action adm-file-action--scan" data-adm-act="fl-scan" data-adm-id="' + f.id + '">' + icon('activity', 'ic-13') + '<span>扫描</span></button>';
        acts += '<button type="button" class="btn ghost sm adm-file-action" data-adm-act="fl-link" data-adm-id="' + f.id + '">' + icon('link', 'ic-13') + '<span>临时链接</span></button>';
        acts += '<div class="drop-wrap adm-file-menu"><button type="button" class="btn-icon adm-file-menu__toggle" onclick="toggleDrop(event,\'adm-file-menu-' + f.id + '\')" aria-label="更多操作" title="更多操作" aria-haspopup="menu" aria-expanded="false"><span class="adm-file-menu__dots" aria-hidden="true"></span></button><div class="dropdown dd-right adm-file-menu__dropdown" id="adm-file-menu-' + f.id + '" role="menu" aria-label="' + esc(f.name) + ' 的更多操作"><button type="button" class="dd-item danger" data-adm-act="fl-del" data-adm-id="' + f.id + '">' + icon('x', 'ic-13') + '<span>删除附件</span></button></div></div>';
      }
      return '<tr class="' + (del ? 'is-file-removed' : '') + '"><td><b>' + esc(f.name) + '</b><span class="sub">' + f.id + '</span></td><td>' + f.user + '</td><td class="num">' + f.size + '</td><td>' + f.scope + '</td><td>' + (del ? '<span class="sbadge sb-gray">已删除</span>' : '<span class="sbadge ' + stCls + '">' + st + '</span>') + '</td><td class="adm-acts adm-file-acts">' + acts + '</td></tr>';
    }).join('');
    
    mountTpl('admin', 'admin-attachments', function () {
      fillAdminTbody('[data-af-table]', rows);

    q('[data-fl-scanall]').addEventListener('click', function () {
      actionPending(this, '扫描中…', function () {
        var sm = state.fileScan || {}; var n = 0;
        ADM_FILES.forEach(function (f) { if ((sm[f.id] || f.scan) !== '已通过') { sm[f.id] = '已通过'; n += 1; } });
        state.fileScan = sm; saveState(); logAudit('批量安全扫描', n + ' 个附件全部通过'); renderAttachmentsAdmin(); toast('扫描完成 · ' + n + ' 个附件全部通过');
      });
    });
    q('[data-fl-export]').addEventListener('click', function () {
      if (adminCsv('bblbb-attachments.csv', ['文件', '上传者', '大小', '范围', '扫描状态'], ADM_FILES.map(function (f) { return [f.name, f.user, f.size, f.scope, (state.fileScan || {})[f.id] || f.scan]; }))) { logAudit('导出附件清单', ADM_FILES.length + ' 个'); toast('清单已导出'); }
    });
    bindAdminActs(function (el, act, id) {
      var f = ADM_FILES.filter(function (x) { return x.id === id; })[0]; if (!f) return;
      if (act === 'fl-scan') { actionPending(el, '扫描中…', function () { var sm = state.fileScan || {}; sm[id] = '已通过'; state.fileScan = sm; saveState(); logAudit('安全扫描 ' + f.name, '通过 · 0 风险'); renderAttachmentsAdmin(); toast(f.name + ' 扫描通过'); }); return; }
      if (act === 'fl-link') { var url = 'https://bblbb.local/dl/' + f.id + '?t=' + (Date.now() % 100000) + '&exp=3600'; openModal('临时链接 · ' + f.name, '<p class="app-muted">链接 1 小时后过期，访问会记录审计。</p><div class="app-secret"><input readonly value="' + url + '">' + button('复制链接', 'ghost', 'data-copy-share') + '</div>', '关闭'); return; }
      if (act === 'fl-del') confirmAdmin('删除附件', '删除 ' + esc(f.name) + '（' + f.size + '）？引用它的内容将显示缺失占位。', '确认删除', function () { var dm = state.filesDeleted || {}; dm[id] = true; state.filesDeleted = dm; saveState(); logAudit('删除附件', f.name); renderAttachmentsAdmin(); toast('附件已删除 · 已写入审计'); });
    });
    });
}

  /* ---- 通知与邮件 ---- */
  function renderNotificationsAdmin() {
    var fail = state.notifFail == null ? 2 : state.notifFail;
    var outbox = state.notifOutbox || [];
    var rows = ADM_TEMPLATES.map(function (t) {
      return '<tr><td><b>' + t.name + '</b><span class="sub">' + t.mode + '</span></td><td class="num">' + t.queue + '</td><td class="num">' + (t.id === 'tpl-verify' ? fail : 0) + '</td><td class="adm-acts">' + button('预览', 'ghost sm', 'data-adm-act="nt-preview" data-adm-id="' + t.id + '"') + button('发送测试', 'ghost sm', 'data-adm-act="nt-test" data-adm-id="' + t.id + '"') + '</td></tr>';
    }).join('');
    var outboxHtml = outbox.length ? outbox.map(function (o, i) { return '<div class="admin-outbox-row"><div><b>' + esc(o.title) + '</b><span class="sub">' + esc(o.target) + ' · ' + auditTime(o.t) + ' · ' + esc(o.body.slice(0, 24)) + '…</span></div>' + button('撤回', 'ghost sm', 'data-adm-act="nt-recall" data-adm-id="' + i + '"') + '</div>'; }).join('') : empty('bell', '还没有已发送的广播', '使用左侧表单发送第一条全员广播。');
    
    mountTpl('admin', 'admin-notifications', function () {
      fillAdminTbody('[data-nt-table]', rows);
      var ntFailEl = q('[data-nt-fail]'); if (ntFailEl) ntFailEl.textContent = fail;
      var ntOutboxEl = q('[data-nt-outbox]'); if (ntOutboxEl) ntOutboxEl.innerHTML = outboxHtml;

    q('[data-nt-send]').addEventListener('click', function () {
      var title = q('[data-nt-title]').value.trim();
      var body = q('[data-nt-body]').value.trim();
      if (!title || !body) { toast('请填写标题与内容'); return; }
      state.notifOutbox = [{ t: Date.now(), title: title, body: body, target: q('[data-nt-target]').value }].concat(state.notifOutbox || []);
      saveState(); logAudit('发送全员广播', title); renderNotificationsAdmin(); toast('广播已发送 · 邮件已入队');
    });
    q('[data-nt-retry]').addEventListener('click', function () {
      if (fail <= 0) { toast('没有失败队列'); return; }
      actionPending(this, '重试中…', function () { state.notifFail = 0; saveState(); logAudit('重试管道失败邮件', fail + ' 条已重发'); renderNotificationsAdmin(); toast('失败邮件已重试'); });
    });
    bindAdminActs(function (el, act, id) {
      var t = ADM_TEMPLATES.filter(function (x) { return x.id === id; })[0];
      if (act === 'nt-preview') { if (t) openModal('模板预览 · ' + t.name, '<p class="app-muted">' + t.mode + '</p><blockquote class="topic-prose">' + esc(t.sample) + '</blockquote>', '关闭'); return; }
      if (act === 'nt-test') { if (t) actionPending(el, '发送中…', function () { logAudit('发送测试邮件', t.name); toast('测试邮件已加入队列'); }); return; }
      if (act === 'nt-recall') {
        var idx = Number(id); var list = state.notifOutbox || []; var item = list[idx];
        if (!item) return;
        confirmAdmin('撤回广播', '撤回「' + esc(item.title) + '」？未读用户将不再看到这条广播。', '撤回广播', function () { list.splice(idx, 1); state.notifOutbox = list; saveState(); logAudit('撤回广播', item.title); renderNotificationsAdmin(); toast('广播已撤回'); });
      }
    });
    });
}

  /* ---- 审计日志 ---- */
  function renderAuditAdmin() {
    if (!state.auditLog || !state.auditLog.length) {
      state.auditLog = ADM_AUDIT_SEED.map(function (x, i) { return { t: Date.now() - (i + 1) * 3600000, actor: x.actor, text: x.text, object: x.object }; });
      saveState();
    }
    var rows = state.auditLog.slice(0, 80).map(function (x) {
      return '<tr><td>' + auditTime(x.t) + '</td><td>' + esc(x.actor) + '</td><td>' + esc(x.text) + '</td><td>' + esc(x.object || '—') + '</td></tr>';
    }).join('');
    
    mountTpl('admin', 'admin-audit', function () {
      fillAdminTbody('[data-audit-table]', rows);
      var auditCountEl = q('[data-audit-count]'); if (auditCountEl) auditCountEl.textContent = '共 ' + state.auditLog.length + ' 条';

    q('[data-audit-q]').addEventListener('input', function () {
      var term = this.value.trim().toLowerCase();
      var visible = 0;
      var tbody = q('#page-admin .app-table tbody');
      if (!tbody) return;
      qa('tr', tbody).forEach(function (row) { var ok = !term || row.textContent.toLowerCase().indexOf(term) >= 0; row.hidden = !ok; if (ok) visible += 1; });
    });
    q('[data-audit-export]').addEventListener('click', function () {
      if (adminCsv('bblbb-audit.csv', ['时间', '操作人', '操作', '对象'], state.auditLog.map(function (x) { return [auditTime(x.t), x.actor, x.text, x.object || '']; }))) { logAudit('导出审计日志', state.auditLog.length + ' 条'); toast('审计日志已导出'); }
    });
    q('[data-audit-clear]').addEventListener('click', function () {
      confirmAdmin('清空审计日志', '将移除全部 ' + state.auditLog.length + ' 条记录，并写入一条清空记录。生产环境该操作不可用。', '确认清空', function () {
        state.auditLog = []; saveState();
        logAudit('清空审计日志', 'request_' + Math.random().toString(16).slice(2, 8));
        renderAuditAdmin(); toast('日志已清空 · 清空动作已记录');
      });
    });
    });
}

  /* ---- 系统设置（模拟真实后台：变更追踪 / 校验 / 异步保存 / 失败重试 / 离开拦截 / 审计） ---- */
  var SYS_DEFAULTS = { register: true, emailVerify: true, anonymous: false, maintain: false, rss: true, siteName: 'BBLBB 社区', lang: 'zh-CN', source: 'https://bblbb.local', rateLimit: 30 };
  var SYS_TOGGLE_KEYS = ['register', 'emailVerify', 'anonymous', 'rss', 'maintain'];
  var SYS_FIELD_KEYS = ['siteName', 'lang', 'source', 'rateLimit'];
  var SYS_LABELS = { register: '开放注册', emailVerify: '邮箱验证', anonymous: '匿名回复', rss: 'RSS / Atom', maintain: '维护模式', siteName: '站点名称', lang: '默认语言', source: '公开源', rateLimit: 'API 限流' };
  function sysNorm(key, value) { return key === 'rateLimit' ? (Number(value) || 0) : String(value == null ? '' : value); }
  function sysHuman(key, value) {
    if (SYS_TOGGLE_KEYS.indexOf(key) >= 0) return value ? '开' : '关';
    if (key === 'lang') return value === 'zh-CN' ? '简体中文' : 'English';
    if (key === 'rateLimit') return String(value) + ' 次/分';
    return value ? ('“' + value + '”') : '（空）';
  }
  function sysInputFor(key) { return q(key === 'siteName' ? '[data-sys-name]' : key === 'lang' ? '[data-sys-lang]' : key === 'source' ? '[data-sys-source]' : '[data-sys-ratelimit]'); }
  function sysReadForm() {
    var next = { siteName: sysInputFor('siteName').value.trim(), lang: sysInputFor('lang').value, source: sysInputFor('source').value.trim(), rateLimit: sysInputFor('rateLimit').value };
    SYS_TOGGLE_KEYS.forEach(function (k) { next[k] = q('[data-sys-key="' + k + '"]').checked; });
    return next;
  }
  function sysValidate(next) {
    var errors = {};
    if (!next.siteName) errors.siteName = '站点名称不能为空';
    else if (next.siteName.length > 40) errors.siteName = '站点名称不能超过 40 个字符';
    if (!/^https?:\/\S+\.\S+/.test(next.source)) errors.source = '请输入有效的 http(s):// 地址';
    var n = Number(next.rateLimit);
    if (next.rateLimit === '' || !isFinite(n) || n !== Math.floor(n) || n < 1 || n > 10000) errors.rateLimit = '限流需为 1 - 10000 的整数';
    return errors;
  }
  function renderSettingsAdmin() {
    var s = state.sysSettings || Object.assign({}, SYS_DEFAULTS);
    var saving = false;
    var changed = [];
    mountTpl('admin', 'admin-settings', function () {
      SYS_TOGGLE_KEYS.forEach(function (k) { var el = q('[data-sys-key="' + k + '"]'); if (el) el.checked = !!s[k]; });
      sysInputFor('siteName').value = s.siteName;
      sysInputFor('lang').value = s.lang;
      sysInputFor('source').value = s.source;
      sysInputFor('rateLimit').value = s.rateLimit;
      var saveBtn = q('[data-sys-save]');
      var statusEl = q('[data-sys-status]');
      var bannerEl = q('[data-sys-error-banner]');

      function setFieldError(key, msg) {
        var input = sysInputFor(key), box = q('[data-sys-error="' + key + '"]');
        if (input) input.classList.toggle('is-error', !!msg);
        if (box) { box.hidden = !msg; if (msg) box.textContent = msg; }
      }
      function refresh() {
        var next = sysReadForm();
        changed = [];
        SYS_TOGGLE_KEYS.forEach(function (k) {
          var el = q('[data-sys-key="' + k + '"]'); el = el && el.closest ? el.closest('.app-check') : null;
          var d = !!next[k] !== !!s[k];
          if (el) el.classList.toggle('is-dirty', d);
          if (d) changed.push(k);
        });
        SYS_FIELD_KEYS.forEach(function (k) {
          var el = q('[data-sys-field="' + k + '"]');
          var d = sysNorm(k, next[k]) !== sysNorm(k, s[k]);
          if (el) el.classList.toggle('is-dirty', d);
          if (d) changed.push(k);
        });
        if (statusEl) statusEl.innerHTML = saving ? '正在保存，请稍候…' : changed.length ? '有 <b>' + changed.length + '</b> 处未保存的更改' : (state.sysSettingsSavedAt ? '最近保存于 ' + auditTime(state.sysSettingsSavedAt) + ' · 配置已同步' : '与已保存配置一致');
        if (saveBtn) saveBtn.disabled = saving || !changed.length;
        window.__sysSettingsGuard = !!(changed.length || saving);
        var adv = q('[data-sys-advisory]'), advText = q('[data-sys-advisory-text]');
        if (adv && advText) {
          var msgs = [];
          if (next.register && !next.emailVerify) msgs.push('注册开放且邮箱验证关闭，新账号注册后即可发帖，建议开启邮箱验证');
          if (next.maintain) msgs.push('维护模式下前台访客将看到维护提示，仅管理员可操作');
          adv.hidden = !msgs.length;
          advText.textContent = msgs.join('；') + '。';
        }
        var notice = q('[data-sys-notice]');
        if (notice) notice.hidden = !next.maintain;
      }
      function onEdit() {
        if (saving) return;
        refresh();
        SYS_FIELD_KEYS.forEach(function (k) {
          var box = q('[data-sys-error="' + k + '"]');
          if (box && !box.hidden) setFieldError(k, sysValidate(sysReadForm())[k] || '');
        });
      }
      qa('[data-sys-name], [data-sys-lang], [data-sys-source], [data-sys-ratelimit]').forEach(function (el) { el.addEventListener('input', onEdit); el.addEventListener('change', onEdit); });
      qa('[data-sys-key]').forEach(function (el) { el.addEventListener('change', onEdit); });

      saveBtn.addEventListener('click', function () {
        if (saving || saveBtn.disabled) return;
        var next = sysReadForm();
        var errors = sysValidate(next);
        SYS_FIELD_KEYS.forEach(function (k) { setFieldError(k, errors[k] || ''); });
        if (Object.keys(errors).length) { toast('请先修正表单中的 ' + Object.keys(errors).length + ' 处错误'); var first = sysInputFor(Object.keys(errors)[0]); if (first) first.focus(); return; }
        saving = true;
        refresh();
        var oldLabel = saveBtn.textContent;
        saveBtn.setAttribute('aria-busy', 'true');
        saveBtn.textContent = '保存中…';
        window.setTimeout(function () {
          if (saveBtn.isConnected) { saveBtn.removeAttribute('aria-busy'); saveBtn.textContent = oldLabel; }
          saving = false;
          if (q('[data-sys-sim-fail]').checked) {
            if (bannerEl) {
              bannerEl.hidden = false;
              var bt = q('[data-sys-error-text]');
              if (bt) bt.textContent = '服务超时（演示模拟）。你的更改未丢失，可修改后重试。';
            }
            toast('保存失败 · 请重试');
            refresh();
            return;
          }
          var clean = { siteName: next.siteName, lang: next.lang, source: next.source, rateLimit: Number(next.rateLimit) };
          SYS_TOGGLE_KEYS.forEach(function (k) { clean[k] = next[k]; });
          var parts = SYS_TOGGLE_KEYS.concat(SYS_FIELD_KEYS).filter(function (k) { return sysNorm(k, clean[k]) !== sysNorm(k, s[k]); }).map(function (k) { return SYS_LABELS[k] + ' ' + sysHuman(k, s[k]) + ' → ' + sysHuman(k, clean[k]); });
          s = clean;
          state.sysSettings = clean;
          state.sysSettingsSavedAt = Date.now();
          saveState();
          logAudit('更新系统设置', parts.length ? parts.join('；') : '无字段变化');
          if (bannerEl) bannerEl.hidden = true;
          toast('设置已保存 · 已写入审计日志');
          refresh();
        }, 650);
      });

      q('[data-sys-defaults]').addEventListener('click', function () {
        confirmAdmin('恢复默认设置', '站点配置将重置为默认值（开放注册、邮箱验证开启、限流 30 次/分钟）。更改需点击“保存设置”后生效，并写入审计日志。', '恢复默认', function () {
          sysInputFor('siteName').value = SYS_DEFAULTS.siteName;
          sysInputFor('lang').value = SYS_DEFAULTS.lang;
          sysInputFor('source').value = SYS_DEFAULTS.source;
          sysInputFor('rateLimit').value = SYS_DEFAULTS.rateLimit;
          SYS_TOGGLE_KEYS.forEach(function (k) { q('[data-sys-key="' + k + '"]').checked = SYS_DEFAULTS[k]; });
          SYS_FIELD_KEYS.forEach(function (k) { setFieldError(k, ''); });
          if (bannerEl) bannerEl.hidden = true;
          refresh();
          toast('已恢复默认值 · 点击“保存设置”后生效');
        });
      });

      q('[data-sys-export]').addEventListener('click', function () {
        var next = sysReadForm();
        var out = { siteName: next.siteName, lang: next.lang, source: next.source, rateLimit: Number(next.rateLimit) };
        SYS_TOGGLE_KEYS.forEach(function (k) { out[k] = next[k]; });
        if (adminDownload('bblbb-settings.json', 'application/json', JSON.stringify(out, null, 2))) {
          logAudit('导出系统配置', 'JSON · ' + (changed.length ? '含未保存更改' : '当前已保存值'));
          toast('配置已导出');
        }
      });

      q('[data-sys-reset]').addEventListener('click', function () {
        confirmAdmin('重置演示数据', '将清除 localStorage 中的全部演示状态（登录、积分、草稿、后台变更）并刷新页面。', '确认重置', function () {
          try { localStorage.removeItem(APP_KEY); localStorage.removeItem('bblbb:mock-runtime:v1'); localStorage.removeItem('returnTo'); localStorage.removeItem('bblbb-theme'); sessionStorage.removeItem('returnTo'); } catch (e) {}
          location.reload();
        });
      });
      refresh();
    });
  }
  function renderArticleAudit(route) {
    var postId = route.indexOf(':') >= 0 ? route.split(':').slice(1).join(':') : 'p-201';
    var post = ADM_POSTS.filter(function (p) { return p.id === postId; })[0] || ADM_POSTS[0];
    var diff = '<div class="app-diff"><div class="app-diff__legend"><span class="is-added">新增</span><span class="is-removed">删除</span><span class="is-changed">变更</span></div><div class="app-diff__grid"><section><h3>修改前</h3><p class="app-diff__text app-diff__text--removed">这个月用 SvelteKit 写完 BBLBB 的核心页面，包括 SSR、SEO、CSRF 与表单接入。</p></section><section><h3>修改后</h3><p class="app-diff__text app-diff__text--added">这个月用 SvelteKit 写完 BBLBB 的核心页面，包括 SSR、SEO、CSRF、表单与 OIDC 接入，整体体验比想象中好。</p></section></div><div class="admin-action-row" style="margin-top:14px">' + button('通过审核', 'primary', 'data-adm-act="audit-approve" data-adm-id="' + esc(post.id) + '"') + button('填写驳回理由', 'danger', 'data-adm-act="audit-reject" data-adm-id="' + esc(post.id) + '"') + '</div></div>';
    adminShell('#admin-posts', '内容审核 Diff', '逐字段检查修改前后的内容，确认后再决定发布或驳回。', card('版本对比 · ' + post.title, diff), button('返回内容列表', 'ghost', 'data-audit-back'));
    q('[data-audit-back]').addEventListener('click', function () { go('#admin-posts'); });
    bindAdminActs(function (el, act, id) { var p = ADM_POSTS.filter(function (x) { return x.id === id; })[0] || post; if (act === 'audit-approve') { var m = state.postStatus || {}; m[p.id] = '公开'; state.postStatus = m; saveState(); logAudit('审核通过', p.title); toast('已通过审核 · 审计已记录'); } if (act === 'audit-reject') rejectPostModal(p, function (reason) { var r = state.postRejections || {}; r[p.id] = { reason: reason, at: Date.now() }; state.postRejections = r; var s = state.postStatus || {}; s[p.id] = '已隐藏'; state.postStatus = s; saveState(); logAudit('审核驳回', p.title + ' · ' + reason); toast('已驳回 · 审计已记录'); }); });
  }
  function renderAdmin(route) {
    hidePages();
    if (!requireAdmin(route)) return;
    setDocumentTitle(route === '#admin' ? '管理后台' : '管理后台 · ' + route.replace(/^#admin-?/, '').replace(/[:].*$/, ''));
    if (route === '#admin') renderAdminDashboard();
    else if (route === '#admin-reports' || route.indexOf('#admin-report:R-') === 0) renderReports(route);
    else if (route === '#admin-points') renderPoints();
    else if (route === '#admin-levels') renderLevels();
    else if (route === '#admin-achievements') renderAchievementsAdmin();
    else if (route === '#admin-themes') renderThemesAdmin();
    else if (route === '#admin-plugins') renderPluginsAdmin();
    else if (route === '#admin-oauth') renderOAuthAdmin();
    else if (route === '#admin-storage') renderStorageAdmin();
    else if (route === '#admin-download-billing') renderDownloadBilling();
    else if (route === '#admin-ai') renderAiAdmin();
    else if (route === '#admin-video') renderVideoAdmin();
    else if (route === '#admin-marketplace') renderMarketplaceAdmin();
    else if (route === '#admin-bi') renderBiAdmin();
    else if (route === '#admin-users' || route.indexOf('#admin-users:') === 0) renderUsersAdmin(route);
    else if (route === '#admin-roles') renderRolesAdmin();
    else if (route === '#admin-boards') renderBoardsAdmin();
    else if (route === '#admin-posts') renderPostsAdmin();
    else if (route.indexOf('#admin-article-audit') === 0) renderArticleAudit(route);
    else if (route === '#admin-tags') renderTagsAdmin();
    else if (route === '#admin-attachments') renderAttachmentsAdmin();
    else if (route === '#admin-notifications') renderNotificationsAdmin();
    else if (route === '#admin-audit') renderAuditAdmin();
    else if (route === '#admin-settings') renderSettingsAdmin();
    else renderGenericAdmin(route);
  }

  function renderAchievements() {
    mountTpl('achievements', 'achievements', function () {

    });
}
  function renderStatePage(kind) {
    var name = { '403': '403', '404': '404', '429': '429', error: 'error' }[kind] || 'error';
    mountTpl(name, name, function () {});
  }


  function setMutationState(button, pendingText, completeText, fn) { if (!button || button.disabled) return; var original = button.textContent; button.disabled = true; button.setAttribute('aria-busy', 'true'); button.textContent = pendingText || '处理中…'; window.setTimeout(function () { try { fn(); button.textContent = completeText || original; } finally { button.disabled = false; button.removeAttribute('aria-busy'); } }, 420); }
  function renderAuth(kind) {
    if (kind === 'login') {
      if (!localStorage.getItem('returnTo')) state.returnTo = '';
      var locked = state.lockedUntil && state.lockedUntil > Date.now();
      mountTpl('login', 'login', function () {
      var loginFormError = q('[data-auth-form-error]'); if (loginFormError) loginFormError.textContent = locked ? '尝试次数过多，请稍后再试。' : '';
      var loginSubmitBtn = q('[data-auth-login] .login-submit'); if (loginSubmitBtn) loginSubmitBtn.disabled = !!locked;

      q('[data-auth-login]').addEventListener('submit', function (event) { event.preventDefault(); var user = q('[data-auth-user]'); var password = q('[data-auth-password]'); var formError = q('[data-auth-form-error]'); q('[data-auth-error]').textContent = ''; q('[data-auth-password-error]').textContent = ''; formError.textContent = ''; if (!user.value.trim()) { q('[data-auth-error]').textContent = '请输入账号'; user.focus(); return; } if (!password.value) { q('[data-auth-password-error]').textContent = '请输入密码'; password.focus(); return; } if (state.lockedUntil && state.lockedUntil > Date.now()) { formError.textContent = '尝试次数过多，请稍后再试（429）'; return; } if (user.value.trim() !== 'admin' || password.value !== 'admin123') { state.loginFailures += 1; if (state.loginFailures >= 5) { state.lockedUntil = Date.now() + 10 * 60 * 1000; saveState(); formError.textContent = '尝试次数过多，账号已临时锁定 10 分钟（429）'; } else { saveState(); formError.textContent = '账号或密码错误，请重试'; } password.focus(); return; } state.auth = true; state.role = 'admin'; state.loginFailures = 0; state.lockedUntil = 0; var target = returnTarget(); state.returnTo = ''; saveState(); try { localStorage.removeItem('returnTo'); sessionStorage.removeItem('returnTo'); } catch (e) {} updateUserChrome(); go(target || '#home'); toast('登录成功，已返回原操作'); });
      });
    } else if (kind === 'register') {
      mountTpl('register', 'register', function () {

      q('[data-reg-terms]').addEventListener('change', function () { q('[data-reg-submit]').disabled = !this.checked; });
      q('[data-register]').addEventListener('submit', function (event) { event.preventDefault(); var ok = true; if (!q('[data-reg-user]').value.trim()) { q('[data-reg-user-error]').textContent = '请输入用户名'; ok = false; } if (!/^[^@]+@[^@]+\\.[^@]+$/.test(q('[data-reg-email]').value)) { q('[data-reg-email-error]').textContent = '请输入有效邮箱'; ok = false; } if (q('[data-reg-password]').value.length < 10) { q('[data-reg-password-error]').textContent = '密码至少 10 位'; ok = false; } if (!ok) return; var submit = q('[data-reg-submit]'); actionPending(submit, '创建中…', function () { state.pendingVerification = { username: q('[data-reg-user]').value.trim(), email: q('[data-reg-email]').value.trim(), createdAt: Date.now() }; saveState(); openModal('验证邮箱', '<p>注册成功。验证邮件已加入发送队列（原型演示）。</p><p class="app-muted">生产环境必须由真实 sender 投递并可追踪。</p>', '返回登录 ' + button('我知道了', 'primary', 'data-reg-done')); q('[data-reg-done]').addEventListener('click', function () { closeModal(); go('#login'); toast('注册成功，验证邮件已发送'); }); }); });
      });
    } else if (kind === 'forgot-password') {
      mountTpl('forgot-password', 'forgot-password', function () {

      q('[data-forgot]').addEventListener('submit', function (event) { event.preventDefault(); var input = q('[data-forgot-email]'); if (!/^[^@]+@[^@]+\\.[^@]+$/.test(input.value)) { q('[data-forgot-error]').textContent = '请输入有效邮箱'; input.focus(); return; } openModal('重置邮件已发送', '<p>为防止账号枚举，无论邮箱是否注册都会显示成功态。</p><p class="app-muted">链接 1 小时内有效；生产环境需由真实邮件 sender 投递。</p>', '返回登录 ' + button('返回登录', 'primary', 'data-forgot-done')); q('[data-forgot-done]').addEventListener('click', function () { closeModal(); go('#login'); }); });
      });
    }
  }

  function renderRoute(raw) {
    readState();
    var parsed = routeKey(raw);
    var base = parsed.base;
    var target = parsed.raw;
    var restricted = ['publish', 'favorites', 'settings', 'notifications', 'shop', 'achievements', 'billing', 'appeals', 'checkout', 'purchases', 'apikeys', 'mfa', 'messages', 'me'];
    if (restricted.indexOf(base) >= 0 && !state.auth) { requireAuth(target); return; }
    if (base.indexOf('admin') === 0) { renderAdmin(target); return; }
    if (base === 'home') { hidePages(); q('#page-home').hidden = false; enhanceHome(); setDocumentTitle('首页'); return; }
    if (base === 'discover') { hidePages(); var discoverPage = q('#page-discover'); discoverPage.hidden = false; enhanceDiscover(); setDocumentTitle('发现'); return; }
    if (base === 'loading') { hidePages(); q('#page-loading').hidden = false; setDocumentTitle('加载'); if (typeof window.startLoading === 'function') window.startLoading(); return; }
    if (base === 'design') { hidePages(); q('#page-design').hidden = false; setDocumentTitle('原型界面标准文档'); return; }
    if (base === 'login' || base === 'register' || base === 'forgot-password') { hidePages(); renderAuth(base); setDocumentTitle(base === 'login' ? '登录' : base === 'register' ? '注册' : '重置密码'); return; }
    if (base === 'articles') { hidePages(); renderArticles(); setDocumentTitle('内容'); return; }
    if (base === 'boards') { hidePages(); renderBoards(); setDocumentTitle('板块'); return; }
    if (base === 'board') { hidePages(); renderBoard(parsed.rest || 'rust'); setDocumentTitle('板块'); return; }
    if (base === 'tags') { hidePages(); renderTags(); setDocumentTitle('标签'); return; }
    if (base === 'tag') { hidePages(); renderTags(parsed.rest || 'Rust'); setDocumentTitle('标签'); return; }
    if (base === 'thread' || base === 'topic') { hidePages(); renderTopic(parsed.rest || '201'); return; }
    if (base === 'publish') { hidePages(); renderPublish(parsed.rest || 'article'); setDocumentTitle('发布内容'); return; }
    if (base === 'user') { hidePages(); renderUser(parsed.rest || 'Chaos', 'user'); setDocumentTitle('用户主页'); return; }
    if (base === 'me') { hidePages(); renderUser('Chaos', 'me'); setDocumentTitle('我的'); return; }
    if (base === 'favorites') { hidePages(); renderFavorites(); setDocumentTitle('我的收藏'); return; }
    if (base === 'settings') { hidePages(); renderSettings(parsed.rest || 'profile'); setDocumentTitle('账号设置'); return; }
    if (base === 'messages') { hidePages(); renderMessages(); setDocumentTitle('消息'); return; }
    if (base === 'search') { hidePages(); renderSearch(parsed.rest || ''); setDocumentTitle('搜索'); return; }
    if (base === 'notifications') { hidePages(); renderNotifications(); setDocumentTitle('通知中心'); return; }
    if (base === 'shop') { hidePages(); renderShop(); setDocumentTitle('商城与积分'); return; }
    if (base === 'billing') { hidePages(); renderBilling(); setDocumentTitle('下载账单'); return; }
    if (base === 'appeals') { hidePages(); renderAppeals(); setDocumentTitle('申诉中心'); return; }
    if (base === 'mfa') { hidePages(); renderMfa(); setDocumentTitle('两步验证'); return; }
    if (base === 'market') { hidePages(); renderMarket(); setDocumentTitle('应用市场'); return; }
    if (base === 'checkout') { hidePages(); renderCheckout(); setDocumentTitle('市场结算'); return; }
    if (base === 'purchases') { hidePages(); renderPurchases(); setDocumentTitle('市场购买记录'); return; }
    if (base === 'apikeys') { hidePages(); renderApiKeys(); setDocumentTitle('API 密钥'); return; }
    if (base === 'achievements') { hidePages(); renderAchievements(); setDocumentTitle('成就墙'); return; }
    if (base === 'drafts') { goDrafts(); return; }
    if (base === '403' || base === '404' || base === '429' || base === 'error') { hidePages(); renderStatePage(base); setDocumentTitle(base.toUpperCase()); return; }
    hidePages(); renderStatePage('404'); setDocumentTitle('页面不存在');
  }

  function allowedBase(base) {
    return ['home', 'loading', 'login', 'discover', 'messages', 'me', 'search', 'thread', 'notifications', 'shop', 'achievements', 'drafts', 'admin', 'design', 'articles', 'boards', 'board', 'tags', 'tag', 'topic', 'publish', 'user', 'favorites', 'settings', 'register', 'forgot-password', 'billing', 'appeals', 'market', 'checkout', 'purchases', 'apikeys', 'mfa', '403', '404', '429', 'error', 'admin-reports', 'admin-report', 'admin-article-audit', 'admin-points', 'admin-levels', 'admin-achievements', 'admin-themes', 'admin-plugins', 'admin-oauth', 'admin-storage', 'admin-download-billing', 'admin-ai', 'admin-video', 'admin-marketplace', 'admin-bi', 'admin-users', 'admin-roles', 'admin-boards', 'admin-posts', 'admin-tags', 'admin-attachments', 'admin-notifications', 'admin-audit', 'admin-settings'].indexOf(base) >= 0;
  }
  function route() {
    readState();
    closeDrawer();
    qa('[data-batch-bar]').forEach(function (bar) { bar.remove(); });
    TPL_TOKEN += 1; /* 使未完成的异步模板挂载过期 */
    if (profileCard) profileCard.classList.remove('is-visible');
    var raw = location.hash || '#home';
    var parsed = routeKey(raw);
    if (!allowedBase(parsed.base)) {
      if (location.hash !== '#404') history.replaceState(null, '', '#404');
      raw = '#404';
    }
    if (window.__sysSettingsGuard) {
      if (parsed.base === 'admin-settings') return; /* 重进本页时保留未保存的表单状态 */
      window.__sysSettingsGuard = false;
      openModal('未保存的更改', '<p>系统设置中有未保存的更改（或保存正在进行）。离开本页将放弃这些更改。</p>', button('留在本页', '', 'data-sys-stay') + ' ' + button('放弃并离开', 'danger', 'data-sys-leave'), function (backdrop) {
        var leave = function () { route(); };
        qa('[data-modal-close]', backdrop).forEach(function (b) { b.addEventListener('click', leave); });
        backdrop.addEventListener('click', function (event) { if (event.target === backdrop) leave(); });
        backdrop.addEventListener('keydown', function (event) { if (event.key === 'Escape') leave(); });
      });
      q('[data-sys-stay]').addEventListener('click', function () { window.__sysSettingsGuard = true; closeModal(); history.replaceState(null, '', '#admin-settings'); });
      q('[data-sys-leave]').addEventListener('click', function () { closeModal(); route(); });
      return;
    }
    updateUserChrome();
    renderRoute(raw);
    syncBellBadge();
    decorateProfileTriggers(document.querySelector('.page:not([hidden])') || document);
    window.scrollTo(0, 0);
    window.requestAnimationFrame(function () { var page = q('.page:not([hidden])'); var heading = page && q('h1', page); if (heading) { if (!heading.hasAttribute('tabindex')) heading.setAttribute('tabindex', '-1'); heading.focus({ preventScroll: true }); } });
  }
  function go(hash) { if (location.hash === hash) route(); else location.hash = hash; }
  window.addEventListener('beforeunload', function (event) { if (window.__sysSettingsGuard) { event.preventDefault(); event.returnValue = ''; } });

  window.route = route;
  window.go = go;
  window.isLoggedIn = isLoggedIn;
  window.openAppModal = openModal;
  window.closeAppModal = closeModal;
  window.openPublish = function () { go(state.auth ? '#publish:article' : '#login'); };
  window.startLoading = startLoading;
  window.like = like;
  window.pickCat = legacyPickCat;
  window.pickFilter = legacyPickFilter;
  window.cycleSort = legacyCycleSort;
  window.shuffleDiscover = legacyShuffleDiscover;
  window.filterTag = legacyFilterTag;
  window.loadMore = legacyLoadMore;
  window.toggleDrop = function (event, id) { if (event) { event.preventDefault(); event.stopPropagation(); } var menu = q('#' + id); if (!menu) return; var isOpen = menu.classList.toggle('open'); var trigger = event && event.currentTarget; if (trigger && trigger.setAttribute) trigger.setAttribute('aria-expanded', String(isOpen)); };
  /* 下拉菜单：点击外部或按 Escape 关闭（原关闭逻辑在 legacy 死代码中，补回 active 运行时） */
  document.addEventListener('click', function (event) {
    if (event.target.closest && event.target.closest('.drop-wrap')) return;
    closeDrops();
  });
  document.addEventListener('keydown', function (event) { if (event.key === 'Escape') closeDrops(); });
  window.closeDrops = closeDrops;
  window.markAllRead = markAllRead;
  window.switchTab = switchTab;
  window.logout = logout;
  window.replySubmit = legacyReplySubmit;
  window.unlockRestricted = legacyUnlockRestricted;
  window.copyCode = legacyCopyCode;
  window.openConv = legacyOpenConv;
  window.addBubble = legacyAddBubble;
  window.sendMsg = legacySendMsg;
  window.updateThreadDetail = legacyUpdateThreadDetail;
  window.renderRoute = renderRoute;
  window.globalSearch = function (_input, value) { var query = String(value || '').trim(); var target = '#search:' + encodeURIComponent(query); if (location.hash === target) route(); else location.hash = target; };
  window.toggleTheme = function () { var root = document.documentElement; var next = root.dataset.theme === 'dark' ? 'light' : 'dark'; root.dataset.theme = next; root.dataset.themePreference = next; root.style.colorScheme = next; var meta = q('#theme-color'); if (meta) meta.setAttribute('content', next === 'dark' ? '#0D0D0D' : '#F5F3EE'); try { localStorage.setItem('bblbb-theme', next); } catch (e) {} toast(next === 'dark' ? '已切换为暗色主题' : '已切换为亮色主题'); };
  window.addEventListener('hashchange', route);
  var headerSearch = document.getElementById('header-search-form');
  if (headerSearch) headerSearch.addEventListener('submit', function (event) { event.preventDefault(); window.globalSearch(null, (q('#header-search-input') || {}).value || ''); });
  var pageSearch = document.querySelector('[data-legacy-search-form]');
  if (pageSearch) pageSearch.addEventListener('submit', function (event) { event.preventDefault(); window.globalSearch(null, (q('#search-query') || {}).value || ''); });
  document.addEventListener('pointerover', function (event) {
    var trigger = event.target.closest('.profile-hover-trigger');
    if (trigger && trigger.offsetParent === null) return;
    if (trigger) showProfileCard(trigger);
  });
  document.addEventListener('pointerout', function (event) {
    var trigger = event.target.closest('.profile-hover-trigger');
    if (trigger && !trigger.contains(event.relatedTarget)) scheduleProfileClose();
  });
  document.addEventListener('focusin', function (event) {
    var trigger = event.target.closest('.profile-hover-trigger');
    if (trigger && trigger.offsetParent !== null) showProfileCard(trigger);
  });
  document.addEventListener('focusout', function (event) {
    var trigger = event.target.closest('.profile-hover-trigger');
    if (trigger && !trigger.contains(event.relatedTarget)) scheduleProfileClose();
  });
  document.addEventListener('keydown', function (event) {
    if (event.key === 'Escape' && profileCard) { profileCard.classList.remove('is-visible'); activeProfileTrigger = null; }
    if ((event.key === 'Enter' || event.key === ' ') && event.target && event.target.classList && event.target.classList.contains('profile-hover-trigger')) {
      event.preventDefault();
      var name = event.target.getAttribute('data-profile-name');
      if (name) go('#user:' + encodeURIComponent(name));
    }
  });
  window.addEventListener('scroll', function () { if (profileCard) profileCard.classList.remove('is-visible'); }, true);
  window.addEventListener('resize', function () { if (profileCard) profileCard.classList.remove('is-visible'); });
  document.addEventListener('click', function (event) {
    var target = event.target.closest('[data-go-home]'); if (target) { event.preventDefault(); go('#home'); return; }
    target = event.target.closest('[data-go-search]'); if (target) { event.preventDefault(); go('#search'); return; }
    target = event.target.closest('[data-go-publish]'); if (target) { event.preventDefault(); go('#publish:' + target.getAttribute('data-go-publish')); return; }
    target = event.target.closest('[data-route-retry]'); if (target) { event.preventDefault(); route(); return; }
    target = event.target.closest('[data-copy-share]'); if (target) { var input = target.parentElement && target.parentElement.querySelector('input'); if (input) { input.select(); if (navigator.clipboard && navigator.clipboard.writeText) { navigator.clipboard.writeText(input.value).then(function () { toast('已复制到剪贴板'); }, function () { toast('无法访问剪贴板，请按 Ctrl/Cmd + C 复制'); }); } else { toast('请按 Ctrl/Cmd + C 复制'); } } return; }
  });
  readState();
  ensurePages();
  bindLegacyChrome();
  decorateProfileTriggers(document);
  window.setTimeout(route, 0);
}());
