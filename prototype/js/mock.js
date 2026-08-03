// ============================================
// BBLBB Mock Data
// ============================================

window.MockData = (function() {
  // Current User: Chaos (LV.6, Rust mod)
  const currentUser = {
    name: 'Chaos',
    bio: 'Rust 爱好者，喜欢折腾自托管服务',
    level: 6,
    exp: 2680,
    expNext: 3000,
    coins: 328,
    contribution: 146,
    roles: ['member', 'moderator'],
    modBoards: ['rust'],
    joinedAt: '2024-03-15',
    lastActive: '2026-08-02 20:30'
  };

  // Users
  const users = {
    Chaos: currentUser,
    Alice: {
      name: 'Alice',
      bio: '前端开发者',
      level: 4,
      exp: 890,
      expNext: 1500,
      coins: 56,
      contribution: 23,
      roles: ['member'],
      modBoards: [],
      joinedAt: '2025-01-20',
      lastActive: '2026-08-02 19:00'
    },
    Bob: {
      name: 'Bob',
      bio: '运维工程师',
      level: 5,
      exp: 1820,
      expNext: 3000,
      coins: 180,
      contribution: 67,
      roles: ['member'],
      modBoards: [],
      joinedAt: '2024-08-10',
      lastActive: '2026-08-02 18:45'
    },
    Echo: {
      name: 'Echo',
      bio: '社区管理员',
      level: 8,
      exp: 8500,
      expNext: 12000,
      coins: 1200,
      contribution: 520,
      roles: ['admin'],
      modBoards: [],
      joinedAt: '2023-06-01',
      lastActive: '2026-08-02 21:00'
    }
  };

  // Boards (6)
  const boards = [
    {
      slug: 'tech-essay',
      color: '#0088CC',
      name: '技术随笔',
      description: '技术思考与经验分享',
      icon: 'book-open',
      postCount: 1256,
      todayCount: 12,
      mods: ['Echo'],
      rules: ['原创优先', '技术相关', '禁止广告']
    },
    {
      slug: 'rust',
      color: '#B85C38',
      name: 'Rust',
      description: 'Rust 语言学习与实践',
      icon: 'cog',
      postCount: 892,
      todayCount: 8,
      mods: ['Chaos', 'Echo'],
      rules: ['提问前先搜索', '贴代码用代码块', '友善讨论']
    },
    {
      slug: 'web-dev',
      color: '#12A89D',
      name: 'Web 开发',
      description: '前端、后端、全栈 Web 技术',
      icon: 'globe',
      postCount: 2103,
      todayCount: 25,
      mods: ['Alice'],
      rules: ['框架讨论请加标签', '求助贴提供复现']
    },
    {
      slug: 'opensource',
      color: '#652D90',
      name: '开源项目',
      description: '开源项目分享与协作',
      icon: 'git-branch',
      postCount: 567,
      todayCount: 5,
      mods: ['Bob'],
      rules: ['项目需开源', '禁止纯推广']
    },
    {
      slug: 'chat',
      color: '#F1592A',
      name: '闲聊',
      description: '非技术话题随便聊',
      icon: 'message-circle',
      postCount: 3421,
      todayCount: 42,
      mods: ['Echo'],
      rules: ['遵守社区规则', '禁止人身攻击']
    },
    {
      slug: 'meta',
      color: '#808281',
      name: '站务',
      description: '社区建设、意见反馈、Bug 报告',
      icon: 'settings',
      postCount: 234,
      todayCount: 2,
      mods: ['Echo'],
      rules: ['反馈请详细描述', '功能建议先搜索']
    }
  ];

  // Tags (6)
  const tags = [
    { name: 'Rust', count: 456 },
    { name: 'SvelteKit', count: 128 },
    { name: 'SQLite', count: 89 },
    { name: 'OAuth', count: 67 },
    { name: '自托管', count: 234 },
    { name: '性能优化', count: 156 }
  ];

  // Posts content
  const article101Content = `
## 背景

在资源受限的嵌入式设备上运行 SQLite，并发写入一直是个棘手的问题。本文分享我们在小机器上的实践经验。

## WAL 模式的优势

SQLite 的 WAL（Write-Ahead Logging）模式是并发读写的基础：

- 读操作不会阻塞写操作
- 写操作之间串行，但读可以并行
- 性能在大多数场景下优于默认的 DELETE 模式

\`\`\`sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
\`\`\`

## 并发控制策略

### 1. 忙处理回调

在多线程环境下，设置合理的 busy_timeout：

\`\`\`rust
let db = Connection::open("data.db")?;
db.pragma_update(None, "busy_timeout", &5000)?;
\`\`\`

### 2. 连接池管理

使用 r2d2 连接池，控制最大连接数：

- 读连接：CPU 核数 × 2
- 写连接：1 个（WAL 模式下写串行）

### 3. 批量写入

将多次写入合并到一个事务中，大幅提升吞吐量：

\`\`\`rust
db.execute("BEGIN IMMEDIATE", [])?;
for item in items {
    db.execute("INSERT INTO ...", [...])?;
}
db.execute("COMMIT", [])?;
\`\`\`

## 性能测试结果

在 Raspberry Pi 4 上的测试数据：

| 模式 | 单线程写入 | 8 线程读取 |
|------|-----------|-----------|
| DELETE | 120/s | 850/s |
| WAL | 180/s | 2400/s |

> WAL 模式下读取性能提升接近 3 倍，这在以读为主的场景下非常显著。

## 总结

- 小机器上优先使用 WAL 模式
- 合理设置 busy_timeout 避免 SQLITE_BUSY
- 批量写入是提升吞吐量的关键
- 连接池大小需要根据实际场景调优
`;

  const restrictedLevelContent = `
## 进阶内容：SQLite 调优实战

这部分是给高级用户的深度内容，包含一些生产环境的调优技巧。

### 内存映射 I/O

\`\`\`sql
PRAGMA mmap_size = 268435456; -- 256MB
\`\`\`

在内存足够的设备上，启用 mmap 可以显著提升读取性能。

### 页面大小优化

根据存储介质调整 page_size：
- SSD：4096（默认）
- 传统 HDD：8192 或更大

### WAL 检查点策略

\`\`\`sql
PRAGMA wal_autocheckpoint = 1000;
\`\`\`

调整自动检查点的阈值，平衡写入延迟和 WAL 文件大小。
`;

  const topic201Content = `最近在做一个个人项目，需要同时支持博客和轻量论坛功能。试过几个方案：

1. **纯静态博客 + Giscus** — 评论用 GitHub Discussions，但论坛感不强
2. **Discourse** — 功能太强了，部署重，插件体系复杂
3. **Flarum** — PHP 的，不太想维护
4. **自己用 SvelteKit 写** — 灵活，但工作量大

现在倾向于方案 4，用 SvelteKit + SQLite 做一个轻量的：

- 文章 + 评论
- 板块 + 讨论
- 用户系统（OAuth + 本地）
- 积分 / 等级

想问问大家的看法：
- 有没有必要上 SvelteKit 5？
- SQLite 够用吗？大概几千用户量级
- 有没有类似的开源项目可以参考？

先谢过各位！`;

  const restrictedReplyContent = `
## 补充：回复可见内容

感谢大家的讨论！这里补充一些我后续的调研结果：

### SvelteKit 5 的选择

目前 SvelteKit 5 已经比较稳定，新特性如 Runes 和 简化的 load 函数确实提升了开发体验。对于新项目，建议直接上 5.0。

### SQLite 的性能

几千用户量级完全没问题，配合 WAL 模式和适当的索引，读性能非常好。写操作如果不是特别频繁也够用。

### 参考项目

推荐几个可以参考的开源项目：
- **Lemmy** — Rust 写的联邦社区
- **Flarum** — PHP 的轻论坛，UI 设计不错
- **Discourse** — 虽然重，但功能设计很成熟
`;

  const topic202Content = `在做一个 SaaS 产品，需要 OAuth 2.0 / OIDC Provider。

目前有几个选择：

### 方案一：完全自研
- 灵活度最高
- 但安全坑太多，RFC 细节记不住
- 维护成本高

### 方案二：用 Keycloak / Authelia
- 成熟稳定
- 部署重，资源占用大
- 定制化麻烦

### 方案三：基于库自研到 80%
- 比如用 node-oidc-provider 这种库
- 核心协议交给库，UI 和业务逻辑自己写
- 平衡灵活度和安全性

想问问大家：
1. 你们的产品 OAuth 是自研还是用现成的？
2. 自研的话，哪些部分最容易踩坑？
3. 有没有推荐的 Rust 生态的库？

欢迎分享经验！`;

  const restrictedPaidContent = `
## 付费内容：OAuth 安全最佳实践清单

### 1. 始终使用 PKCE

即使是 Confidential Client，也建议启用 PKCE。Authorization Code + PKCE 是当前最安全的授权方式。

### 2. 严格校验 Redirect URI

- 精确匹配，不允许通配符
- 只允许 HTTPS（localhost 除外）
- 不允许 fragment

### 3. State 参数不可省略

State 用于防止 CSRF，必须：
- 加密安全的随机值
- 绑定到用户会话
- 一次性使用

### 4. Token 安全

- Access Token：短生命周期（15-30 分钟）
- Refresh Token：旋转（Rotation）+ 重用检测
- 所有 Token 都应在服务端存储，不下发到前端

### 5. Scope 最小化原则

只请求必要的 scope，客户端申请的 scope 需要用户确认。

### 6. 安全事件监控

- 异常登录地点
- 短时间多次授权失败
- Token 异常刷新模式

### 7. 定期安全审计

- 检查未使用的客户端
- 清理过期的 refresh token
- 审计管理员操作日志
`;

  // Posts
  const posts = [
    {
      id: 101,
      type: 'article',
      title: 'Rust 小机器上的 SQLite 并发实践',
      summary: '在资源受限的嵌入式设备上运行 SQLite，并发写入一直是个棘手的问题。本文分享 WAL 模式、连接池管理、批量写入等实践经验。',
      content: article101Content,
      author: 'Chaos',
      board: 'tech-essay',
      tags: ['Rust', 'SQLite', '性能优化'],
      views: 3421,
      replies: 23,
      likes: 156,
      createdAt: '2026-07-28 14:30',
      lastReplyBy: 'Alice',
      lastReplyAt: '3 天前',
      isPinned: false,
      updatedAt: '2026-07-30 09:15',
      isEssence: true,
      status: 'published',
      restricted: {
        type: 'level',
        level: 5,
        content: restrictedLevelContent,
        unlocked: false
      }
    },
    {
      id: 201,
      type: 'topic',
      title: '使用 SvelteKit 构建博客与轻量论坛是否合理？',
      summary: '最近在做一个个人项目，需要同时支持博客和轻量论坛功能。试过几个方案，想问问大家对 SvelteKit + SQLite 这个组合的看法。',
      content: topic201Content,
      author: 'Alice',
      board: 'web-dev',
      tags: ['SvelteKit', '自托管'],
      views: 1256,
      replies: 18,
      likes: 42,
      createdAt: '2026-08-01 10:20',
      lastReplyBy: 'Chaos',
      lastReplyAt: '2 小时前',
      isPinned: false,
      updatedAt: '2026-08-02 16:45',
      status: 'published',
      restricted: {
        type: 'reply',
        content: restrictedReplyContent,
        unlocked: false
      }
    },
    {
      id: 202,
      type: 'topic',
      title: 'OAuth Provider 应该自研到什么程度？',
      summary: '在做一个 SaaS 产品，需要 OAuth 2.0 / OIDC Provider。完全自研、用 Keycloak、还是基于库自研到 80%？想听听大家的经验。',
      content: topic202Content,
      author: 'Bob',
      board: 'opensource',
      tags: ['OAuth', '自托管'],
      views: 892,
      replies: 12,
      likes: 28,
      createdAt: '2026-07-30 21:00',
      lastReplyBy: 'Echo',
      lastReplyAt: '1 天前',
      isPinned: false,
      updatedAt: '2026-08-01 08:30',
      status: 'published',
      restricted: {
        type: 'paid',
        price: 10,
        content: restrictedPaidContent,
        unlocked: false
      }
    },
    {
      id: 203,
      type: 'topic',
      title: '分享一个自己写的 Rust 命令行工具',
      summary: '用 Rust 写了一个日志分析工具，处理速度比 awk 快 5 倍，开源了欢迎 Star。',
      content: '这是一个用 Rust 写的日志分析工具...',
      author: 'Chaos',
      board: 'rust',
      tags: ['Rust', '性能优化'],
      views: 567,
      replies: 8,
      likes: 34,
      createdAt: '2026-08-02 09:00',
      lastReplyBy: 'Bob',
      lastReplyAt: '5 小时前',
      isPinned: true,
      updatedAt: '2026-08-02 15:20',
      status: 'published'
    },
    {
      id: 204,
      type: 'topic',
      title: '自托管 VPS 选择讨论 — 哪家性价比最高？',
      summary: '想换个 VPS 来自托管服务，目前看了 Hetzner、DigitalOcean、Vultr 几家，想问问大家都用哪家？',
      content: '最近想把一些服务从云服务商迁到自己的 VPS...',
      author: 'Bob',
      board: 'chat',
      tags: ['自托管'],
      views: 789,
      replies: 32,
      likes: 15,
      createdAt: '2026-08-01 14:00',
      lastReplyBy: 'Alice',
      lastReplyAt: '1 天前',
      isPinned: false,
      updatedAt: '2026-08-02 18:30',
      status: 'published'
    },
    {
      id: 205,
      type: 'topic',
      title: '建议增加暗黑模式',
      summary: '现在的亮色模式晚上看有点刺眼，建议增加暗黑模式切换功能。',
      content: '晚上刷社区的时候，白色背景太亮了...',
      author: 'Alice',
      board: 'meta',
      tags: [],
      views: 234,
      replies: 5,
      likes: 45,
      createdAt: '2026-07-25 11:00',
      lastReplyBy: 'Echo',
      lastReplyAt: '8 天前',
      isPinned: false,
      updatedAt: '2026-07-28 09:00',
      status: 'published'
    },
    {
      id: 206,
      type: 'article',
      title: '从零搭建自托管监控系统',
      summary: '用 Prometheus + Grafana + Alertmanager 搭建一套完整的自托管监控方案，包含完整的配置示例。',
      content: '监控是自托管的重要组成部分...',
      author: 'Bob',
      board: 'tech-essay',
      tags: ['自托管', '性能优化'],
      views: 1890,
      replies: 15,
      likes: 89,
      createdAt: '2026-07-20 16:00',
      lastReplyBy: 'Chaos',
      lastReplyAt: '13 天前',
      isPinned: false,
      updatedAt: '2026-07-25 10:00',
      isEssence: true,
      status: 'published'
    }
  ];

  // Replies
  const replies = {
    201: [
      {
        id: 1,
        topicId: 201,
        floor: 1,
        author: 'Chaos',
        content: '推荐 SvelteKit + SQLite，我自己的博客就是这么搭的。\n\n几千用户完全没问题，SQLite 的性能比你想象的强得多。配合 WAL 模式，读性能非常好。\n\nSvelteKit 5 的 Runes 确实香，新项目建议直接上。',
        likes: 12,
        createdAt: '2026-08-01 11:00',
        isAuthor: false
      },
      {
        id: 2,
        topicId: 201,
        floor: 2,
        author: 'Bob',
        content: '建议看看 Lemmy 的架构，虽然是 Rust 写的，但设计思路可以参考。\n\n另外如果需要 OAuth，建议用现成的库，别自己造轮子，安全坑太多了。',
        likes: 8,
        createdAt: '2026-08-01 12:30',
        isAuthor: false
      },
      {
        id: 3,
        topicId: 201,
        floor: 3,
        author: 'Alice',
        content: '感谢两位的建议！\n\n决定用 SvelteKit 5 + SQLite 了，OAuth 部分打算用 oauth2-proxy 先顶着，后续再考虑自研到什么程度。',
        likes: 5,
        createdAt: '2026-08-01 14:00',
        isAuthor: true
      }
    ],
    202: [
      {
        id: 4,
        topicId: 202,
        floor: 1,
        author: 'Chaos',
        content: '个人经验：基于库自研到 80% 是比较好的平衡。\n\n核心协议（token 生成、校验、加密）交给成熟的库，UI、业务逻辑、权限控制自己写。这样既安全又灵活。',
        likes: 15,
        createdAt: '2026-07-30 22:00',
        isAuthor: false
      },
      {
        id: 5,
        topicId: 202,
        floor: 2,
        author: 'Echo',
        content: '补充几个容易踩的坑：\n\n1. Redirect URI 必须精确匹配，不能偷懒用通配符\n2. PKCE 一定要开，不管是 public 还是 confidential client\n3. Refresh token rotation + reuse detection 是标配\n4. State 参数不能省，防 CSRF',
        likes: 28,
        createdAt: '2026-07-31 08:00',
        isAuthor: false
      }
    ],
    101: [
      {
        id: 6,
        topicId: 101,
        floor: 1,
        author: 'Bob',
        content: '干货！正好在做类似的项目。\n\n请教一下，mmap_size 一般设多大比较合适？设备内存 512MB 的话。',
        likes: 3,
        createdAt: '2026-07-29 10:00',
        isAuthor: false
      }
    ]
  };

  // Notifications
  const notifications = [
    {
      id: 1,
      type: 'reply',
      content: 'Chaos 回复了你的帖子《使用 SvelteKit 构建博客与轻量论坛是否合理？》',
      source: 'SvelteKit 构建博客',
      sourceUrl: '#/topics/201',
      time: '2026-08-01 11:00',
      read: false
    },
    {
      id: 2,
      type: 'like',
      content: 'Echo 赞了你的文章《Rust 小机器上的 SQLite 并发实践》',
      source: 'SQLite 并发实践',
      sourceUrl: '#/topics/101',
      time: '2026-07-30 09:00',
      read: false
    },
    {
      id: 3,
      type: 'system',
      content: '你的等级提升到 LV.6，解锁更多权限',
      source: '系统通知',
      sourceUrl: '#/users/Chaos',
      time: '2026-07-28 16:00',
      read: true
    },
    {
      id: 4,
      type: 'mention',
      content: 'Bob 在帖子中 @了你',
      source: '自托管 VPS 选择',
      sourceUrl: '#/topics/204',
      time: '2026-08-02 10:00',
      read: true
    }
  ];

  // Reports
  const reports = [
    {
      id: 'R-1024',
      reason: '广告 / 垃圾信息',
      priority: 'high',
      status: 'pending',
      reporter: 'Chaos',
      reportedUser: 'Alice',
      board: 'rust',
      content: '出一个 Rust 培训课程，原价 2999，现在只要 999，加微信 xxx...',
      contentUrl: '#/topics/203',
      createdAt: '2026-08-02 15:30',
      assignee: 'Echo',
      evidence: '用户在多个板块发布相同内容，疑似广告账号',
      history: [
        {
          id: 1,
          type: 'report',
          operator: 'Chaos',
          reason: '发布广告内容',
          time: '2026-08-02 15:30'
        }
      ]
    },
    {
      id: 'R-1023',
      reason: '人身攻击',
      priority: 'medium',
      status: 'processing',
      reporter: 'Bob',
      reportedUser: 'Alice',
      board: 'chat',
      content: '你懂个屁，不懂别瞎说...',
      contentUrl: '#/topics/204',
      createdAt: '2026-08-01 20:00',
      assignee: 'Echo',
      history: [
        { id: 1, type: 'report', operator: 'Bob', reason: '人身攻击', time: '2026-08-01 20:00' },
        { id: 2, type: 'accept', operator: 'Echo', reason: '已受理', time: '2026-08-01 20:30' }
      ]
    },
    {
      id: 'R-1022',
      reason: '内容违规',
      priority: 'low',
      status: 'resolved',
      reporter: 'Alice',
      reportedUser: 'Bob',
      board: 'tech-essay',
      content: '文章内容质量太低...',
      contentUrl: '#/topics/206',
      createdAt: '2026-07-28 10:00',
      assignee: 'Echo',
      history: [
        { id: 1, type: 'report', operator: 'Alice', reason: '水帖', time: '2026-07-28 10:00' },
        { id: 2, type: 'accept', operator: 'Echo', reason: '已受理', time: '2026-07-28 11:00' },
        { id: 3, type: 'warn', operator: 'Echo', reason: '内容质量不达标，警告一次', time: '2026-07-28 12:00' }
      ]
    },
    {
      id: 'R-1021',
      reason: '重复发帖',
      priority: 'low',
      status: 'rejected',
      reporter: 'Chaos',
      reportedUser: 'Bob',
      board: 'rust',
      content: '同一个问题发了三遍...',
      contentUrl: '#/topics/203',
      createdAt: '2026-07-25 14:00',
      assignee: 'Echo',
      history: [
        { id: 1, type: 'report', operator: 'Chaos', reason: '重复发帖', time: '2026-07-25 14:00' },
        { id: 2, type: 'report', operator: 'Echo', reason: '经核实内容不重复，驳回举报', time: '2026-07-25 15:00' }
      ]
    }
  ];

  // Levels
  const levels = [
    { level: 1, name: 'LV.1 新手上路', color: '#919191', expRequired: 0, benefits: ['发帖', '回复'], userCount: 1250 },
    { level: 2, name: 'LV.2 初窥门径', color: '#919191', expRequired: 100, benefits: ['发帖', '回复', '收藏'], userCount: 890 },
    { level: 3, name: 'LV.3 略有小成', color: '#919191', expRequired: 500, benefits: ['发帖', '回复', '收藏', '私信'], userCount: 560 },
    { level: 4, name: 'LV.4 渐入佳境', color: '#0088CC', expRequired: 1000, benefits: ['发帖', '回复', '收藏', '私信', '签名档'], userCount: 340 },
    { level: 5, name: 'LV.5 炉火纯青', color: '#0088CC', expRequired: 1500, benefits: ['发帖', '回复', '收藏', '私信', '签名档', '付费内容'], userCount: 180 },
    { level: 6, name: 'LV.6 登峰造极', color: '#E9A100', expRequired: 3000, benefits: ['发帖', '回复', '收藏', '私信', '签名档', '付费内容', '自定义头衔'], userCount: 85 },
    { level: 7, name: 'LV.7 出神入化', color: '#E9A100', expRequired: 5000, benefits: ['全部 LV.6 权益', '版主申请资格', '专属徽章'], userCount: 32 },
    { level: 8, name: 'LV.8 一代宗师', color: '#E45735', expRequired: 8000, benefits: ['全部 LV.7 权益', '邀请码', '年度礼物'], userCount: 12 },
    { level: 9, name: 'LV.9 天人合一', color: '#E45735', expRequired: 15000, benefits: ['全部 LV.8 权益', '终身会员', '社区顾问'], userCount: 3 },
    { level: 10, name: 'LV.10 返璞归真', color: '#E45735', expRequired: 30000, benefits: ['全部权益', '传说级徽章', '创始人面对面'], userCount: 0 }
  ];

  // Plugins
  const plugins = [
    {
      id: 'p1',
      name: 'Markdown 增强',
      version: '1.2.0',
      type: 'config',
      status: 'enabled',
      capabilities: ['post_render', 'editor_toolbar'],
      description: '为 Markdown 编辑器增加数学公式、流程图、脚注等扩展语法支持'
    },
    {
      id: 'p2',
      name: 'SEO 优化',
      version: '2.0.1',
      type: 'config',
      status: 'enabled',
      capabilities: ['meta_tags', 'sitemap', 'structured_data'],
      description: '自动生成 SEO 友好的 meta 标签、站点地图和结构化数据'
    },
    {
      id: 'p3',
      name: '实时通知',
      version: '0.9.0',
      type: 'precompiled',
      status: 'error',
      capabilities: ['websocket', 'push_notification'],
      description: '基于 WebSocket 的实时通知推送插件'
    },
    {
      id: 'p4',
      name: '数据统计',
      version: '1.5.0',
      type: 'precompiled',
      status: 'disabled',
      capabilities: ['analytics', 'dashboard_widget'],
      description: '社区数据统计与可视化报表'
    }
  ];

  // OAuth Clients
  const oauthClients = [
    {
      id: 'oc1',
      name: 'BBLBB Mobile',
      clientId: 'bblbb-mobile-app',
      type: 'public',
      status: 'enabled',
      redirectUri: 'bblbb://auth/callback',
      postLogoutRedirectUri: 'bblbb://auth/logout',
      scopes: ['openid', 'profile', 'email'],
      recentAuthUsers: 256,
      createdAt: '2026-01-15'
    },
    {
      id: 'oc2',
      name: 'Legacy Admin Panel',
      clientId: 'legacy-admin-panel',
      type: 'confidential',
      status: 'disabled',
      redirectUri: 'https://admin-old.bblbb.com/callback',
      postLogoutRedirectUri: 'https://admin-old.bblbb.com/logout',
      scopes: ['openid', 'profile', 'email', 'admin:read'],
      recentAuthUsers: 0,
      createdAt: '2025-06-20'
    }
  ];

  // Login Devices
  const loginDevices = [
    {
      id: 'd1',
      name: '当前设备',
      os: 'Linux',
      browser: 'Chrome 126',
      ip: '192.168.1.100',
      location: '福建 厦门',
      lastActive: '刚刚',
      isCurrent: true
    },
    {
      id: 'd2',
      name: 'MacBook Pro',
      os: 'macOS 14',
      browser: 'Safari 17',
      ip: '10.0.0.50',
      location: '福建 厦门',
      lastActive: '2 小时前',
      isCurrent: false
    },
    {
      id: 'd3',
      name: 'iPhone 15',
      os: 'iOS 17',
      browser: 'Safari Mobile',
      ip: '172.20.10.5',
      location: '福建 厦门',
      lastActive: '昨天',
      isCurrent: false
    }
  ];

  // Admin Dashboard Stats
  const adminStats = {
    totalUsers: 3256,
    todayNewUsers: 23,
    totalPosts: 8942,
    pendingPosts: 12,
    totalReplies: 45678,
    pendingReports: 3,
    storageUsed: 2.4,
    storageTotal: 10
  };

  // User Moderation History
  const userModerationHistory = [
    {
      id: 1,
      type: 'warn',
      operator: 'Echo',
      reason: '发布低质量内容',
      time: '2026-07-15 10:00'
    }
  ];

  return {
    currentUser,
    users,
    boards,
    tags,
    posts,
    replies,
    notifications,
    reports,
    levels,
    plugins,
    oauthClients,
    loginDevices,
    adminStats,
    userModerationHistory,
    favoritePostIds: [],
    // Helper functions
    getBoard(slug) { return boards.find(b => b.slug === slug); },
    getPost(id) { return posts.find(p => p.id === id); },
    getPostsByBoard(slug) { return posts.filter(p => p.board === slug); },
    getPostsByTag(tag) { return posts.filter(p => p.tags.includes(tag)); },
    getArticles() { return posts.filter(p => p.type === 'article'); },
    getReplies(topicId) { return replies[topicId] || []; },
    getUser(name) { return users[name]; }
  };
})();
