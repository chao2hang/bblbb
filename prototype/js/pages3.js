// ============================================
// BBLBB Page Renderers — Part 3: Admin Pages
// ============================================

(function() {
  const C = Components;
  const P = Pages;

  // Helper: admin layout wrapper
  function adminLayout(content, path) {
    return `
      ${C.navbar(path)}
      <div class="container">
        <div class="page-content admin-layout">
          ${C.adminSidebar(path)}
          <main class="admin-main">
            ${content}
          </main>
        </div>
      </div>
    `;
  }

  // ============================================
  // Admin Dashboard (/admin)
  // ============================================
  P.adminDashboard = function() {
    const stats = MockData.adminStats;
    const statCards = [
      { label: '总用户数', value: stats.totalUsers.toLocaleString(), change: `+${stats.todayNewUsers} 今日新增`, icon: 'users' },
      { label: '总帖子数', value: stats.totalPosts.toLocaleString(), change: `${stats.pendingPosts} 待审核`, icon: 'file-text', negative: false },
      { label: '总回复数', value: stats.totalReplies.toLocaleString(), change: '较昨日 +156', icon: 'message-square' },
      { label: '待处理举报', value: stats.pendingReports, change: '需要处理', icon: 'flag', negative: true }
    ];

    const recentReports = Store.state.reports.slice(0, 3);
    const recentUsers = Object.values(MockData.users).slice(0, 5);

    const content = `
      <div class="admin-dashboard">
      <div class="admin-page-header admin-dashboard-header">
        <div>
          <h1 class="admin-page-title">仪表盘</h1>
          <p class="admin-page-desc">社区运营数据概览</p>
        </div>
        <div style="display: flex; gap: var(--space-2);">
          ${C.button({ text: Store.state.theme === 'dark' ? '亮色模式' : '暗色模式', variant: 'secondary', icon: Store.state.theme === 'dark' ? 'sun' : 'moon', onClick: 'Store.toggleTheme(); Router.refresh();' })}
        </div>
      </div>

      <div class="stats-grid admin-dashboard-stats">
        ${statCards.map(s => `
          <div class="stat-card admin-dashboard-stat">
            <span class="admin-stat-icon">${C.icon(s.icon, 17)}</span>
            <div class="admin-stat-copy">
              <div class="stat-card-label">${s.label}</div>
              <div class="stat-card-value">${s.value}</div>
              <div class="stat-card-change ${s.negative ? 'negative' : ''}">${s.change}</div>
            </div>
          </div>
        `).join('')}
      </div>

      <div class="admin-split-grid admin-dashboard-grid">
        <div class="data-table-wrapper">
          <div style="padding: var(--space-4); border-bottom: var(--border-default); display: flex; justify-content: space-between; align-items: center;">
            <span style="font-weight: var(--weight-semibold);">待处理举报</span>
            <a href="#/admin/reports" class="text-secondary" style="font-size: var(--text-sm);">查看全部 →</a>
          </div>
          <table class="data-table">
            <thead>
              <tr>
                <th>编号</th>
                <th>原因</th>
                <th>优先级</th>
                <th>状态</th>
                <th>被举报用户</th>
                <th>时间</th>
              </tr>
            </thead>
            <tbody>
              ${recentReports.map(r => `
                <tr>
                  <td><span class="mono">${r.id}</span></td>
                  <td>${r.reason}</td>
                  <td>${C.priorityBadge(r.priority)}</td>
                  <td>${C.statusBadge(r.status)}</td>
                  <td><a href="#/users/${r.reportedUser}">${r.reportedUser}</a></td>
                  <td class="text-secondary">${r.createdAt}</td>
                </tr>
              `).join('')}
            </tbody>
          </table>
        </div>

        <div class="data-table-wrapper">
          <div style="padding: var(--space-4); border-bottom: var(--border-default); display: flex; justify-content: space-between; align-items: center;">
            <span style="font-weight: var(--weight-semibold);">活跃用户</span>
          </div>
          <div style="padding: var(--space-2);">
            ${recentUsers.map(u => `
              <div style="display: flex; align-items: center; gap: var(--space-3); padding: var(--space-2) var(--space-2); border-radius: var(--radius-sm);">
                ${C.avatar(u.name, 'sm')}
                <div style="flex: 1; min-width: 0;">
                  <div style="font-size: var(--text-sm); font-weight: var(--weight-medium);">${u.name}</div>
                  <div style="font-size: var(--text-xs); color: var(--color-text-tertiary);">LV.${u.level}</div>
                </div>
                ${C.roleBadge(u.roles)}
              </div>
            `).join('')}
          </div>
        </div>
      </div>

      <div class="data-table-wrapper admin-storage-panel">
        <div class="admin-panel-head">
          <span>存储使用</span>
          <strong>${Math.round(stats.storageUsed / stats.storageTotal * 100)}%</strong>
        </div>
        <div class="admin-storage-body">
          <div class="admin-storage-meta">
            <span class="text-secondary">已使用</span>
            <span>${stats.storageUsed} GB / ${stats.storageTotal} GB</span>
          </div>
          <div class="admin-storage-track">
            <div class="admin-storage-fill" style="width: ${(stats.storageUsed / stats.storageTotal * 100)}%;"></div>
          </div>
        </div>
      </div>
      </div>
    `;

    return adminLayout(content, '/admin');
  };

  // ============================================
  // Admin Users (/admin/users)
  // ============================================
  P.adminUsers = function(params) {
    const query = (params.q || '').toLowerCase();
    const role = params.role || 'all';
    const users = Object.values(MockData.users).filter(user => {
      const matchesQuery = !query || user.name.toLowerCase().includes(query) || user.bio.toLowerCase().includes(query);
      const matchesRole = role === 'all' || (role === 'member' ? user.roles.length === 1 && user.roles.includes('member') : user.roles.includes(role));
      return matchesQuery && matchesRole;
    });

    const content = `
      <div class="admin-page-header">
        <div>
          <h1 class="admin-page-title">用户管理</h1>
          <p class="admin-page-desc">查看账号状态、角色与社区贡献</p>
        </div>
        ${C.button({ text: '邀请用户', variant: 'primary', icon: 'user-plus', onClick: "Toast.show('邀请链接已生成', 'success')" })}
      </div>

      <div class="data-table-wrapper">
        <div class="filter-bar">
          <div class="filter-left">
            <form class="admin-filter-form" onsubmit="event.preventDefault(); Router.updateAdminParams({ q: this.querySelector('input').value });">
              <input class="input-field" value="${C.escapeHtml(params.q || '')}" placeholder="搜索用户名或简介" />
              <button type="submit" class="btn btn-secondary btn-sm">${C.icon('search', 14)}<span>搜索</span></button>
            </form>
            <select class="filter-select" onchange="Router.updateAdminParams({ role: this.value })">
              <option value="all" ${role === 'all' ? 'selected' : ''}>全部角色</option>
              <option value="admin" ${role === 'admin' ? 'selected' : ''}>管理员</option>
              <option value="moderator" ${role === 'moderator' ? 'selected' : ''}>版主</option>
              <option value="member" ${role === 'member' ? 'selected' : ''}>普通会员</option>
            </select>
          </div>
          <span class="text-secondary" style="font-size:var(--text-sm);">共 ${users.length} 位用户</span>
        </div>
        <table class="data-table">
          <thead><tr><th>用户</th><th>角色</th><th>等级</th><th>贡献值</th><th>B币</th><th>注册时间</th><th>最后活跃</th><th>操作</th></tr></thead>
          <tbody>${users.map(user => `
            <tr>
              <td><div style="display:flex;align-items:center;gap:var(--space-2);">${C.avatar(user.name, 'sm')}<div><a href="#/users/${user.name}">${user.name}</a><div class="text-secondary" style="font-size:var(--text-xs);">${user.bio}</div></div></div></td>
              <td>${C.roleBadge(user.roles) || '<span class="text-secondary">会员</span>'}</td>
              <td>${C.levelBadge(user.level)}</td><td>${user.contribution}</td><td>${user.coins}</td>
              <td class="text-secondary">${user.joinedAt}</td><td class="text-secondary">${user.lastActive}</td>
              <td><div class="table-actions"><a class="btn btn-ghost btn-sm" href="#/users/${user.name}">查看</a><button class="btn btn-ghost btn-sm" onclick="Toast.show('用户编辑面板已打开', 'info')">编辑</button></div></td>
            </tr>`).join('') || `<tr><td colspan="8">${C.emptyState({ icon: 'users', title: '未找到用户', desc: '请调整搜索条件后重试。' })}</td></tr>`}</tbody>
        </table>
      </div>`;
    return adminLayout(content, '/admin/users');
  };

  // ============================================
  // Admin Content (/admin/content)
  // ============================================
  P.adminContent = function(params) {
    const type = params.type || 'all';
    const board = params.board || 'all';
    let posts = [...MockData.posts];
    if (type !== 'all') posts = posts.filter(post => post.type === type);
    if (board !== 'all') posts = posts.filter(post => post.board === board);

    const content = `
      <div class="admin-content-page">
        <div class="admin-page-header admin-content-header">
          <div><h1 class="admin-page-title">内容管理</h1><p class="admin-page-desc">审核、推荐和维护社区发布内容</p></div>
          ${C.button({ text: '发布内容', variant: 'primary', icon: 'plus', href: '#/publish' })}
        </div>
        <div class="stats-grid admin-content-stats">
          ${[
            { label: '全部内容', value: MockData.posts.length, note: '帖子与文章', icon: 'file-text' },
            { label: '待审核', value: MockData.adminStats.pendingPosts, note: '需要处理', icon: 'clock', alert: true },
            { label: '精华内容', value: MockData.posts.filter(p => p.isEssence).length, note: '社区精选', icon: 'award' },
            { label: '今日发布', value: 18, note: '较昨日 +3', icon: 'trending-up' }
          ].map(stat => `
            <div class="admin-content-stat ${stat.alert ? 'is-alert' : ''}">
              <span class="admin-content-stat-icon">${C.icon(stat.icon, 16)}</span>
              <div>
                <div class="stat-card-label">${stat.label}</div>
                <div class="admin-content-stat-value">${stat.value}</div>
                <div class="stat-card-change ${stat.alert ? 'negative' : ''}">${stat.note}</div>
              </div>
            </div>
          `).join('')}
        </div>
        <div class="data-table-wrapper admin-content-table">
          <div class="filter-bar admin-content-toolbar">
            <div class="filter-left">
              <span class="admin-toolbar-label">筛选</span>
              <select class="filter-select" onchange="Router.updateAdminParams({ type: this.value })"><option value="all">全部类型</option><option value="topic" ${type === 'topic' ? 'selected' : ''}>讨论帖</option><option value="article" ${type === 'article' ? 'selected' : ''}>专栏文章</option></select>
              <select class="filter-select" onchange="Router.updateAdminParams({ board: this.value })"><option value="all">全部板块</option>${MockData.boards.map(b => `<option value="${b.slug}" ${board === b.slug ? 'selected' : ''}>${b.name}</option>`).join('')}</select>
            </div>
            <span class="admin-result-count">${posts.length} 条内容</span>
          </div>
          <table class="data-table"><thead><tr><th>内容</th><th>类型</th><th>板块</th><th>可见等级</th><th>作者</th><th>数据</th><th>发布时间</th><th class="admin-action-head">操作</th></tr></thead>
            <tbody>${posts.map(post => `<tr><td class="admin-content-cell"><a class="admin-content-title" href="#/topics/${post.id}">${post.title}</a>${post.isPinned || post.isEssence ? `<div class="admin-content-badges">${post.isPinned ? C.badge('置顶', 'pinned') : ''}${post.isEssence ? C.badge('精华', 'essence') : ''}</div>` : ''}</td><td>${post.type === 'article' ? '文章' : '讨论'}</td><td>${C.categoryBadge(post.board)}</td><td>${post.visibilityLevel > 1 ? C.badge(`LV.${post.visibilityLevel}+`, 'warning') : C.badge('公开', 'neutral')}</td><td><a href="#/users/${post.author}">${post.author}</a></td><td class="text-secondary admin-content-data"><span>${post.replies} 回复</span><span>${post.views} 浏览</span></td><td class="text-secondary admin-content-date">${post.createdAt}</td><td><div class="table-actions"><a class="btn btn-ghost btn-sm" href="#/topics/${post.id}">查看</a><button class="btn btn-secondary btn-sm" onclick="Toast.show('审核状态已更新', 'success')">审核</button></div></td></tr>`).join('')}</tbody>
          </table>
        </div>
      </div>`;
    return adminLayout(content, '/admin/posts');
  };

  // ============================================
  // Admin Boards (/admin/boards)
  // ============================================
  P.adminBoards = function() {
    const content = `
      <div class="admin-page-header"><div><h1 class="admin-page-title">板块管理</h1><p class="admin-page-desc">配置板块信息、版主和发布规则</p></div>${C.button({ text: '新建板块', variant: 'primary', icon: 'plus', onClick: "Toast.show('新建板块面板已打开', 'info')" })}</div>
      <div class="data-table-wrapper"><table class="data-table"><thead><tr><th>排序</th><th>板块</th><th>内容统计</th><th>版主</th><th>规则</th><th>状态</th><th>操作</th></tr></thead>
        <tbody>${MockData.boards.map((board, index) => `<tr><td class="text-secondary">${index + 1}</td><td><div style="display:flex;align-items:center;gap:var(--space-3);"><span style="width:10px;height:36px;border-radius:var(--radius-full);background:${board.color};"></span><div><a href="#/boards/${board.slug}" style="font-weight:var(--weight-semibold);">${board.name}</a><div class="text-secondary" style="font-size:var(--text-xs);">${board.description}</div></div></div></td><td><strong>${board.postCount}</strong> 帖子<div class="text-secondary" style="font-size:var(--text-xs);">今日 +${board.todayCount}</div></td><td>${board.mods.map(mod => `<a href="#/users/${mod}">${mod}</a>`).join('、')}</td><td>${board.rules.length} 条</td><td><span class="badge badge-status-resolved">正常</span></td><td><div class="table-actions"><button class="btn btn-ghost btn-sm" onclick="Toast.show('板块编辑面板已打开', 'info')">编辑</button><button class="btn btn-ghost btn-sm" onclick="Toast.show('排序已调整', 'success')">排序</button></div></td></tr>`).join('')}</tbody>
      </table></div>`;
    return adminLayout(content, '/admin/boards');
  };

  function adminSimpleTable(title, desc, headers, rows, path, actionText = '新建') {
    const content = `
      <div class="admin-page-header"><div><h1 class="admin-page-title">${title}</h1><p class="admin-page-desc">${desc}</p></div>${C.button({ text: actionText, variant: 'primary', icon: 'plus', onClick: `Toast.show('${actionText}面板已打开', 'info')` })}</div>
      <div class="data-table-wrapper"><div class="filter-bar"><div class="filter-left"><input class="input-field" placeholder="搜索${title}" style="width:min(260px,100%);" /></div><span class="text-secondary" style="font-size:var(--text-sm);">共 ${rows.length} 条</span></div>
      <table class="data-table"><thead><tr>${headers.map(h => `<th>${h}</th>`).join('')}</tr></thead><tbody>${rows.map(row => `<tr>${row.map(cell => `<td>${cell}</td>`).join('')}</tr>`).join('')}</tbody></table></div>`;
    return adminLayout(content, path);
  }

  P.adminRoles = function() {
    return adminSimpleTable('角色与权限', '管理全局角色、板块角色与权限分配', ['角色', '作用域', '用户数', '核心权限', '状态'], [
      ['管理员', '全站', '2', '系统、用户、审核、积分', C.statusBadge('published')],
      ['版主', '全站 / 板块', '8', '内容审核、板块处罚', C.statusBadge('published')],
      ['社区成员', '全站', '3,246', '发布、回复、收藏、举报', C.statusBadge('published')]
    ], '/admin/roles', '创建角色');
  };

  P.adminTags = function() {
    return adminSimpleTable('标签管理', '维护标签、分组和内容关联', ['标签', '分组', '使用次数', '最近使用', '操作'], MockData.tags.map(tag => [C.tag(tag.name), '技术主题', tag.count, '今天', '<button class="btn btn-ghost btn-sm" onclick="Toast.show(\'标签编辑面板已打开\', \'info\')">编辑</button>']), '/admin/tags', '新建标签');
  };

  P.adminAttachments = function() {
    return adminSimpleTable('附件管理', '管理持久附件对象、用户容量与安全状态；临时公开链接过期不会删除附件', ['文件', '大小', '上传者 / 等级', '配额占用', '链接策略', '状态'], [
      ['sqlite-wal.png', '286 KB', `Chaos · ${C.levelBadge(6)}`, '638 MB / 2 GB', '按需签发 · 300 秒', C.statusBadge('published')],
      ['svelte-arch.png', '42 KB', `Alice · ${C.levelBadge(4)}`, '126 MB / 500 MB', '按需签发 · 300 秒', C.badge('待检查', 'warning')],
      ['oauth-flow.pdf', '1.2 MB', `Bob · ${C.levelBadge(5)}`, '814 MB / 1 GB', '按需签发 · 300 秒', C.statusBadge('published')]
    ], '/admin/attachments', '上传附件');
  };

  P.adminShop = function() {
    const products=Store.state.shopProducts;
    const content=`<div class="admin-page-header"><div><h1 class="admin-page-title">内部商城</h1><p class="admin-page-desc">管理全局装扮、互动道具、库存、价格和安全展示 Token</p></div>${C.button({text:'新建商品',variant:'primary',icon:'plus',onClick:"Toast.show('商品编辑器已打开：只允许安全 Token 和受控附件', 'info')"})}</div><div class="card" style="padding:var(--space-4);margin-bottom:var(--space-4);display:flex;gap:var(--space-3);border-color:var(--color-warning);background:var(--color-warning-soft);">${C.icon('shield-check',18)}<div><strong>商城不能出售权限</strong><div class="text-secondary">禁止任意 CSS/HTML/脚本、远程资源、管理员徽章仿冒，以及购买审核结果、内容权限或封禁豁免。</div></div></div><div class="stats-grid" style="margin-bottom:var(--space-4);"><div class="stat-card"><div class="stat-card-label">已发布商品</div><div class="stat-card-value">${products.length}</div></div><div class="stat-card"><div class="stat-card-label">今日订单</div><div class="stat-card-value">86</div></div><div class="stat-card"><div class="stat-card-label">今日回收 B币</div><div class="stat-card-value">4,680</div></div></div><div class="data-table-wrapper"><table class="data-table"><thead><tr><th>商品</th><th>类型 / 槽位</th><th>价格</th><th>持有</th><th>状态</th><th>操作</th></tr></thead><tbody>${products.map(p=>`<tr><td><strong>${C.escapeHtml(p.title)}</strong><div class="text-secondary">${C.escapeHtml(p.description)}</div></td><td><span class="mono">${p.kind}</span><div class="text-secondary">${p.slot}</div></td><td>${p.price} B币</td><td>${p.owned?'已有用户持有':'—'}</td><td>${C.statusBadge('published')}</td><td><button class="btn btn-ghost btn-sm" onclick="Toast.show('商品版本编辑器已打开', 'info')">编辑</button></td></tr>`).join('')}</tbody></table></div>`;
    return adminLayout(content,'/admin/shop');
  };

  P.adminActivity = function() {
    const content=`<div class="admin-page-header"><div><h1 class="admin-page-title">活跃运营</h1><p class="admin-page-desc">配置签到、社区任务、Reaction 与防刷奖励规则</p></div>${C.button({text:'新建任务',variant:'primary',icon:'plus',onClick:"Toast.show('活跃任务编辑器已打开', 'info')"})}</div><div class="stats-grid" style="margin-bottom:var(--space-4);"><div class="stat-card"><div class="stat-card-label">今日签到</div><div class="stat-card-value">648</div></div><div class="stat-card"><div class="stat-card-label">今日奖励</div><div class="stat-card-value">8,420 B币</div></div><div class="stat-card"><div class="stat-card-label">风控拒绝</div><div class="stat-card-value">37</div></div></div><div class="data-table-wrapper"><table class="data-table"><thead><tr><th>规则</th><th>周期</th><th>奖励</th><th>上限/去重</th><th>状态</th></tr></thead><tbody><tr><td>每日签到</td><td>站点自然日</td><td>10 B币</td><td>用户 + activity_day</td><td>${C.statusBadge('published')}</td></tr><tr><td>优质内容发布</td><td>每周</td><td>20 B币</td><td>审核通过 + 帖子唯一</td><td>${C.statusBadge('published')}</td></tr><tr><td>收到真实互动</td><td>每日</td><td>最多 15 B币</td><td>排除自己/关联风险账号</td><td>${C.statusBadge('published')}</td></tr></tbody></table><div style="padding:var(--space-4);" class="text-secondary">奖励撤销通过补偿账本完成；规则换版不重算历史，不能用活动规则修改权限或审核。</div></div>`;
    return adminLayout(content,'/admin/activity');
  };

  P.adminVideo = function() {
    const cfg = Store.state.videoConfig;
    const content = `
      <div class="admin-page-header"><div><h1 class="admin-page-title">视频插件</h1><p class="admin-page-desc">配置用户手动插入的视频 URL、西瓜视频和 HLS 安全策略</p></div><span class="badge ${cfg.enabled ? 'badge-success' : 'badge-neutral'}">${cfg.enabled ? '已启用' : '已停用'}</span></div>
      <div class="card" style="padding:var(--space-4);margin-bottom:var(--space-4);display:flex;gap:var(--space-3);border-color:var(--color-warning);background:var(--color-warning-soft);">${C.icon('shield-check',18)}<div><strong>受控解析，不是开放代理</strong><div class="text-secondary" style="margin-top:var(--space-1);">后端只解析 HTTPS 白名单来源并防御 SSRF。西瓜视频仅使用官方允许的嵌入方式，否则降级为外链；不会抓取签名播放地址、绕过 DRM 或转存第三方视频。</div></div></div>
      <div class="data-table-wrapper" style="margin-bottom:var(--space-4);"><div style="padding:var(--space-4);border-bottom:var(--border-default);font-weight:var(--weight-semibold);">Provider 与格式策略</div><div style="padding:var(--space-5);">
        <div class="settings-row"><div class="settings-row-label"><div class="settings-row-title">启用视频插件</div><div class="settings-row-desc">停用后禁止创建新引用，历史引用按安全降级策略显示。</div></div><button class="switch ${cfg.enabled?'is-on':''}" role="switch" aria-checked="${cfg.enabled}" onclick="toggleVideoOption('enabled')"><span class="switch-knob"></span></button></div>
        <div class="settings-row"><div class="settings-row-label"><div class="settings-row-title">直接视频 URL</div><div class="settings-row-desc">支持 MP4、WebM、OGV；MOV 需经服务端能力探测。</div></div><button class="switch ${cfg.directEnabled?'is-on':''}" role="switch" aria-checked="${cfg.directEnabled}" onclick="toggleVideoOption('directEnabled')"><span class="switch-knob"></span></button></div>
        <div class="settings-row"><div class="settings-row-label"><div class="settings-row-title">HLS 流媒体</div><div class="settings-row-desc">支持 .m3u8；playlist、分片、Key、Map 和重定向逐级校验。</div></div><button class="switch ${cfg.hlsEnabled?'is-on':''}" role="switch" aria-checked="${cfg.hlsEnabled}" onclick="toggleVideoOption('hlsEnabled')"><span class="switch-knob"></span></button></div>
        <div class="settings-row"><div class="settings-row-label"><div class="settings-row-title">西瓜视频</div><div class="settings-row-desc">仅允许公开页面 URL 和经确认的官方嵌入域名。</div></div><button class="switch ${cfg.xiguaEnabled?'is-on':''}" role="switch" aria-checked="${cfg.xiguaEnabled}" onclick="toggleVideoOption('xiguaEnabled')"><span class="switch-knob"></span></button></div>
        <div class="form-grid form-grid-2" style="margin-top:var(--space-4);">
          <div class="input-wrapper"><label class="input-label" for="video-max-duration">最长视频时长（秒）</label><input type="number" min="60" max="86400" class="input-field" id="video-max-duration" value="${cfg.maxDurationSeconds}" /></div>
          <div class="input-wrapper"><label class="input-label" for="video-hls-segments">HLS 最大分片数</label><input type="number" min="1" max="10000" class="input-field" id="video-hls-segments" value="${cfg.hlsMaxSegments}" /></div>
          <div class="input-wrapper"><label class="input-label" for="video-hls-bytes">HLS 最大估算流量（MB）</label><input type="number" min="1" max="4096" class="input-field" id="video-hls-bytes" value="${cfg.hlsMaxBytesMb}" /></div>
          <div class="input-wrapper"><label class="input-label">允许的媒体类型</label><div class="input-field" style="height:auto;min-height:40px;">${cfg.allowedMediaTypes.map(type => `<span class="mono" style="margin-right:var(--space-2);">${C.escapeHtml(type)}</span>`).join('')}</div></div>
        </div>
      </div><div class="card-footer" style="display:flex;justify-content:flex-end;gap:var(--space-2);">${C.button({text:'安全探测测试',variant:'secondary',icon:'activity',onClick:'testVideoPolicy()'})}${C.button({text:'保存视频策略',variant:'primary',icon:'save',onClick:'saveVideoConfig()'})}</div></div>
      <div class="data-table-wrapper"><div style="padding:var(--space-4);border-bottom:var(--border-default);font-weight:var(--weight-semibold);">解析与降级状态</div><table class="data-table"><thead><tr><th>来源</th><th>处理方式</th><th>关键限制</th><th>状态</th></tr></thead><tbody>
        <tr><td>MP4 / WebM / OGV</td><td>原生播放器</td><td>HTTPS + MIME/Range 探测</td><td>${cfg.directEnabled ? C.statusBadge('published') : C.badge('停用','neutral')}</td></tr>
        <tr><td>HLS (.m3u8)</td><td>受控 HLS 播放器</td><td>${cfg.hlsMaxSegments} 分片 / ${cfg.hlsMaxBytesMb} MB</td><td>${cfg.hlsEnabled ? C.statusBadge('published') : C.badge('停用','neutral')}</td></tr>
        <tr><td>西瓜视频</td><td>官方嵌入或外链卡片</td><td>CSP + sandbox + 无自动播放</td><td>${cfg.xiguaEnabled ? C.statusBadge('published') : C.badge('停用','neutral')}</td></tr>
      </tbody></table><div style="padding:var(--space-4);border-top:var(--border-default);" class="text-secondary">策略版本 <span class="mono">v${cfg.policyVersion}</span> · 测试接口 <span class="mono">POST /api/v1/admin/video/policies/test</span></div></div>
    `;
    return adminLayout(content, '/admin/video');
  };

  P.adminAI = function() {
    const cfg = Store.state.aiConfig;
    const content = `
      <div class="admin-page-header"><div><h1 class="admin-page-title">大模型设置</h1><p class="admin-page-desc">配置受控 AI Gateway，用于格式化、内容审计和 SEO 辅助</p></div><span class="badge ${cfg.enabled ? 'badge-success' : 'badge-neutral'}">${cfg.enabled ? '已启用' : '已停用'}</span></div>
      <div class="card" style="padding:var(--space-4);margin-bottom:var(--space-4);display:flex;gap:var(--space-3);border-color:var(--color-warning);background:var(--color-warning-soft);">${C.icon('shield-check',18)}<div><strong>模型不是业务裁决者</strong><div class="text-secondary" style="margin-top:var(--space-1);">模型只能生成建议和风险信号，不能直接发布、删除、封禁、修改权限、价格或积分。浏览器不会获得 Provider Secret。</div></div></div>
      <div class="data-table-wrapper" style="margin-bottom:var(--space-4);"><div style="padding:var(--space-4);border-bottom:var(--border-default);font-weight:var(--weight-semibold);">Provider 与模型</div><div style="padding:var(--space-5);">
        <div class="settings-row"><div class="settings-row-label"><div class="settings-row-title">启用 AI Gateway</div><div class="settings-row-desc">停用后普通发帖、审核和 SEO 仍可正常运行。</div></div><button class="switch ${cfg.enabled?'is-on':''}" role="switch" aria-checked="${cfg.enabled}" onclick="toggleAiGateway()"><span class="switch-knob"></span></button></div>
        <div class="form-grid form-grid-2" style="margin-top:var(--space-4);">
          <div class="input-wrapper"><label class="input-label" for="ai-provider">Provider</label><input class="input-field" id="ai-provider" value="${C.escapeHtml(cfg.providerName)}" /></div>
          <div class="input-wrapper"><label class="input-label" for="ai-model">默认模型</label><input class="input-field" id="ai-model" value="${C.escapeHtml(cfg.defaultModel)}" /></div>
          <div class="input-wrapper"><label class="input-label" for="ai-base-url">API Base URL</label><input class="input-field" id="ai-base-url" value="${C.escapeHtml(cfg.baseUrl)}" /><div class="input-hint">生产仅允许 HTTPS 白名单域名，并执行 SSRF/DNS 重绑定防护。</div></div>
          <div class="input-wrapper"><label class="input-label" for="ai-secret">API Secret</label><input type="password" class="input-field" id="ai-secret" value="" placeholder="${cfg.secretConfigured?'已配置，留空表示不修改':'请输入 Secret'}" autocomplete="new-password" /><div class="input-hint">Secret 仅写入后端受保护配置，不回显。</div></div>
          <div class="input-wrapper"><label class="input-label" for="ai-data-mode">内容发送策略</label><select class="input-field" id="ai-data-mode"><option value="redacted" ${cfg.dataMode==='redacted'?'selected':''}>脱敏内容（推荐）</option><option value="metadata_only" ${cfg.dataMode==='metadata_only'?'selected':''}>仅元数据</option><option value="full_with_consent" ${cfg.dataMode==='full_with_consent'?'selected':''}>用户单独同意后完整内容</option><option value="disabled" ${cfg.dataMode==='disabled'?'selected':''}>禁止外发</option></select></div>
          <div class="input-wrapper"><label class="input-label" for="ai-budget">每日 Token 预算</label><input type="number" min="0" class="input-field" id="ai-budget" value="${cfg.dailyBudget}" /></div>
          <div class="input-wrapper"><label class="input-label" for="ai-timeout">请求超时（秒）</label><input type="number" min="3" max="120" class="input-field" id="ai-timeout" value="${cfg.timeoutSeconds}" /></div>
        </div>
      </div><div class="card-footer" style="display:flex;justify-content:flex-end;gap:var(--space-2);">${C.button({text:'测试脱敏连接',variant:'secondary',icon:'activity',onClick:'testAiProvider()'})}${C.button({text:'保存 AI 配置',variant:'primary',icon:'save',onClick:'saveAiConfig()'})}</div></div>
      <div class="data-table-wrapper"><div style="padding:var(--space-4);border-bottom:var(--border-default);font-weight:var(--weight-semibold);">功能与任务状态</div><table class="data-table"><thead><tr><th>用途</th><th>模型</th><th>数据策略</th><th>今日任务</th><th>P95</th><th>状态</th></tr></thead><tbody>
        <tr><td>发帖格式化</td><td>${C.escapeHtml(cfg.defaultModel)}</td><td>脱敏草稿</td><td>186</td><td>1.8s</td><td>${C.statusBadge('published')}</td></tr>
        <tr><td>内容审计建议</td><td>${C.escapeHtml(cfg.defaultModel)}</td><td>脱敏 + 人工复核</td><td>328</td><td>2.3s</td><td>${C.statusBadge('published')}</td></tr>
        <tr><td>SEO 优化</td><td>${C.escapeHtml(cfg.defaultModel)}</td><td>仅公开内容</td><td>64</td><td>2.0s</td><td>${C.statusBadge('published')}</td></tr>
      </tbody></table><div style="padding:var(--space-4);border-top:var(--border-default);" class="text-secondary">策略版本 <span class="mono">v${cfg.policyVersion}</span> · Provider 故障时不阻塞普通发帖，也不能绕过核心审核。</div></div>
    `;
    return adminLayout(content, '/admin/ai');
  };

  P.adminDownloadBilling = function() {
    const cfg = Store.state.downloadBillingConfig;
    const content = `
      <div class="admin-page-header">
        <div><h1 class="admin-page-title">下载计费</h1><p class="admin-page-desc">配置附件下载抵扣 B币的策略、授权复用和安全限额</p></div>
        <span class="badge ${cfg.enabled ? 'badge-success' : 'badge-neutral'}">${cfg.enabled ? '已启用' : '已停用'}</span>
      </div>
      <div class="card" style="padding:var(--space-4);margin-bottom:var(--space-4);display:flex;gap:var(--space-3);border-color:var(--color-warning);background:var(--color-warning-soft);">
        ${C.icon('shield-check', 18)}<div><strong>扣费发生在下载授权阶段</strong><div class="text-secondary" style="margin-top:var(--space-1);">只有后端鉴权、余额校验、扣款、不可变流水和下载授权同一事务提交后，才会签发临时链接。S3 链接过期不会再次扣费。</div></div>
      </div>
      <div class="data-table-wrapper">
        <div style="padding:var(--space-4);border-bottom:var(--border-default);font-weight:var(--weight-semibold);">全局下载策略</div>
        <div style="padding:var(--space-5);">
          <div class="settings-row"><div class="settings-row-label"><div class="settings-row-title">启用下载抵扣积分</div><div class="settings-row-desc">停用只影响新授权，不撤销已有授权、不修改历史流水。</div></div><button class="switch ${cfg.enabled ? 'is-on' : ''}" role="switch" aria-checked="${cfg.enabled}" onclick="toggleDownloadBilling()"><span class="switch-knob"></span></button></div>
          <div class="form-grid form-grid-2" style="margin-top:var(--space-4);">
            <div class="input-wrapper"><label class="input-label" for="download-default-price">默认下载价格（B币）</label><input type="number" min="0" max="1000000" class="input-field" id="download-default-price" value="${cfg.defaultPrice}" /><div class="input-hint">0 表示免费；价格由后端策略决定，不能信任客户端提交值。</div></div>
            <div class="input-wrapper"><label class="input-label" for="download-auth-ttl">下载授权有效期（小时）</label><input type="number" min="1" max="720" class="input-field" id="download-auth-ttl" value="${cfg.authorizationTtlHours}" /><div class="input-hint">授权有效期与 S3 临时 URL 有效期独立。</div></div>
            <div class="input-wrapper"><label class="input-label" for="download-daily-limit">用户每日下载扣费上限（B币）</label><input type="number" min="0" max="100000" class="input-field" id="download-daily-limit" value="${cfg.dailyUserLimit}" /><div class="input-hint">0 表示不额外限制，仍受余额和频率限制。</div></div>
            <div class="input-wrapper"><label class="input-label" for="download-max-charge">单次扣费上限（B币）</label><input type="number" min="0" max="1000000" class="input-field" id="download-max-charge" value="${cfg.maxSingleCharge}" /></div>
          </div>
          <div class="settings-row" style="margin-top:var(--space-4);"><div class="settings-row-label"><div class="settings-row-title">免费等级</div><div class="settings-row-desc">命中免费等级仍需通过后端附件权限校验。</div></div><strong>LV.${cfg.freeLevels.join('、LV.')}</strong></div>
          <div class="settings-row"><div class="settings-row-label"><div class="settings-row-title">当前策略版本</div><div class="settings-row-desc">只影响新下载授权，便于审计和并发冲突检测。</div></div><span class="mono">v${cfg.policyVersion}</span></div>
        </div>
        <div class="card-footer" style="display:flex;justify-content:flex-end;gap:var(--space-2);">${C.button({ text: '保存下载策略', variant: 'primary', icon: 'save', onClick: 'saveDownloadBilling()' })}</div>
      </div>
      <div class="data-table-wrapper" style="margin-top:var(--space-4);"><div style="padding:var(--space-4);border-bottom:var(--border-default);font-weight:var(--weight-semibold);">计费覆盖与接口状态</div><table class="data-table"><thead><tr><th>范围</th><th>策略</th><th>授权复用</th><th>最近扣费</th><th>状态</th></tr></thead><tbody>
        <tr><td>站点默认</td><td><strong>${cfg.defaultPrice} B币 / 授权周期</strong></td><td>${cfg.authorizationTtlHours} 小时</td><td>1,284 次</td><td>${C.statusBadge('published')}</td></tr>
        <tr><td>Rust 板块</td><td>继承站点策略</td><td>不重复扣费</td><td>638 次</td><td>${C.statusBadge('published')}</td></tr>
        <tr><td>未标记付费附件</td><td>免费（范围策略）</td><td>仍需鉴权</td><td>—</td><td>${C.badge('免费', 'neutral')}</td></tr>
      </tbody></table><div style="padding:var(--space-4);border-top:var(--border-default);" class="text-secondary">接口：<span class="mono">POST /api/v1/attachments/{id}/download</span> · 必须携带 <span class="mono">Idempotency-Key</span></div></div>
    `;
    return adminLayout(content, '/admin/download-billing');
  };

  P.adminStorage = function() {
    const cfg = Store.state.storageConfig;
    const status = cfg.connectionStatus === 'connected'
      ? '<span class="badge badge-success">连接正常</span>'
      : cfg.connectionStatus === 'error'
        ? '<span class="badge badge-danger">连接失败</span>'
        : '<span class="badge badge-neutral">尚未测试</span>';
    const content = `
      <div class="admin-page-header"><div><h1 class="admin-page-title">文件存储</h1><p class="admin-page-desc">配置本地磁盘或 S3 兼容对象存储</p></div><div>${status}</div></div>

      <div class="data-table-wrapper" style="margin-bottom:var(--space-4);">
        <div style="padding:var(--space-4);border-bottom:var(--border-default);font-weight:var(--weight-semibold);">存储后端</div>
        <div style="padding:var(--space-5);">
          <div class="type-switch" style="width:max-content;margin-bottom:var(--space-5);">
            <button class="${cfg.backend === 'local' ? 'active' : ''}" onclick="setStorageBackend('local')">${C.icon('laptop', 14)} 本地磁盘</button>
            <button class="${cfg.backend === 's3' ? 'active' : ''}" onclick="setStorageBackend('s3')">${C.icon('package', 14)} S3 兼容存储</button>
          </div>
          ${cfg.backend === 'local' ? `
            <div class="input-wrapper"><label class="input-label" for="storage-local-path">附件目录</label><input class="input-field" id="storage-local-path" value="${C.escapeHtml(cfg.localPath)}" /><div class="input-hint">目录必须位于 Web 根目录之外，由服务进程独占写入。</div></div>
          ` : `
            <div class="form-grid form-grid-2">
              <div class="input-wrapper"><label class="input-label" for="storage-endpoint">Endpoint</label><input class="input-field" id="storage-endpoint" value="${C.escapeHtml(cfg.endpoint)}" placeholder="https://s3.amazonaws.com" /><div class="input-hint">支持 AWS S3、MinIO、Cloudflare R2 等兼容服务。</div></div>
              <div class="input-wrapper"><label class="input-label" for="storage-region">Region</label><input class="input-field" id="storage-region" value="${C.escapeHtml(cfg.region)}" placeholder="ap-southeast-1" /></div>
              <div class="input-wrapper"><label class="input-label" for="storage-bucket">Bucket</label><input class="input-field" id="storage-bucket" value="${C.escapeHtml(cfg.bucket)}" placeholder="bblbb-attachments" /></div>
              <div class="input-wrapper"><label class="input-label" for="storage-access-key">Access Key ID</label><input class="input-field" id="storage-access-key" value="${C.escapeHtml(cfg.accessKeyId)}" autocomplete="off" /></div>
              <div class="input-wrapper"><label class="input-label" for="storage-secret">Secret Access Key</label><input type="password" class="input-field" id="storage-secret" value="" placeholder="${cfg.secretConfigured ? '已配置，留空表示不修改' : '请输入 Secret Access Key'}" autocomplete="new-password" /><div class="input-hint">密钥只提交给后端，不在页面和日志中回显。</div></div>
              <div class="input-wrapper"><label class="input-label" for="storage-public-url">公开资源域名（可选）</label><input class="input-field" id="storage-public-url" value="${C.escapeHtml(cfg.publicBaseUrl)}" placeholder="https://cdn.example.com" /></div>
            </div>
            <div class="settings-row"><div class="settings-row-label"><div class="settings-row-title">Path-style 请求</div><div class="settings-row-desc">MinIO 等服务可能需要启用；AWS S3 默认关闭。</div></div><button class="switch ${cfg.pathStyle ? 'is-on' : ''}" role="switch" aria-checked="${cfg.pathStyle}" onclick="toggleStorageOption('pathStyle')"><span class="switch-knob"></span></button></div>
            <div class="settings-row"><div class="settings-row-label"><div class="settings-row-title">预签名直传</div><div class="settings-row-desc">客户端获取短期上传参数，完成后仍由服务端 HEAD 和内容校验。</div></div><button class="switch ${cfg.presignedUploads ? 'is-on' : ''}" role="switch" aria-checked="${cfg.presignedUploads}" onclick="toggleStorageOption('presignedUploads')"><span class="switch-knob"></span></button></div>
          `}
        </div>
      </div>

      <div class="data-table-wrapper">
        <div style="padding:var(--space-4);border-bottom:var(--border-default);font-weight:var(--weight-semibold);">安全与上传限制</div>
        <div style="padding:var(--space-5);">
          <div class="form-grid form-grid-2">
            <div class="input-wrapper"><label class="input-label" for="storage-max-upload">站点单文件硬上限（MB）</label><input type="number" min="1" max="1024" class="input-field" id="storage-max-upload" value="${cfg.maxUploadMb}" /><div class="input-hint">用户实际限制取站点、用途、板块和等级限制的最小值。</div></div>
            <div class="input-wrapper"><label class="input-label" for="storage-signed-ttl">S3 公开链接有效期（秒）</label><input type="number" min="60" max="604800" class="input-field" id="storage-signed-ttl" value="${cfg.signedUrlTtl}" /><div class="input-hint">链接过期后重新鉴权并签发，不删除附件对象。</div></div>
          </div>
          <div class="notice notice-warning" style="margin-top:var(--space-4);">${C.icon('alert-triangle', 16)} Bucket 应保持私有。公开链接是临时访问凭证，过期只会使 URL 失效；附件对象持续保留，只有用户主动删除或管理员清理才会删除。切换存储后端不会自动迁移已有对象。</div>
        </div>
        <div class="card-footer" style="display:flex;justify-content:flex-end;gap:var(--space-2);">
          ${C.button({ text: '保存配置', variant: 'secondary', onClick: 'saveStorageConfig()' })}
          ${C.button({ text: '保存并测试连接', variant: 'primary', icon: 'check-circle', onClick: 'testStorageConfig()' })}
        </div>
      </div>
      ${cfg.lastTestedAt ? `<p class="text-secondary" style="font-size:var(--text-xs);margin-top:var(--space-3);">上次测试：${cfg.lastTestedAt}</p>` : ''}`;
    return adminLayout(content, '/admin/storage');
  };

  P.adminNotifications = function() {
    return adminSimpleTable('通知与邮件', '管理模板、投递渠道和失败重试', ['模板', '渠道', '今日发送', '失败', '状态'], [
      ['新回复通知', '站内 + 邮件', '186', '2', C.statusBadge('published')],
      ['审核结果通知', '站内 + 邮件', '24', '0', C.statusBadge('published')],
      ['安全登录通知', '邮件', '38', '1', C.badge('需关注', 'warning')]
    ], '/admin/notifications', '新建模板');
  };

  P.adminAudit = function(params) {
    const rows = Store.state.reports.flatMap(report => report.history.slice(-2).map(item => [item.time, item.operator, item.type, report.id, item.reason || '—', '<span class="mono">req_demo</span>']));
    return adminSimpleTable('审计日志', '不可删除的管理操作与状态变更记录', ['时间', '操作者', '动作', '目标', '原因', 'Request ID'], rows.length ? rows : [['今天', 'Echo', 'settings.update', 'site', '初始化站点', '<span class="mono">req_init</span>']], '/admin/audit', '导出日志');
  };

  // ============================================
  // Admin Reports List (/admin/reports)
  // ============================================
  P.adminReports = function(params) {
    const status = params.status || 'all';
    const priority = params.priority || 'all';
    let reports = Store.state.reports;

    if (status !== 'all') reports = reports.filter(r => r.status === status);
    if (priority !== 'all') reports = reports.filter(r => r.priority === priority);

    const content = `
      <div class="admin-page-header">
        <div>
          <h1 class="admin-page-title">举报与审核</h1>
          <p class="admin-page-desc">处理用户举报，维护社区秩序</p>
        </div>
      </div>

      <div class="data-table-wrapper">
        <div class="filter-bar">
          <div class="filter-left">
            <select class="filter-select" onchange="Router.updateAdminParams({ status: this.value })">
              <option value="all" ${status === 'all' ? 'selected' : ''}>全部状态</option>
              <option value="pending" ${status === 'pending' ? 'selected' : ''}>待处理</option>
              <option value="processing" ${status === 'processing' ? 'selected' : ''}>处理中</option>
              <option value="resolved" ${status === 'resolved' ? 'selected' : ''}>已解决</option>
              <option value="rejected" ${status === 'rejected' ? 'selected' : ''}>已驳回</option>
            </select>
            <select class="filter-select" onchange="Router.updateAdminParams({ priority: this.value })">
              <option value="all" ${priority === 'all' ? 'selected' : ''}>全部优先级</option>
              <option value="high" ${priority === 'high' ? 'selected' : ''}>高优先级</option>
              <option value="medium" ${priority === 'medium' ? 'selected' : ''}>中优先级</option>
              <option value="low" ${priority === 'low' ? 'selected' : ''}>低优先级</option>
            </select>
          </div>
          <div class="filter-right">
            <span class="text-secondary" style="font-size: var(--text-sm);">共 ${reports.length} 条</span>
          </div>
        </div>
        <table class="data-table">
          <thead>
            <tr>
              <th>编号</th>
              <th>原因</th>
              <th>优先级</th>
              <th>状态</th>
              <th>举报人</th>
              <th>被举报用户</th>
              <th>板块</th>
              <th>创建时间</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            ${reports.length > 0 ? reports.map(r => `
              <tr>
                <td><span class="mono">${r.id}</span></td>
                <td>${r.reason}</td>
                <td>${C.priorityBadge(r.priority)}</td>
                <td>${C.statusBadge(r.status)}</td>
                <td><a href="#/users/${r.reporter}">${r.reporter}</a></td>
                <td><a href="#/users/${r.reportedUser}">${r.reportedUser}</a></td>
                <td>${MockData.getBoard(r.board)?.name || r.board}</td>
                <td class="text-secondary">${r.createdAt}</td>
                <td>
                  <div class="table-actions">
                    <a href="#/admin/reports/${r.id}" class="btn btn-ghost btn-sm">查看</a>
                  </div>
                </td>
              </tr>
            `).join('') : `
              <tr><td colspan="9">${C.emptyState({ icon: 'flag', title: '暂无举报', desc: '当前筛选条件下暂无举报记录。' })}</td></tr>
            `}
          </tbody>
        </table>
      </div>
    `;

    return adminLayout(content, '/admin/reports');
  };

  // ============================================
  // Admin Report Detail (/admin/reports/[id])
  // ============================================
  P.adminReportDetail = function(id) {
    const report = Store.state.reports.find(r => r.id === id);
    if (!report) return P.notFound('举报不存在');

    const actionLabels = {
      report: '举报提交',
      accept: '受理举报',
      hide: '隐藏内容',
      restore: '恢复内容',
      warn: '警告用户',
      mute: '禁言用户',
      ban: '封禁用户',
      appeal: '申诉'
    };

    const content = `
      <div class="admin-page-header">
        <div>
          <a href="#/admin/reports" class="text-secondary" style="font-size: var(--text-sm);">← 返回举报列表</a>
          <h1 class="admin-page-title" style="margin-top: var(--space-2);">
            举报 ${report.id}
            ${C.priorityBadge(report.priority)}
            ${C.statusBadge(report.status)}
          </h1>
          <p class="admin-page-desc">${report.reason}</p>
        </div>
      </div>

      <div class="admin-split-grid">
        <div>
          <div class="data-table-wrapper" style="margin-bottom: var(--space-4);">
            <div style="padding: var(--space-4); border-bottom: var(--border-default); font-weight: var(--weight-semibold);">
              举报内容
            </div>
            <div style="padding: var(--space-4);">
              <div style="background: var(--color-danger-soft); border: 1px solid var(--color-danger); border-radius: var(--radius-sm); padding: var(--space-3); margin-bottom: var(--space-3);">
                <div style="font-size: var(--text-sm); color: var(--color-danger); margin-bottom: var(--space-2);">
                  ${C.icon('alert-triangle', 14)} 被举报内容
                </div>
                <div style="font-size: var(--text-sm); line-height: var(--text-sm-leading);">"${report.content}"</div>
              </div>
              <div style="font-size: var(--text-sm); color: var(--color-text-secondary);">
                <div style="margin-bottom: var(--space-2);">
                  <strong>板块：</strong>${MockData.getBoard(report.board)?.name || report.board}
                </div>
                <div style="margin-bottom: var(--space-2);">
                  <strong>证据：</strong>${report.evidence || '无'}
                </div>
                <div>
                  <a href="${report.contentUrl}" target="_blank">查看原帖 →</a>
                </div>
              </div>
            </div>
          </div>

          <div class="data-table-wrapper">
            <div style="padding: var(--space-4); border-bottom: var(--border-default); font-weight: var(--weight-semibold);">
              处理时间线
            </div>
            <div style="padding: var(--space-5);">
              <div class="mod-timeline">
                ${report.history.map(h => `
                  <div class="mod-timeline-item ${h.type}">
                    <div class="mod-timeline-dot"></div>
                    <div class="mod-timeline-content">
                      <strong>${actionLabels[h.type] || h.type}</strong>
                      <span class="text-secondary"> — ${h.operator}</span>
                    </div>
                    <div class="mod-timeline-time">${h.time}</div>
                    ${h.reason ? `<div class="mod-timeline-reason">原因：${h.reason}</div>` : ''}
                  </div>
                `).join('')}
              </div>
            </div>
          </div>
        </div>

        <div>
          <div class="data-table-wrapper" style="margin-bottom: var(--space-4);">
            <div style="padding: var(--space-4); border-bottom: var(--border-default); font-weight: var(--weight-semibold);">
              被举报用户
            </div>
            <div style="padding: var(--space-4);">
              <div style="display: flex; align-items: center; gap: var(--space-3); margin-bottom: var(--space-3);">
                ${C.avatar(report.reportedUser, 'md')}
                <div>
                  <div style="font-weight: var(--weight-medium);">
                    <a href="#/users/${report.reportedUser}">${report.reportedUser}</a>
                  </div>
                  <div style="font-size: var(--text-xs); color: var(--color-text-tertiary);">
                    LV.${MockData.getUser(report.reportedUser)?.level || 1}
                  </div>
                </div>
              </div>
              <div style="font-size: var(--text-sm); color: var(--color-text-secondary); line-height: 2;">
                <div>B币：${MockData.getUser(report.reportedUser)?.coins || 0}</div>
                <div>贡献值：${MockData.getUser(report.reportedUser)?.contribution || 0}</div>
                <div>历史处罚：1 次警告</div>
              </div>
            </div>
          </div>

          <div class="data-table-wrapper">
            <div style="padding: var(--space-4); border-bottom: var(--border-default); font-weight: var(--weight-semibold);">
              处理操作
            </div>
            <div style="padding: var(--space-4); display: flex; flex-direction: column; gap: var(--space-2);">
              ${report.status === 'pending' ? `
                ${C.button({ text: '受理举报', variant: 'primary', icon: 'check', onClick: `handleReportAction('${report.id}', 'accept', '已受理')`, extraClass: 'btn-block' })}
              ` : ''}
              ${report.status !== 'resolved' && report.status !== 'rejected' ? `
                ${C.button({ text: '隐藏内容', variant: 'secondary', icon: 'eye-off', onClick: `handleReportAction('${report.id}', 'hide', '内容已隐藏')`, extraClass: 'btn-block' })}
                ${C.button({ text: '警告用户', variant: 'secondary', icon: 'alert-triangle', onClick: `handleReportAction('${report.id}', 'warn', '已警告用户')`, extraClass: 'btn-block' })}
                ${C.button({ text: '禁言 7 天', variant: 'secondary', icon: 'mic-off', onClick: `handleReportAction('${report.id}', 'mute', '已禁言 7 天')`, extraClass: 'btn-block' })}
                ${C.button({ text: '封禁账号', variant: 'danger', icon: 'ban', onClick: `handleReportAction('${report.id}', 'ban', '已封禁账号')`, extraClass: 'btn-block' })}
                ${C.button({ text: '驳回举报', variant: 'ghost', icon: 'x', onClick: `handleReportReject('${report.id}')`, extraClass: 'btn-block' })}
              ` : `
                <div class="text-secondary" style="text-align: center; font-size: var(--text-sm); padding: var(--space-4) 0;">
                  举报已${report.status === 'resolved' ? '解决' : '驳回'}
                </div>
              `}
            </div>
          </div>
        </div>
      </div>
    `;

    return adminLayout(content, '/admin/reports');
  };

  // ============================================
  // Admin Points (/admin/points)
  // ============================================
  P.adminPoints = function() {
    const stats = MockData.adminStats;

    const content = `
      <div class="admin-page-header">
        <div>
          <h1 class="admin-page-title">积分与货币</h1>
          <p class="admin-page-desc">管理用户积分和 B币</p>
        </div>
      </div>

      <div class="stats-grid stats-grid-3">
        <div class="stat-card">
          <div class="stat-card-label">${C.icon('coins', 16)} B币总量</div>
          <div class="stat-card-value">${(stats.totalUsers * 100).toLocaleString()}</div>
          <div class="stat-card-change">流通中</div>
        </div>
        <div class="stat-card">
          <div class="stat-card-label">${C.icon('trophy', 16)} 总经验值</div>
          <div class="stat-card-value">${(stats.totalUsers * 500).toLocaleString()}</div>
          <div class="stat-card-change">累计</div>
        </div>
        <div class="stat-card">
          <div class="stat-card-label">${C.icon('award', 16)} 总贡献值</div>
          <div class="stat-card-value">${(stats.totalUsers * 20).toLocaleString()}</div>
          <div class="stat-card-change">累计</div>
        </div>
      </div>

      <div class="data-table-wrapper" style="margin-top: var(--space-4);">
        <div style="padding: var(--space-4); border-bottom: var(--border-default); font-weight: var(--weight-semibold);">
          积分调整
        </div>
        <div style="padding: var(--space-5);">
          <div class="form-grid form-grid-3" style="margin-bottom: var(--space-4);">
            <div class="input-wrapper">
              <label class="input-label">用户名</label>
              <input type="text" class="input-field" id="adjust-username" value="Chaos" placeholder="输入用户名" />
            </div>
            <div class="input-wrapper">
              <label class="input-label">调整类型</label>
              <select class="input-field" id="adjust-type">
                <option value="coins">B币</option>
                <option value="exp">经验值</option>
                <option value="contribution">贡献值</option>
              </select>
            </div>
            <div class="input-wrapper">
              <label class="input-label">调整数量</label>
              <input type="number" class="input-field" id="adjust-amount" value="100" placeholder="正数增加，负数扣除" />
            </div>
          </div>
          <div class="input-wrapper" style="margin-bottom: var(--space-4);">
            <label class="input-label">调整原因</label>
            <input type="text" class="input-field" id="adjust-reason" value="活动奖励" placeholder="请输入调整原因" />
          </div>
          <div style="display: flex; gap: var(--space-2);">
            ${C.button({ text: '增加积分', variant: 'primary', icon: 'plus', onClick: "adjustPoints('add')" })}
            ${C.button({ text: '扣除积分', variant: 'danger', icon: 'minus', onClick: "adjustPoints('sub')" })}
          </div>
        </div>
      </div>

      <div class="data-table-wrapper" style="margin-top: var(--space-4);">
        <div style="padding: var(--space-4); border-bottom: var(--border-default); font-weight: var(--weight-semibold);">
          最近调整记录
        </div>
        <table class="data-table">
          <thead>
            <tr>
              <th>时间</th>
              <th>用户</th>
              <th>类型</th>
              <th>变动</th>
              <th>原因</th>
              <th>操作人</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td class="text-secondary">2026-08-01 15:30</td>
              <td><a href="#/users/Alice">Alice</a></td>
              <td>B币</td>
              <td class="text-success">+50</td>
              <td>精华帖奖励</td>
              <td>Echo</td>
            </tr>
            <tr>
              <td class="text-secondary">2026-07-30 10:00</td>
              <td><a href="#/users/Bob">Bob</a></td>
              <td>经验值</td>
              <td class="text-success">+200</td>
              <td>版务贡献</td>
              <td>Echo</td>
            </tr>
            <tr>
              <td class="text-secondary">2026-07-28 14:20</td>
              <td><a href="#/users/Chaos">Chaos</a></td>
              <td>B币</td>
              <td class="text-danger">-20</td>
              <td>违规扣除</td>
              <td>Echo</td>
            </tr>
          </tbody>
        </table>
      </div>
    `;

    return adminLayout(content, '/admin/points');
  };

  // ============================================
  // Admin Levels (/admin/levels)
  // ============================================
  P.adminLevels = function() {
    const levels = MockData.levels;

    const content = `
      <div class="admin-page-header">
        <div>
          <h1 class="admin-page-title">等级管理</h1>
          <p class="admin-page-desc">管理用户等级体系和权益</p>
        </div>
      </div>

      <div class="data-table-wrapper">
        <table class="data-table">
          <thead>
            <tr>
              <th>等级</th>
              <th>名称</th>
              <th>所需经验</th>
              <th>用户数</th>
              <th>每日发布</th>
              <th>可设可见等级</th>
              <th>单附件上限</th>
              <th>附件总容量</th>
              <th>权益</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            ${levels.map(l => {
              const quota = Store.state.attachmentLevelQuotas[l.level];
              return `
              <tr>
                <td>
                  <div class="level-badge" style="background: ${l.color}; width: 36px; height: 36px; font-size: 12px;">
                    L${l.level}
                  </div>
                </td>
                <td style="font-weight: var(--weight-medium);">${l.name}</td>
                <td><span class="mono">${l.expRequired.toLocaleString()}</span></td>
                <td>${l.userCount.toLocaleString()}</td>
                <td>${l.dailyPosts} 条</td>
                <td><strong>LV.${l.maxVisibilityLevel}</strong></td>
                <td><strong>${quota.maxFileMb} MB</strong></td>
                <td><strong>${quota.totalCapacityMb >= 1024 ? `${quota.totalCapacityMb / 1024} GB` : `${quota.totalCapacityMb} MB`}</strong></td>
                <td>
                  <div class="level-benefits">
                    ${l.benefits.slice(0, 3).map(b => `<span class="level-benefit">${b}</span>`).join('')}
                    ${l.benefits.length > 3 ? `<span class="level-benefit">+${l.benefits.length - 3}</span>` : ''}
                  </div>
                </td>
                <td>
                  <div class="table-actions">
                    <button class="btn btn-ghost btn-sm" onclick="openLevelQuotaDialog(${l.level})">编辑容量</button>
                  </div>
                </td>
              </tr>
            `}).join('')}
          </tbody>
        </table>
      </div>
    `;

    return adminLayout(content, '/admin/levels');
  };

  // ============================================
  // Admin Themes (/admin/themes)
  // ============================================
  P.adminThemes = function() {
    const currentTheme = Store.state.theme;

    const content = `
      <div class="admin-page-header">
        <div>
          <h1 class="admin-page-title">主题管理</h1>
          <p class="admin-page-desc">配置社区主题和外观</p>
        </div>
      </div>

      <div class="data-table-wrapper" style="margin-bottom: var(--space-4);">
        <div style="padding: var(--space-4); border-bottom: var(--border-default); font-weight: var(--weight-semibold);">
          默认主题
        </div>
        <div style="padding: var(--space-5);">
          <div class="theme-preview">
            <div class="theme-card ${currentTheme === 'light' ? 'active' : ''}" data-preview-theme="light" role="button" tabindex="0" aria-pressed="${currentTheme === 'light'}" onclick="Store.setTheme('light'); Router.refresh();" onkeydown="if(event.key==='Enter'||event.key===' '){event.preventDefault();this.click();}">
              <div class="theme-preview-bar">
                <div class="theme-preview-dot" style="background: #FF5F57;"></div>
                <div class="theme-preview-dot" style="background: #FEBC2E;"></div>
                <div class="theme-preview-dot" style="background: #28C840;"></div>
              </div>
              <div class="theme-preview-body">
                <div class="theme-preview-line is-brand" style="width: 80%;"></div>
                <div class="theme-preview-line is-fg short"></div>
                <div class="theme-preview-line is-muted" style="width: 60%;"></div>
              </div>
              <div class="theme-label">亮色模式</div>
            </div>
            <div class="theme-card ${currentTheme === 'dark' ? 'active' : ''}" data-preview-theme="dark" role="button" tabindex="0" aria-pressed="${currentTheme === 'dark'}" onclick="Store.setTheme('dark'); Router.refresh();" onkeydown="if(event.key==='Enter'||event.key===' '){event.preventDefault();this.click();}">
              <div class="theme-preview-bar">
                <div class="theme-preview-dot" style="background: #FF5F57;"></div>
                <div class="theme-preview-dot" style="background: #FEBC2E;"></div>
                <div class="theme-preview-dot" style="background: #28C840;"></div>
              </div>
              <div class="theme-preview-body">
                <div class="theme-preview-line is-brand" style="width: 80%;"></div>
                <div class="theme-preview-line is-fg short"></div>
                <div class="theme-preview-line is-muted" style="width: 60%;"></div>
              </div>
              <div class="theme-label">暗色模式</div>
            </div>
          </div>
        </div>
      </div>

      <div class="data-table-wrapper">
        <div style="padding: var(--space-4); border-bottom: var(--border-default); font-weight: var(--weight-semibold);">
          主题设置
        </div>
        <div style="padding: var(--space-5);">
          <div class="settings-row">
            <div class="settings-row-label">
              <div class="settings-row-title">允许用户切换主题</div>
              <div class="settings-row-desc">用户可在个人设置中切换亮色/暗色主题</div>
            </div>
            ${C.switchEl(true, "Toast.show('设置已更新', 'success')")}
          </div>
          <div class="settings-row">
            <div class="settings-row-label">
              <div class="settings-row-title">跟随系统</div>
              <div class="settings-row-desc">默认跟随系统主题设置</div>
            </div>
            ${C.switchEl(true, "Toast.show('设置已更新', 'success')")}
          </div>
          <div class="settings-row">
            <div class="settings-row-label">
              <div class="settings-row-title">自定义品牌色</div>
              <div class="settings-row-desc">修改社区主色调</div>
            </div>
            <div class="theme-swatches">
              ${['#B23E2A', '#0F756C', '#946200', '#CF222E', '#8250DF'].map((color, index) => `
                <button type="button" class="theme-swatch ${index === 0 ? 'is-active' : ''}" aria-label="选择品牌色 ${color}" style="--swatch-color: ${color};" onclick="Toast.show('品牌色预览已更新', 'info')"></button>
              `).join('')}
            </div>
          </div>
        </div>
      </div>
    `;

    return adminLayout(content, '/admin/themes');
  };

  // ============================================
  // Admin Plugins (/admin/plugins)
  // ============================================
  P.adminPlugins = function() {
    const plugins = Store.state.plugins;

    const content = `
      <div class="admin-page-header">
        <div>
          <h1 class="admin-page-title">插件管理</h1>
          <p class="admin-page-desc">管理社区插件和扩展功能</p>
        </div>
        ${C.button({ text: '安装插件', variant: 'primary', icon: 'plus', onClick: "Toast.show('插件市场开发中', 'info')" })}
      </div>

      <div>
        ${plugins.map(p => `
          <div class="plugin-card">
            <div class="plugin-icon">
              ${C.icon(p.type === 'config' ? 'settings' : 'package', 20)}
            </div>
            <div class="plugin-info">
              <div class="plugin-name">
                ${p.name}
                <span class="plugin-version">v${p.version}</span>
                ${p.status === 'enabled' ? '<span class="badge badge-status-resolved">已启用</span>' : ''}
                ${p.status === 'disabled' ? '<span class="badge badge-status-rejected">已禁用</span>' : ''}
                ${p.status === 'error' ? '<span class="badge badge-priority-high">错误</span>' : ''}
              </div>
              <div class="plugin-desc">${p.description}</div>
              <div class="plugin-caps">
                ${p.capabilities.map(c => `<span class="plugin-cap">${c}</span>`).join('')}
              </div>
            </div>
            <div style="display: flex; flex-direction: column; gap: var(--space-2);">
              ${C.button({ text: p.status === 'enabled' ? '禁用' : '启用', variant: p.status === 'enabled' ? 'secondary' : 'primary', size: 'sm', onClick: `Store.togglePlugin('${p.id}'); Router.refresh();` })}
              ${C.button({ text: '设置', variant: 'ghost', size: 'sm', icon: 'settings', onClick: "Toast.show('插件设置开发中', 'info')" })}
            </div>
          </div>
        `).join('')}
      </div>
    `;

    return adminLayout(content, '/admin/plugins');
  };

  // ============================================
  // Admin OAuth (/admin/oauth)
  // ============================================
  P.adminOAuth = function() {
    const clients = Store.state.oauthClients;

    const content = `
      <div class="admin-page-header">
        <div>
          <h1 class="admin-page-title">OAuth 客户端</h1>
          <p class="admin-page-desc">管理 OAuth 2.0 / OIDC 应用</p>
        </div>
        ${C.button({ text: '创建应用', variant: 'primary', icon: 'plus', onClick: 'showCreateOAuthModal()' })}
      </div>

      <div class="data-table-wrapper">
        <table class="data-table">
          <thead>
            <tr>
              <th>应用名称</th>
              <th>Client ID</th>
              <th>类型</th>
              <th>状态</th>
              <th>授权用户</th>
              <th>创建时间</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            ${clients.map(c => `
              <tr>
                <td style="font-weight: var(--weight-medium);">${c.name}</td>
                <td><span class="mono" style="font-size: var(--text-xs);">${c.clientId}</span></td>
                <td>${c.type === 'public' ? '公开' : '机密'}</td>
                <td>${c.status === 'enabled' ? '<span class="badge badge-status-resolved">已启用</span>' : '<span class="badge badge-status-rejected">已禁用</span>'}</td>
                <td>${c.recentAuthUsers}</td>
                <td class="text-secondary">${c.createdAt}</td>
                <td>
                  <div class="table-actions">
                    <button class="btn btn-ghost btn-sm" onclick="Toast.show('编辑功能开发中', 'info')">编辑</button>
                    <button class="btn btn-ghost btn-sm text-danger" onclick="Toast.show('删除功能开发中', 'info')">删除</button>
                  </div>
                </td>
              </tr>
            `).join('')}
          </tbody>
        </table>
      </div>
    `;

    return adminLayout(content, '/admin/oauth');
  };

  // ============================================
  // Admin Marketplace (/admin/marketplace)
  // ============================================
  P.adminMarketplace = function() {
    const content = `
      <div class="admin-page-header">
        <div>
          <h1 class="admin-page-title">市场与交易</h1>
          <p class="admin-page-desc">审批自建市场接入，监控原子扣款、退款、Webhook 与对账</p>
        </div>
        ${C.button({ text: '接入文档', variant: 'secondary', icon: 'book-open', onClick: "Toast.show('公开交易 API 文档已打开', 'info')" })}
      </div>

      <div class="card" style="margin-bottom:var(--space-4);padding:var(--space-4);display:flex;gap:var(--space-3);border-color:var(--color-warning);background:var(--color-warning-soft);">
        <div>${C.icon('shield-check', 18)}</div>
        <div><strong>安全边界</strong><div class="text-secondary" style="margin-top:var(--space-1);">市场不能直接修改余额或提交可信价格。购买成功只在意图消费、订单、扣款、不可变流水与 Outbox 同一事务提交后返回。</div></div>
      </div>

      <div class="stats-grid stats-grid-3">
        <div class="stat-card"><div class="stat-card-label">${C.icon('shopping-bag', 16)} 今日已提交交易</div><div class="stat-card-value">1,286</div><div class="stat-card-change">成功率 99.72%</div></div>
        <div class="stat-card"><div class="stat-card-label">${C.icon('clock', 16)} 原子提交延迟 P95</div><div class="stat-card-value">84 ms</div><div class="stat-card-change">仅数据库已提交结果</div></div>
        <div class="stat-card"><div class="stat-card-label">${C.icon('webhook', 16)} Outbox 待投递</div><div class="stat-card-value">3</div><div class="stat-card-change">最老 12 秒 · 核心交易不受影响</div></div>
      </div>

      <div class="data-table-wrapper" style="margin-top:var(--space-4);">
        <div style="padding:var(--space-4);border-bottom:var(--border-default);font-weight:var(--weight-semibold);">市场 Client 与风险限额</div>
        <table class="data-table">
          <thead><tr><th>市场</th><th>Client</th><th>交易 Scope</th><th>单笔 / 日限额</th><th>Webhook</th><th>状态</th><th>操作</th></tr></thead>
          <tbody>
            <tr><td><strong>Rust 工坊</strong><div class="text-secondary" style="font-size:var(--text-xs);">所有者 Chaos</div></td><td><span class="mono">mkt_rust_7f3a</span><div class="text-secondary" style="font-size:var(--text-xs);">Confidential</div></td><td><span class="badge badge-neutral">purchase</span> <span class="badge badge-neutral">read</span></td><td>500 / 5,000 B币</td><td>${C.statusBadge('published')} <span class="text-secondary">签名正常</span></td><td>${C.statusBadge('published')}</td><td><div class="table-actions"><button class="btn btn-ghost btn-sm" onclick="Toast.show('市场安全策略已打开', 'info')">审查</button><button class="btn btn-danger btn-sm" onclick="confirmDisableMarketplace('Rust 工坊')">紧急禁用</button></div></td></tr>
            <tr><td><strong>像素素材铺</strong><div class="text-secondary" style="font-size:var(--text-xs);">所有者 Alice</div></td><td><span class="mono">mkt_pixel_2c91</span><div class="text-secondary" style="font-size:var(--text-xs);">Confidential</div></td><td><span class="badge badge-neutral">purchase</span> <span class="badge badge-neutral">refund</span></td><td>200 / 2,000 B币</td><td>${C.badge('待验证', 'warning')}</td><td>${C.badge('待审批', 'warning')}</td><td><button class="btn btn-primary btn-sm" onclick="Toast.show('审批前需完成 Webhook 与所有权验证', 'warning')">审批</button></td></tr>
          </tbody>
        </table>
      </div>

      <div class="data-table-wrapper" style="margin-top:var(--space-4);">
        <div style="padding:var(--space-4);border-bottom:var(--border-default);display:flex;justify-content:space-between;align-items:center;"><strong>实时交易与对账</strong><span class="text-secondary" style="font-size:var(--text-xs);">Purchase + Point operation + Outbox 已核对</span></div>
        <table class="data-table">
          <thead><tr><th>Purchase ID</th><th>市场 / 商户订单</th><th>物品</th><th>金额</th><th>提交耗时</th><th>入账</th><th>Webhook</th></tr></thead>
          <tbody>
            <tr><td><span class="mono">pur_019af31c</span></td><td>Rust 工坊<div class="text-secondary mono" style="font-size:var(--text-xs);">ORD-20260803-1042</div></td><td>Axum 部署手册</td><td><strong>-32 B币</strong></td><td>71 ms</td><td>${C.badge('原子提交', 'success')}</td><td>${C.badge('已签收', 'success')}</td></tr>
            <tr><td><span class="mono">pur_019af2e8</span></td><td>Rust 工坊<div class="text-secondary mono" style="font-size:var(--text-xs);">ORD-20260803-1038</div></td><td>代码审查额度包</td><td><strong>-80 B币</strong></td><td>93 ms</td><td>${C.badge('原子提交', 'success')}</td><td>${C.badge('重试中', 'warning')}</td></tr>
            <tr><td><span class="mono">ref_019af19d</span></td><td>Rust 工坊<div class="text-secondary mono" style="font-size:var(--text-xs);">ORD-20260802-0881</div></td><td>补偿退款</td><td><strong class="text-success">+32 B币</strong></td><td>66 ms</td><td>${C.badge('补偿流水', 'success')}</td><td>${C.badge('已签收', 'success')}</td></tr>
          </tbody>
        </table>
      </div>
    `;
    return adminLayout(content, '/admin/marketplace');
  };

  // ============================================
  // Admin Settings (/admin/settings)
  // ============================================
  P.adminSettings = function() {
    const content = `
      <div class="admin-page-header">
        <div>
          <h1 class="admin-page-title">系统设置</h1>
          <p class="admin-page-desc">配置社区全局参数</p>
        </div>
      </div>

      <div class="data-table-wrapper" style="margin-bottom: var(--space-4);">
        <div style="padding: var(--space-4); border-bottom: var(--border-default); font-weight: var(--weight-semibold);">
          基本设置
        </div>
        <div style="padding: var(--space-5);">
          <div class="settings-row">
            <div class="settings-row-label">
              <div class="settings-row-title">站点名称</div>
              <div class="settings-row-desc">显示在页面标题和导航栏</div>
            </div>
            <input type="text" class="input-field admin-setting-input" value="BBLBB" />
          </div>
          <div class="settings-row">
            <div class="settings-row-label">
              <div class="settings-row-title">站点描述</div>
              <div class="settings-row-desc">用于 SEO 和社交分享</div>
            </div>
            <input type="text" class="input-field admin-setting-input" value="技术爱好者的社区" />
          </div>
          <div class="settings-row">
            <div class="settings-row-label">
              <div class="settings-row-title">开放注册</div>
              <div class="settings-row-desc">允许新用户注册账号</div>
            </div>
            ${C.switchEl(true, "Toast.show('设置已更新', 'success')")}
          </div>
          <div class="settings-row">
            <div class="settings-row-label">
              <div class="settings-row-title">注册需要审核</div>
              <div class="settings-row-desc">新用户注册后需管理员审核</div>
            </div>
            ${C.switchEl(false, "Toast.show('设置已更新', 'success')")}
          </div>
        </div>
      </div>

      <div class="data-table-wrapper">
        <div style="padding: var(--space-4); border-bottom: var(--border-default); font-weight: var(--weight-semibold);">
          内容设置
        </div>
        <div style="padding: var(--space-5);">
          <div class="settings-row">
            <div class="settings-row-label">
              <div class="settings-row-title">每日发帖上限</div>
              <div class="settings-row-desc">普通用户每日最多发帖数</div>
            </div>
            <input type="number" class="input-field admin-setting-input admin-setting-input-number" value="10" />
          </div>
          <div class="settings-row">
            <div class="settings-row-label">
              <div class="settings-row-title">敏感词过滤</div>
              <div class="settings-row-desc">自动过滤违规内容</div>
            </div>
            ${C.switchEl(true, "Toast.show('设置已更新', 'success')")}
          </div>
        </div>
        <div class="card-footer" style="display: flex; justify-content: flex-end; gap: var(--space-2);">
          ${C.button({ text: '保存设置', variant: 'primary', onClick: "Toast.show('设置已保存', 'success')" })}
        </div>
      </div>
    `;

    return adminLayout(content, '/admin/settings');
  };

  // ============================================
  // Global handlers for admin
  // ============================================
  window.openLevelQuotaDialog = function(level) {
    const quota = Store.state.attachmentLevelQuotas[level];
    Modal.open({
      title: `编辑 LV.${level} 附件容量`,
      content: `
        <div class="form-grid form-grid-2">
          <div class="input-wrapper"><label class="input-label" for="level-max-file">单附件上限（MB）</label><input type="number" min="1" max="1024" class="input-field" id="level-max-file" value="${quota.maxFileMb}" /></div>
          <div class="input-wrapper"><label class="input-label" for="level-total-capacity">附件总容量（MB）</label><input type="number" min="1" max="1048576" class="input-field" id="level-total-capacity" value="${quota.totalCapacityMb}" /></div>
        </div>
        <div class="input-hint" style="margin-top:var(--space-3);">总容量不能小于单附件上限。保存后新上传立即按该等级额度校验。</div>
      `,
      confirmText: '保存容量',
      onConfirm: () => {
        const saved = Store.updateAttachmentLevelQuota(level, {
          maxFileMb: document.getElementById('level-max-file')?.value,
          totalCapacityMb: document.getElementById('level-total-capacity')?.value
        });
        Toast.show(`LV.${level} 附件容量已更新`, 'success');
        Router.refresh();
        return saved;
      }
    });
  };

  window.handleReportAction = function(reportId, actionType, successMsg) {
    Modal.open({
      title: '确认操作',
      content: `<p>确定执行此操作吗？</p>`,
      confirmText: '确认',
      variant: actionType === 'ban' || actionType === 'hide' ? 'danger' : 'primary',
      onConfirm: () => {
        Store.addReportHistory(reportId, {
          type: actionType,
          operator: Store.state.user.name,
          reason: successMsg
        });
        if (actionType === 'accept') {
          Store.updateReport(reportId, { status: 'processing' });
        } else if (actionType === 'warn' || actionType === 'mute' || actionType === 'ban' || actionType === 'hide') {
          Store.updateReport(reportId, { status: 'resolved' });
        }
        Toast.show(successMsg, 'success');
        Router.refresh();
      }
    });
  };

  window.handleReportReject = function(reportId) {
    Modal.open({
      title: '驳回举报',
      content: `
        <div class="input-wrapper">
          <label class="input-label">驳回原因</label>
          <input type="text" class="input-field" id="reject-reason" value="经核实不构成违规" placeholder="请输入驳回原因" />
        </div>
      `,
      confirmText: '确认驳回',
      variant: 'secondary',
      onConfirm: () => {
        const reason = document.getElementById('reject-reason')?.value || '经核实不构成违规';
        Store.addReportHistory(reportId, {
          type: 'report',
          operator: Store.state.user.name,
          reason: reason
        });
        Store.updateReport(reportId, { status: 'rejected' });
        Toast.show('举报已驳回', 'info');
        Router.refresh();
      }
    });
  };

  window.adjustPoints = function(type) {
    const username = document.getElementById('adjust-username')?.value;
    const adjType = document.getElementById('adjust-type')?.value;
    let amount = parseInt(document.getElementById('adjust-amount')?.value) || 0;
    const reason = document.getElementById('adjust-reason')?.value;

    if (!username) {
      Toast.show('请输入用户名', 'warning');
      return;
    }
    if (amount <= 0) {
      Toast.show('请输入有效数量', 'warning');
      return;
    }
    if (type === 'sub') amount = -amount;

    const typeLabel = adjType === 'coins' ? 'B币' : adjType === 'exp' ? '经验值' : '贡献值';
    Modal.open({
      title: '确认调整',
      content: `<p>确定为用户 <strong>${username}</strong> ${type === 'add' ? '增加' : '扣除'} ${Math.abs(amount)} ${typeLabel}吗？</p><p style="font-size: var(--text-sm); color: var(--color-text-secondary);">原因：${reason}</p>`,
      confirmText: '确认',
      variant: type === 'sub' ? 'danger' : 'primary',
      onConfirm: () => {
        if (adjType === 'coins') {
          Store.adjustCoins(amount, reason);
        } else {
          Toast.show(`${typeLabel}已${type === 'add' ? '增加' : '扣除'}`, 'success');
        }
      }
    });
  };

  window.toggleVideoOption = function(key) {
    if (!['enabled', 'directEnabled', 'hlsEnabled', 'xiguaEnabled'].includes(key)) return;
    Store.updateVideoConfig({ [key]: !Store.state.videoConfig[key] });
    Router.refresh();
  };

  window.saveVideoConfig = function() {
    const saved = Store.updateVideoConfig({
      maxDurationSeconds: document.getElementById('video-max-duration')?.value,
      hlsMaxSegments: document.getElementById('video-hls-segments')?.value,
      hlsMaxBytesMb: document.getElementById('video-hls-bytes')?.value
    });
    Toast.show(`视频策略已保存（策略 v${saved.policyVersion}）`, 'success');
    Router.refresh();
  };

  window.testVideoPolicy = function() {
    Toast.show('已使用固定安全样本排队测试 URL、重定向、私网地址和 HLS playlist', 'info');
  };

  window.toggleAiGateway = function() {
    Store.updateAiConfig({ enabled: !Store.state.aiConfig.enabled });
    Router.refresh();
  };

  window.saveAiConfig = function() {
    const saved = Store.updateAiConfig({
      providerName: document.getElementById('ai-provider')?.value.trim(),
      defaultModel: document.getElementById('ai-model')?.value.trim(),
      baseUrl: document.getElementById('ai-base-url')?.value.trim(),
      secret: document.getElementById('ai-secret')?.value,
      dataMode: document.getElementById('ai-data-mode')?.value,
      dailyBudget: document.getElementById('ai-budget')?.value,
      timeoutSeconds: document.getElementById('ai-timeout')?.value
    });
    Toast.show(`AI 配置已保存（策略 v${saved.policyVersion}）`, 'success');
    Router.refresh();
  };

  window.testAiProvider = function() {
    Toast.show('已发送脱敏探针，Provider 连接测试任务已排队', 'info');
  };

  window.toggleDownloadBilling = function() {
    Store.updateDownloadBillingConfig({ enabled: !Store.state.downloadBillingConfig.enabled });
    Router.refresh();
  };

  window.saveDownloadBilling = function() {
    const cfg = Store.state.downloadBillingConfig;
    const saved = Store.updateDownloadBillingConfig({
      defaultPrice: document.getElementById('download-default-price')?.value,
      authorizationTtlHours: document.getElementById('download-auth-ttl')?.value,
      dailyUserLimit: document.getElementById('download-daily-limit')?.value,
      maxSingleCharge: document.getElementById('download-max-charge')?.value
    });
    if (saved.maxSingleCharge < saved.defaultPrice) {
      Toast.show('单次扣费上限不能低于默认价格', 'warning');
      return false;
    }
    Toast.show('下载扣费策略已保存，新授权立即生效', 'success');
    Router.refresh();
    return true;
  };

  window.setStorageBackend = function(backend) {
    Store.updateStorageConfig({ backend });
    Router.refresh();
  };

  window.toggleStorageOption = function(key) {
    Store.updateStorageConfig({ [key]: !Store.state.storageConfig[key], backend: Store.state.storageConfig.backend });
    Router.refresh();
  };

  function collectStorageConfig() {
    const current = Store.state.storageConfig;
    return {
      backend: current.backend,
      localPath: document.getElementById('storage-local-path')?.value.trim() || current.localPath,
      endpoint: document.getElementById('storage-endpoint')?.value.trim() || current.endpoint,
      region: document.getElementById('storage-region')?.value.trim() || current.region,
      bucket: document.getElementById('storage-bucket')?.value.trim() || current.bucket,
      accessKeyId: document.getElementById('storage-access-key')?.value.trim() || current.accessKeyId,
      secretAccessKey: document.getElementById('storage-secret')?.value || '',
      publicBaseUrl: document.getElementById('storage-public-url')?.value.trim() || '',
      maxUploadMb: document.getElementById('storage-max-upload')?.value || current.maxUploadMb,
      signedUrlTtl: document.getElementById('storage-signed-ttl')?.value || current.signedUrlTtl
    };
  }

  window.saveStorageConfig = function() {
    const config = collectStorageConfig();
    if (config.backend === 's3' && (!config.endpoint || !config.region || !config.bucket || !config.accessKeyId || (!config.secretAccessKey && !Store.state.storageConfig.secretConfigured))) {
      Toast.show('请完整填写 S3 连接信息', 'warning');
      return false;
    }
    Store.updateStorageConfig(config);
    Toast.show('存储配置已保存，切换后端前请先迁移已有对象', 'success');
    Router.refresh();
    return true;
  };

  window.testStorageConfig = function() {
    if (!saveStorageConfig()) return;
    const ok = Store.testStorageConnection();
    Toast.show(ok ? '存储连接测试成功' : '连接失败，请检查 Endpoint、Bucket 和凭证', ok ? 'success' : 'danger');
    Router.refresh();
  };

  window.confirmDisableMarketplace = function(name) {
    Modal.open({
      title: '紧急禁用市场',
      content: `<p>禁用 <strong>${C.escapeHtml(name)}</strong> 后，将立即停止新 Token、结账意图、购买与退款。已提交交易和账本不会删除，仍可继续对账。</p><p class="text-danger" style="margin-top:var(--space-3);">这是安全隔离操作，不会回滚历史交易。</p>`,
      confirmText: '确认紧急禁用',
      variant: 'danger',
      onConfirm: () => Toast.show(`${name} 已禁用，新交易已停止`, 'success')
    });
  };

  window.showCreateOAuthModal = function() {
    Modal.open({
      title: '创建 OAuth 应用',
      content: `
        <div style="display: flex; flex-direction: column; gap: var(--space-4);">
          <div class="input-wrapper">
            <label class="input-label">应用名称</label>
            <input type="text" class="input-field" id="oauth-name" placeholder="应用名称" />
          </div>
          <div class="input-wrapper">
            <label class="input-label">应用类型</label>
            <select class="input-field" id="oauth-type">
              <option value="public">公开 (Public)</option>
              <option value="confidential">机密 (Confidential)</option>
            </select>
          </div>
          <div class="input-wrapper">
            <label class="input-label">Redirect URI</label>
            <input type="text" class="input-field" id="oauth-redirect" placeholder="https://example.com/callback" />
          </div>
          <div class="input-wrapper">
            <label class="input-label">授权范围</label>
            <input type="text" class="input-field" id="oauth-scopes" value="openid profile email" placeholder="空格分隔" />
          </div>
        </div>
      `,
      confirmText: '创建应用',
      variant: 'primary',
      onConfirm: () => {
        const name = document.getElementById('oauth-name')?.value;
        const type = document.getElementById('oauth-type')?.value;
        const redirectUri = document.getElementById('oauth-redirect')?.value;
        const scopes = document.getElementById('oauth-scopes')?.value.split(' ').filter(Boolean);

        if (!name || !redirectUri) {
          Toast.show('请填写完整信息', 'warning');
          return false;
        }

        const clientId = name.toLowerCase().replace(/\s+/g, '-') + '-app';
        Store.addOAuthClient({
          name,
          clientId,
          type,
          status: 'enabled',
          redirectUri,
          postLogoutRedirectUri: redirectUri.replace('/callback', '/logout'),
          scopes
        });
        Router.refresh();
      }
    });
  };

})();
