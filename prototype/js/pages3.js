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
      <div class="admin-page-header">
        <div>
          <h1 class="admin-page-title">仪表盘</h1>
          <p class="admin-page-desc">社区运营数据概览</p>
        </div>
        <div style="display: flex; gap: var(--space-2);">
          ${C.button({ text: Store.state.theme === 'dark' ? '亮色模式' : '暗色模式', variant: 'secondary', icon: Store.state.theme === 'dark' ? 'sun' : 'moon', onClick: 'Store.toggleTheme(); Router.refresh();' })}
        </div>
      </div>

      <div class="stats-grid">
        ${statCards.map(s => `
          <div class="stat-card">
            <div class="stat-card-label">
              ${C.icon(s.icon, 16)} ${s.label}
            </div>
            <div class="stat-card-value">${s.value}</div>
            <div class="stat-card-change ${s.negative ? 'negative' : ''}">${s.change}</div>
          </div>
        `).join('')}
      </div>

      <div class="admin-split-grid">
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

      <div class="data-table-wrapper" style="margin-top: var(--space-4);">
        <div style="padding: var(--space-4); border-bottom: var(--border-default); display: flex; justify-content: space-between; align-items: center;">
          <span style="font-weight: var(--weight-semibold);">存储使用</span>
        </div>
        <div style="padding: var(--space-5);">
          <div style="display: flex; justify-content: space-between; font-size: var(--text-sm); margin-bottom: var(--space-2);">
            <span class="text-secondary">已使用</span>
            <span>${stats.storageUsed} GB / ${stats.storageTotal} GB</span>
          </div>
          <div style="height: 8px; background: var(--color-bg-subtle); border-radius: var(--radius-full); overflow: hidden;">
            <div style="width: ${(stats.storageUsed / stats.storageTotal * 100)}%; height: 100%; background: var(--color-brand); border-radius: var(--radius-full);"></div>
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
            <form onsubmit="event.preventDefault(); Router.updateAdminParams({ q: this.querySelector('input').value });" style="display:flex; gap:var(--space-2);">
              <input class="input-field" value="${C.escapeHtml(params.q || '')}" placeholder="搜索用户名或简介" style="width:240px;" />
              ${C.button({ text: '搜索', variant: 'secondary', size: 'sm', icon: 'search' })}
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
      <div class="admin-page-header"><div><h1 class="admin-page-title">内容管理</h1><p class="admin-page-desc">审核、推荐和维护社区发布内容</p></div></div>
      <div class="stats-grid" style="margin-bottom:var(--space-4);">
        <div class="stat-card"><div class="stat-card-label">全部内容</div><div class="stat-card-value">${MockData.posts.length}</div><div class="stat-card-change">帖子与文章</div></div>
        <div class="stat-card"><div class="stat-card-label">待审核</div><div class="stat-card-value">${MockData.adminStats.pendingPosts}</div><div class="stat-card-change negative">需要处理</div></div>
        <div class="stat-card"><div class="stat-card-label">精华内容</div><div class="stat-card-value">${MockData.posts.filter(p => p.isEssence).length}</div><div class="stat-card-change">社区精选</div></div>
        <div class="stat-card"><div class="stat-card-label">今日发布</div><div class="stat-card-value">18</div><div class="stat-card-change">较昨日 +3</div></div>
      </div>
      <div class="data-table-wrapper">
        <div class="filter-bar"><div class="filter-left">
          <select class="filter-select" onchange="Router.updateAdminParams({ type: this.value })"><option value="all">全部类型</option><option value="topic" ${type === 'topic' ? 'selected' : ''}>讨论帖</option><option value="article" ${type === 'article' ? 'selected' : ''}>专栏文章</option></select>
          <select class="filter-select" onchange="Router.updateAdminParams({ board: this.value })"><option value="all">全部板块</option>${MockData.boards.map(b => `<option value="${b.slug}" ${board === b.slug ? 'selected' : ''}>${b.name}</option>`).join('')}</select>
        </div><span class="text-secondary" style="font-size:var(--text-sm);">共 ${posts.length} 条</span></div>
        <table class="data-table"><thead><tr><th>内容</th><th>类型</th><th>板块</th><th>作者</th><th>数据</th><th>发布时间</th><th>操作</th></tr></thead>
          <tbody>${posts.map(post => `<tr><td style="max-width:360px;"><a href="#/topics/${post.id}" style="font-weight:var(--weight-medium);">${post.title}</a><div style="margin-top:var(--space-1);">${post.isPinned ? C.badge('置顶', 'pinned') : ''} ${post.isEssence ? C.badge('精华', 'essence') : ''}</div></td><td>${post.type === 'article' ? '文章' : '讨论'}</td><td>${C.categoryBadge(post.board)}</td><td><a href="#/users/${post.author}">${post.author}</a></td><td class="text-secondary">${post.replies} 回复 · ${post.views} 浏览</td><td class="text-secondary">${post.createdAt}</td><td><div class="table-actions"><a class="btn btn-ghost btn-sm" href="#/topics/${post.id}">查看</a><button class="btn btn-ghost btn-sm" onclick="Toast.show('审核状态已更新', 'success')">审核</button></div></td></tr>`).join('')}</tbody>
        </table>
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
    return adminSimpleTable('标签管理', '维护标签、分组和内容关联', ['标签', '分组', '使用次数', '最近使用', '操作'], MockData.tags.map(tag => [C.tag(tag.name), '技术主题', tag.count, '今天', '<button class="btn btn-ghost btn-sm">编辑</button>']), '/admin/tags', '新建标签');
  };

  P.adminAttachments = function() {
    return adminSimpleTable('附件管理', '所有附件都有有效期；到期后禁止访问并由任务清理', ['文件', '大小', '上传者 / 等级', '配额占用', '到期时间', '状态'], [
      ['sqlite-wal.png', '286 KB', `Chaos · ${C.levelBadge(6)}`, '638 MB / 2 GB', '2027-01-30 · 还剩 180 天', C.statusBadge('published')],
      ['svelte-arch.png', '42 KB', `Alice · ${C.levelBadge(4)}`, '126 MB / 500 MB', '2026-10-02 · 还剩 60 天', C.badge('待检查', 'warning')],
      ['oauth-flow.pdf', '1.2 MB', `Bob · ${C.levelBadge(5)}`, '814 MB / 1 GB', '2026-08-10 · 还剩 7 天', C.badge('即将到期', 'warning')]
    ], '/admin/attachments', '上传附件');
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
            <div class="input-wrapper"><label class="input-label" for="storage-signed-ttl">签名 URL 有效期（秒）</label><input type="number" min="60" max="3600" class="input-field" id="storage-signed-ttl" value="${cfg.signedUrlTtl}" /></div>
            <div class="input-wrapper"><label class="input-label" for="storage-default-ttl">默认附件有效期（天）</label><input type="number" min="1" max="365" class="input-field" id="storage-default-ttl" value="${cfg.defaultAttachmentTtlDays}" /><div class="input-hint">所有附件必须设置到期时间，不能永久保存。</div></div>
            <div class="input-wrapper"><label class="input-label" for="storage-max-ttl">站点最长有效期（天）</label><input type="number" min="1" max="3650" class="input-field" id="storage-max-ttl" value="${cfg.maxAttachmentTtlDays}" /></div>
          </div>
          <div class="notice notice-warning" style="margin-top:var(--space-4);">${C.icon('alert-triangle', 16)} 所有附件到期后立即禁止新访问，随后进入宽限期和异步清理。Bucket 应保持私有；切换存储后端不会自动迁移已有对象。</div>
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
                ${C.button({ text: '受理举报', variant: 'primary', icon: 'check', onClick: `handleReportAction('${report.id}', 'accept', '已受理')`, style: 'width: 100%;' })}
              ` : ''}
              ${report.status !== 'resolved' && report.status !== 'rejected' ? `
                ${C.button({ text: '隐藏内容', variant: 'secondary', icon: 'eye-off', onClick: `handleReportAction('${report.id}', 'hide', '内容已隐藏')`, style: 'width: 100%;' })}
                ${C.button({ text: '警告用户', variant: 'secondary', icon: 'alert-triangle', onClick: `handleReportAction('${report.id}', 'warn', '已警告用户')`, style: 'width: 100%;' })}
                ${C.button({ text: '禁言 7 天', variant: 'secondary', icon: 'mic-off', onClick: `handleReportAction('${report.id}', 'mute', '已禁言 7 天')`, style: 'width: 100%;' })}
                ${C.button({ text: '封禁账号', variant: 'danger', icon: 'ban', onClick: `handleReportAction('${report.id}', 'ban', '已封禁账号')`, style: 'width: 100%;' })}
                ${C.button({ text: '驳回举报', variant: 'ghost', icon: 'x', onClick: `handleReportReject('${report.id}')`, style: 'width: 100%;' })}
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

      <div class="stats-grid" style="grid-template-columns: repeat(3, 1fr);">
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
              <th>单附件上限</th>
              <th>附件总容量</th>
              <th>最长有效期</th>
              <th>权益</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            ${levels.map(l => `
              <tr>
                <td>
                  <div class="level-badge" style="background: ${l.color}; width: 36px; height: 36px; font-size: 12px;">
                    L${l.level}
                  </div>
                </td>
                <td style="font-weight: var(--weight-medium);">${l.name}</td>
                <td><span class="mono">${l.expRequired.toLocaleString()}</span></td>
                <td>${l.userCount.toLocaleString()}</td>
                <td><strong>${l.attachmentMaxMb} MB</strong></td>
                <td><strong>${l.attachmentTotalMb >= 1024 ? `${l.attachmentTotalMb / 1024} GB` : `${l.attachmentTotalMb} MB`}</strong></td>
                <td>${l.attachmentTtlDays} 天</td>
                <td>
                  <div class="level-benefits">
                    ${l.benefits.slice(0, 3).map(b => `<span class="level-benefit">${b}</span>`).join('')}
                    ${l.benefits.length > 3 ? `<span class="level-benefit">+${l.benefits.length - 3}</span>` : ''}
                  </div>
                </td>
                <td>
                  <div class="table-actions">
                    <button class="btn btn-ghost btn-sm" onclick="Toast.show('编辑功能开发中', 'info')">编辑</button>
                  </div>
                </td>
              </tr>
            `).join('')}
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
            <div class="theme-card ${currentTheme === 'light' ? 'active' : ''}" onclick="Store.setTheme('light'); Router.refresh();">
              <div class="theme-preview-bar" style="background: #FFFFFF; border-color: #D1D9E0;">
                <div class="theme-preview-dot" style="background: #FF5F57;"></div>
                <div class="theme-preview-dot" style="background: #FEBC2E;"></div>
                <div class="theme-preview-dot" style="background: #28C840;"></div>
              </div>
              <div class="theme-preview-body" style="background: #F6F8FA;">
                <div class="theme-preview-line" style="background: #0969DA; width: 80%;"></div>
                <div class="theme-preview-line short" style="background: #1F2328;"></div>
                <div class="theme-preview-line" style="background: #D1D9E0; width: 60%; height: 6px;"></div>
              </div>
              <div class="theme-label">亮色模式</div>
            </div>
            <div class="theme-card ${currentTheme === 'dark' ? 'active' : ''}" onclick="Store.setTheme('dark'); Router.refresh();">
              <div class="theme-preview-bar" style="background: #151B23; border-color: #3D444D;">
                <div class="theme-preview-dot" style="background: #FF5F57;"></div>
                <div class="theme-preview-dot" style="background: #FEBC2E;"></div>
                <div class="theme-preview-dot" style="background: #28C840;"></div>
              </div>
              <div class="theme-preview-body" style="background: #0D1117;">
                <div class="theme-preview-line" style="background: #4493F8; width: 80%;"></div>
                <div class="theme-preview-line short" style="background: #F0F6FC;"></div>
                <div class="theme-preview-line" style="background: #3D444D; width: 60%; height: 6px;"></div>
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
            <div style="display: flex; gap: var(--space-2);">
              ${['#0969DA', '#1A7F37', '#9A6700', '#CF222E', '#8250DF'].map(color => `
                <div style="width: 28px; height: 28px; border-radius: var(--radius-sm); background: ${color}; cursor: pointer; border: 2px solid ${color === '#0969DA' ? 'var(--color-text-primary)' : 'transparent'}; transition: all var(--duration-fast);" onclick="Toast.show('品牌色已更新', 'success')"></div>
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
            <input type="text" class="input-field" value="BBLBB" style="width: 200px;" />
          </div>
          <div class="settings-row">
            <div class="settings-row-label">
              <div class="settings-row-title">站点描述</div>
              <div class="settings-row-desc">用于 SEO 和社交分享</div>
            </div>
            <input type="text" class="input-field" value="技术爱好者的社区" style="width: 200px;" />
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
            <input type="number" class="input-field" value="10" style="width: 100px;" />
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
      defaultAttachmentTtlDays: document.getElementById('storage-default-ttl')?.value || current.defaultAttachmentTtlDays,
      maxAttachmentTtlDays: document.getElementById('storage-max-ttl')?.value || current.maxAttachmentTtlDays,
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
