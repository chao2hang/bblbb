// ============================================================
// BBLBB UI — Composites: 复合组件（由 Atoms 组合）
// 纯渲染函数；交互用内联 onclick 委托到 Store/Router（原型阶段）
// ============================================================
window.Composites = (function () {
  'use strict';
  const A = window.Atoms;
  const { icon, avatar, button, tag, badge, categoryBadge, formatCount, escapeHtml } = A;

  // ============================================================
  // Navbar（Geist 式：60px，border-bottom + 轻阴影）
  // ============================================================
  function navbar(activePath) {
    const user = window.Store.state.user;
    const unread = window.Store.getUnreadCount();
    const navItems = [
      { label: '首页', href: '#/' },
      { label: '文章', href: '#/articles' },
      { label: '板块', href: '#/boards' },
      { label: '标签', href: '#/tags' }
    ];

    function isActive(href) {
      const p = href.replace('#', '');
      if (p === '/') return activePath === '/';
      return activePath.startsWith(p);
    }

    return `
      <header class="navbar">
        <div class="container nav-container">
          <div class="nav-left">
            <button class="mobile-menu-btn" onclick="window.Store.toggleMobileDrawer()" aria-label="菜单">${icon('menu', 20)}</button>
            <a href="#/" class="nav-logo">BBLBB</a>
            <nav class="nav-items" aria-label="主导航">
              ${navItems.map((item) => `
                <a href="${item.href}" class="nav-link ${isActive(item.href) ? 'is-active' : ''}" ${isActive(item.href) ? 'aria-current="page"' : ''}>${escapeHtml(item.label)}</a>
              `).join('')}
            </nav>
          </div>
          <div class="nav-center">
            <form class="search-form" role="search" onsubmit="event.preventDefault(); window.Router.navigate('/search?q=' + encodeURIComponent(this.querySelector('input').value.trim()));">
              <button type="submit" class="search-submit" aria-label="提交搜索">
                ${icon('search', 17)}
              </button>
              <input type="search" name="q" placeholder="搜索帖子、用户、标签…" class="search-input" aria-label="搜索帖子、用户和标签" autocomplete="off" />
            </form>
          </div>
          <div class="nav-right">
            ${button({ text: '发布', variant: 'primary', size: 'sm', icon: 'pen-line', href: '#/publish' })}
            <a href="#/notifications" class="nav-icon-btn" aria-label="通知">
              ${icon('bell', 18)}
              ${unread > 0 ? `<span class="nav-notif-dot">${unread > 99 ? '99+' : unread}</span>` : ''}
            </a>
            <div class="user-menu-wrapper">
              <button class="user-avatar-btn" onclick="event.stopPropagation(); window.Store.toggleUserMenu()" aria-label="用户菜单">
                ${avatar(user.name, 'md')}
              </button>
              ${window.Store.state.userMenuOpen ? userMenuDropdown(user) : ''}
            </div>
          </div>
        </div>
      </header>
      ${window.Store.state.mobileDrawerOpen ? mobileDrawer(user, navItems) : ''}
    `;
  }

  function userMenuDropdown(user) {
    return `
      <div class="dropdown user-menu" onclick="event.stopPropagation()">
        <div class="user-menu-header">
          ${avatar(user.name, 'lg')}
          <div>
            <div class="user-menu-name">${escapeHtml(user.name)}</div>
            <div class="user-menu-level">LV.${escapeHtml(user.level)} · ${formatCount(user.coins)} B币</div>
          </div>
        </div>
        <div class="dropdown-sep"></div>
        <a href="#/users/${escapeHtml(user.name)}" class="dropdown-item" onclick="window.Store.closeUserMenu()">${icon('user', 16)}<span>我的主页</span></a>
        <a href="#/users/${escapeHtml(user.name)}?tab=points" class="dropdown-item" onclick="window.Store.closeUserMenu()">${icon('coins', 16)}<span>我的积分</span></a>
        <a href="#/favorites" class="dropdown-item" onclick="window.Store.closeUserMenu()">${icon('heart', 16)}<span>收藏</span></a>
        <a href="#/notifications" class="dropdown-item" onclick="window.Store.closeUserMenu()">${icon('bell', 16)}<span>通知</span></a>
        <div class="dropdown-sep"></div>
        <a href="#/settings" class="dropdown-item" onclick="window.Store.closeUserMenu()">${icon('settings', 16)}<span>账号设置</span></a>
        <a href="#/settings?tab=oauth" class="dropdown-item" onclick="window.Store.closeUserMenu()">${icon('key', 16)}<span>OAuth 授权管理</span></a>
        <a href="#/admin" class="dropdown-item" onclick="window.Store.closeUserMenu()">${icon('shield', 16)}<span>管理后台</span></a>
        <div class="dropdown-sep"></div>
        <button class="dropdown-item is-danger" onclick="window.Store.closeUserMenu(); window.Router.navigate('/login');">${icon('log-out', 16)}<span>退出登录</span></button>
      </div>`;
  }

  function mobileDrawer(user, navItems) {
    return `
      <div class="drawer-overlay" onclick="window.Store.closeMobileDrawer()">
        <div class="drawer" onclick="event.stopPropagation()" role="dialog" aria-modal="true" aria-label="菜单">
          <div class="drawer-header">
            <span class="drawer-title">菜单</span>
            <button class="drawer-close" onclick="window.Store.closeMobileDrawer()" aria-label="关闭菜单">${icon('x', 20)}</button>
          </div>
          <div class="drawer-user">
            ${avatar(user.name, 'lg')}
            <div>
              <div class="drawer-username">${escapeHtml(user.name)}</div>
              <div class="drawer-userlevel">LV.${escapeHtml(user.level)} · ${formatCount(user.coins)} B币</div>
            </div>
          </div>
          <nav class="drawer-nav" aria-label="移动端导航">
            ${navItems.map((item) => `<a href="${item.href}" class="drawer-nav-item" onclick="window.Store.closeMobileDrawer()">${escapeHtml(item.label)}</a>`).join('')}
            <div class="drawer-divider"></div>
            <a href="#/users/${escapeHtml(user.name)}" class="drawer-nav-item" onclick="window.Store.closeMobileDrawer()">我的主页</a>
            <a href="#/favorites" class="drawer-nav-item" onclick="window.Store.closeMobileDrawer()">收藏</a>
            <a href="#/notifications" class="drawer-nav-item" onclick="window.Store.closeMobileDrawer()">通知</a>
            <a href="#/settings" class="drawer-nav-item" onclick="window.Store.closeMobileDrawer()">账号设置</a>
            <a href="#/settings?tab=oauth" class="drawer-nav-item" onclick="window.Store.closeMobileDrawer()">OAuth 授权管理</a>
            <a href="#/admin" class="drawer-nav-item" onclick="window.Store.closeMobileDrawer()">管理后台</a>
            <div class="drawer-divider"></div>
            <button class="drawer-nav-item is-danger" onclick="window.Store.closeMobileDrawer(); window.Router.navigate('/login');">退出登录</button>
          </nav>
        </div>
      </div>`;
  }

  // ============================================================
  // Topic List（对齐表格：主题 | 参与者 | 回复 | 浏览 | 活动）
  // ============================================================
  function postList(posts, options) {
    const opts = options || {};
    if (!posts || posts.length === 0) {
      return A.emptyState(opts.empty || { icon: 'message-square', title: '暂无帖子', desc: '成为第一个发帖的人吧！' });
    }
    return `
      <div class="post-list" role="table" aria-label="帖子列表">
        <div class="post-list-head" role="row">
          <div class="post-list-head-cell is-main" role="columnheader">主题</div>
          <div class="post-list-head-cell is-posters" role="columnheader">参与者</div>
          <div class="post-list-head-cell" role="columnheader">回复</div>
          <div class="post-list-head-cell" role="columnheader">浏览</div>
          <div class="post-list-head-cell" role="columnheader">活动</div>
        </div>
        ${posts.map((p) => postRow(p)).join('')}
      </div>`;
  }

  function postRow(post) {
    const posterNames = [post.author];
    if (post.lastReplyBy && post.lastReplyBy !== post.author) posterNames.push(post.lastReplyBy);
    const posters = posterNames.slice(0, 3);
    return `
      <div class="post-row" role="row">
        <div class="post-row-main" role="cell">
          <div class="post-row-title">
            ${post.isPinned ? badge('置顶', 'pinned') : ''}
            ${post.isEssence ? badge('精华', 'essence') : ''}
            <a href="#/topics/${post.id}">${escapeHtml(post.title)}</a>
          </div>
          <div class="post-row-meta">
            ${categoryBadge(post.board)}
            ${(post.tags || []).map((t) => tag(t, null, '#/tags/' + encodeURIComponent(t))).join('')}
          </div>
        </div>
        <div class="post-row-posters" role="cell">
          ${posters.map((n) => `<a href="#/users/${encodeURIComponent(n)}" title="${escapeHtml(n)}">${avatar(n, 'xs')}</a>`).join('')}
        </div>
        <div class="post-row-num ${post.replies >= 20 ? 'is-hot' : ''}" role="cell">${formatCount(post.replies)}</div>
        <div class="post-row-num" role="cell">${formatCount(post.views)}</div>
        <div class="post-row-activity" role="cell">${escapeHtml(post.lastReplyAt || post.createdAt || '')}</div>
      </div>`;
  }

  // ============================================================
  // Article Card（16:9 渐变封面 + 信息）
  // ============================================================
  function articleCard(post) {
    const author = window.MockData.getUser(post.author);
    const board = window.MockData.getBoard(post.board);
    const cover = (board && board.color) || '#0969DA';
    return `
      <a href="#/topics/${post.id}" class="article-card">
        <div class="article-card-cover" style="background:linear-gradient(135deg, ${escapeHtml(cover)} 0%, ${escapeHtml(cover)}cc 55%, #24292F 130%);">
          ${icon('file-text', 36)}
        </div>
        <div class="article-card-body">
          <div class="article-card-title">${escapeHtml(post.title)}</div>
          <div class="article-card-summary">${escapeHtml(post.summary)}</div>
          <div class="article-card-footer">
            <div class="article-card-author">
              ${avatar(post.author, 'xs')}
              <span>${escapeHtml(post.author)}</span>
            </div>
            <span class="article-card-reads">${formatCount(post.views)} 阅读</span>
          </div>
        </div>
      </a>`;
  }

  // ============================================================
  // Board Card（轻卡片：彩色条 + 图标 + 统计）
  // ============================================================
  function boardCard(board) {
    return `
      <a href="#/boards/${encodeURIComponent(board.slug)}" class="board-card" style="--cat-color:${escapeHtml(board.color || '#0969DA')};">
        <div class="board-card-icon">${icon(board.icon, 20)}</div>
        <div class="board-card-name">${escapeHtml(board.name)}</div>
        <div class="board-card-desc">${escapeHtml(board.description)}</div>
        <div class="board-card-stats">
          <span><strong>${formatCount(board.postCount)}</strong> 帖子</span>
          <span><strong>${board.todayCount}</strong> 今日</span>
        </div>
      </a>`;
  }

  // ============================================================
  // Tabs（Primer 式下划线）
  // ============================================================
  function tabs(items, active, onChange = 'Router.setTab') {
    return `
      <div class="tabs" role="tablist">
        ${items.map((item) => {
          const key = typeof item === 'string' ? item : item.key;
          const label = typeof item === 'string' ? item : item.label;
          const count = typeof item === 'object' && item.count !== undefined ? item.count : null;
          return `<button type="button" role="tab" aria-selected="${key === active ? 'true' : 'false'}" class="tab ${key === active ? 'is-active' : ''}" onclick="${escapeHtml(onChange)}('${escapeHtml(key)}')">${escapeHtml(label)}${count !== null ? `<span class="tab-count">${escapeHtml(count)}</span>` : ''}</button>`;
        }).join('')}
      </div>`;
  }

  // ============================================================
  // Pagination
  // ============================================================
  function pagination(current, total, onChange = 'Router.setPage') {
    const pages = [];
    const maxVisible = 5;
    let start = Math.max(1, current - Math.floor(maxVisible / 2));
    let end = Math.min(total, start + maxVisible - 1);
    if (end - start + 1 < maxVisible) start = Math.max(1, end - maxVisible + 1);

    let html = '<nav class="pagination" aria-label="分页">';
    html += `<button type="button" class="page-btn" ${current === 1 ? 'disabled' : ''} onclick="${escapeHtml(onChange)}(${current - 1})" aria-label="上一页">${icon('chevron-left', 14)}</button>`;
    if (start > 1) {
      html += `<button type="button" class="page-btn" onclick="${escapeHtml(onChange)}(1)">1</button>`;
      if (start > 2) html += `<span class="page-ellipsis">…</span>`;
    }
    for (let i = start; i <= end; i++) {
      html += `<button type="button" class="page-btn ${i === current ? 'is-active' : ''}" ${i === current ? 'aria-current="page"' : ''} onclick="${escapeHtml(onChange)}(${i})">${i}</button>`;
    }
    if (end < total) {
      if (end < total - 1) html += `<span class="page-ellipsis">…</span>`;
      html += `<button type="button" class="page-btn" onclick="${escapeHtml(onChange)}(${total})">${total}</button>`;
    }
    html += `<button type="button" class="page-btn" ${current === total ? 'disabled' : ''} onclick="${escapeHtml(onChange)}(${current + 1})" aria-label="下一页">${icon('chevron-right', 14)}</button>`;
    html += '</nav>';
    return html;
  }

  // ============================================================
  // Breadcrumb
  // ============================================================
  function breadcrumb(items) {
    return `
      <nav class="breadcrumb" aria-label="面包屑">
        ${items.map((item, i) => `
          ${i > 0 ? `<span class="breadcrumb-sep">${icon('chevron-right', 12)}</span>` : ''}
          ${i === items.length - 1
            ? `<span class="breadcrumb-current">${escapeHtml(item.label)}</span>`
            : `<a class="breadcrumb-link" href="${escapeHtml(item.href || '#')}">${escapeHtml(item.label)}</a>`}
        `).join('')}
      </nav>`;
  }

  // ============================================================
  // User Info Card（侧栏）
  // ============================================================
  function userInfoCard(user) {
    return `
      <div class="card sidebar-card">
        <div class="sidebar-card-user">
          ${avatar(user.name, 'xl')}
          <div class="sidebar-card-name">${escapeHtml(user.name)}</div>
          <div class="sidebar-card-badges">${A.roleBadge(user.roles)}${A.levelBadge(user.level)}</div>
        </div>
        ${A.levelProgress(user.exp, user.expNext, user.level)}
        <div class="sidebar-card-stats">
          <div class="sidebar-card-stat"><span class="stat-num">${formatCount(user.coins)}</span><span class="stat-label">B币</span></div>
          <div class="sidebar-card-stat"><span class="stat-num">${formatCount(user.contribution || 0)}</span><span class="stat-label">贡献</span></div>
          <div class="sidebar-card-stat"><span class="stat-num">${user.likes || 0}</span><span class="stat-label">获赞</span></div>
        </div>
      </div>`;
  }

  // ============================================================
  // Restricted Content Card（受限内容，绝不渲染正文进 DOM）
  // ============================================================
  function restrictedCard(post) {
    const r = post.restricted;
    if (!r) return '';
    let hint = '';
    if (r.type === 'level') hint = `达到 LV.${r.level} 后可见`;
    else if (r.type === 'reply') hint = '回复本主题后可见';
    else if (r.type === 'paid') hint = `支付 ${r.price} B币永久解锁`;
    return `
      <div class="restricted-card">
        <div class="restricted-icon">${icon('lock', 24)}</div>
        <div class="restricted-text">${escapeHtml(hint)}</div>
        ${r.type === 'paid'
          ? button({ text: `支付 ${r.price} B币解锁`, variant: 'primary', size: 'sm', onClick: `window.handlePayUnlock(${post.id}, ${r.price})` })
          : r.type === 'reply'
            ? button({ text: '去回复', variant: 'primary', size: 'sm', onClick: "document.getElementById('reply-textarea')?.focus(); document.getElementById('reply-editor')?.scrollIntoView({behavior:'smooth'})" })
            : button({ text: `需要 LV.${r.level}`, variant: 'secondary', size: 'sm', disabled: true })}
      </div>`;
  }

  // ============================================================
  // Admin Sidebar
  // ============================================================
  function adminSidebar(activePath) {
    const groups = [
      {
        label: '概览', items: [
          { key: '/admin', label: '仪表盘', icon: 'monitor' }
        ]
      },
      {
        label: '用户权限', items: [
          { key: '/admin/users', label: '用户管理', icon: 'users' },
          { key: '/admin/roles', label: '角色与权限', icon: 'shield' },
          { key: '/admin/levels', label: '等级管理', icon: 'trophy' }
        ]
      },
      {
        label: '内容治理', items: [
          { key: '/admin/boards', label: '板块管理', icon: 'globe' },
          { key: '/admin/posts', label: '帖子与回复', icon: 'file-text' },
          { key: '/admin/reports', label: '举报与审核', icon: 'flag' },
          { key: '/admin/tags', label: '标签管理', icon: 'tag' }
        ]
      },
      {
        label: '运营', items: [
          { key: '/admin/points', label: '积分与货币', icon: 'coins' },
          { key: '/admin/attachments', label: '附件管理', icon: 'image' },
          { key: '/admin/storage', label: '文件存储', icon: 'package' },
          { key: '/admin/notifications', label: '通知与邮件', icon: 'bell' },
          { key: '/admin/audit', label: '审计日志', icon: 'list' }
        ]
      },
      {
        label: '平台', items: [
          { key: '/admin/themes', label: '主题管理', icon: 'eye' },
          { key: '/admin/plugins', label: '插件管理', icon: 'package' },
          { key: '/admin/oauth', label: 'OAuth 客户端', icon: 'key' },
          { key: '/admin/settings', label: '系统设置', icon: 'settings' }
        ]
      }
    ];
    return `
      <aside class="admin-sidebar" aria-label="后台导航">
        ${groups.map((g) => `
          <div class="admin-sidebar-group">
            <div class="admin-sidebar-label">${escapeHtml(g.label)}</div>
            ${g.items.map((it) => `
              <a href="#${escapeHtml(it.key)}" class="admin-sidebar-item ${(activePath === it.key || (it.key === '/admin/posts' && activePath === '/admin/content')) ? 'is-active' : ''}" ${(activePath === it.key || (it.key === '/admin/posts' && activePath === '/admin/content')) ? 'aria-current="page"' : ''}>
                ${icon(it.icon, 16)}<span>${escapeHtml(it.label)}</span>
              </a>`).join('')}
          </div>`).join('')}
      </aside>`;
  }

  // ============================================================
  // TOC（文章目录，从 markdown 源提取 h2/h3）
  // ============================================================
  function toc(headings) {
    if (!headings || !headings.length) return '';
    return `
      <nav class="toc" aria-label="文章目录">
        <div class="toc-title">目录</div>
        <div class="toc-list">
          ${headings.map((h) => `
            <a class="toc-item toc-item--${h.level}" href="#${escapeHtml(h.id)}">${escapeHtml(h.text)}</a>`).join('')}
        </div>
      </nav>`;
  }

  // 从 markdown 提取 ## / ### 标题（用于目录与锚点）
  function extractHeadings(md) {
    if (!md) return [];
    return md.split('\n')
      .map((line) => line.match(/^(#{2,3})\s+(.+)$/))
      .filter(Boolean)
      .map((m) => {
        const level = m[1].length === 2 ? 2 : 3;
        const text = m[2].trim();
        return { level, text, id: 'sec-' + encodeURIComponent(text.replace(/\s+/g, '-')) };
      });
  }

  // ============================================================
  // renderMarkdown（保持既有渲染语义）
  // ============================================================
  function renderMarkdown(text) {
    if (!text) return '';
    let html = text.trim();

    html = html.replace(/```(\w*)\n([\s\S]*?)```/g, (m, lang, code) =>
      `<pre><code class="language-${escapeHtml(lang)}">${escapeHtml(code.trim())}</code></pre>`);
    html = html.replace(/`([^`]+)`/g, '<code>$1</code>');

    html = html.replace(/^### (.+)$/gm, '<h3>$1</h3>');
    html = html.replace(/^### (.+)$/gm, (m, t) => `<h3 id="sec-${encodeURIComponent(t.trim().replace(/\s+/g, '-'))}">${t}</h3>`);
    html = html.replace(/^## (.+)$/gm, (m, t) => `<h2 id="sec-${encodeURIComponent(t.trim().replace(/\s+/g, '-'))}">${t}</h2>`);
    html = html.replace(/^# (.+)$/gm, '<h1>$1</h1>');
    html = html.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
    html = html.replace(/^> (.+)$/gm, '<blockquote>$1</blockquote>');

    html = html.replace(/\|(.+)\|\n\|[-| :]+\|\n((?:\|.+\|\n?)+)/g, (m, header, body) => {
      const headers = header.split('|').map((h) => h.trim()).filter(Boolean);
      const rows = body.trim().split('\n').map((row) => row.split('|').map((c) => c.trim()).filter(Boolean));
      return `<table><thead><tr>${headers.map((h) => `<th>${h}</th>`).join('')}</tr></thead><tbody>${rows.map((r) => `<tr>${r.map((c) => `<td>${c}</td>`).join('')}</tr>`).join('')}</tbody></table>`;
    });

    html = html.replace(/^- (.+)$/gm, '<li>$1</li>');
    html = html.replace(/^(\d+)\. (.+)$/gm, '<li value="$1">$2</li>');

    const lines = html.split('\n');
    const result = [];
    let paraBuffer = [];
    const flushPara = () => { if (paraBuffer.length) { result.push('<p>' + paraBuffer.join('<br>') + '</p>'); paraBuffer = []; } };

    for (const line of lines) {
      const t = line.trim();
      if (!t) { flushPara(); continue; }
      if (/^<h|^<pre|^<table|^<\/pre|^<\/table|^<li|^<blockquote|^<\/blockquote|^<thead|^<\/thead|^<tbody|^<\/tbody|^<tr|^<\/tr/.test(t)) { flushPara(); result.push(line); continue; }
      paraBuffer.push(t);
    }
    flushPara();

    return result.join('\n').replace(/(<li>.+<\/li>\n?)+/g, (m) => `<ul>${m}</ul>`);
  }

  return {
    navbar, mobileDrawer, userMenuDropdown,
    postList, postRow, articleCard, boardCard,
    tabs, pagination, breadcrumb, userInfoCard,
    restrictedCard, adminSidebar, renderMarkdown,
    toc, extractHeadings
  };
})();
