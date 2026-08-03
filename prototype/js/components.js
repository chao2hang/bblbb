// ============================================
// BBLBB UI Components (HTML string builders)
// ============================================

window.Components = (function() {

  // --- Avatar ---
  function avatar(name, size = 'md') {
    const initial = name ? name.charAt(0).toUpperCase() : '?';
    return `<div class="avatar avatar-${size}" title="${name}">${initial}</div>`;
  }

  // --- Icon (Lucide) ---
  function icon(name, size = 16, extra = '') {
    return `<i data-lucide="${name}" size="${size}" ${extra}></i>`;
  }

  // --- Button ---
  function button({ text = '', variant = 'primary', size = 'md', icon: iconName = '', iconOnly = false, onClick = '', href = '', disabled = false, id = '', extraClass = '' }) {
    const classes = `btn btn-${variant} btn-${size} ${iconOnly ? 'btn-icon' : ''} ${extraClass}`.trim();
    const iconHtml = iconName ? `<i data-lucide="${iconName}" size="${size === 'sm' ? 14 : 16}"></i>` : '';
    const attrs = [
      `class="${classes}"`,
      disabled ? 'disabled' : '',
      id ? `id="${id}"` : '',
      onClick ? `onclick="${onClick}"` : ''
    ].filter(Boolean).join(' ');

    if (href) {
      return `<a href="${href}" ${attrs}>${iconHtml}${iconOnly ? '' : text}</a>`;
    }
    return `<button ${attrs}>${iconHtml}${iconOnly ? '' : text}</button>`;
  }

  // --- Tag ---
  function tag(name, count = null, href = null) {
    const tagContent = `${name}${count !== null ? `<span class="tag-count">${count}</span>` : ''}`;
    if (href) {
      return `<a class="tag" href="${href}">${tagContent}</a>`;
    }
    return `<span class="tag">${tagContent}</span>`;
  }

  // --- Badge ---
  function badge(text, type = 'level') {
    return `<span class="badge badge-${type}">${text}</span>`;
  }

  function levelBadge(level) {
    return `<span class="badge badge-level">LV.${level}</span>`;
  }

  function roleBadge(roles) {
    if (roles.includes('admin')) return `<span class="badge badge-role-admin">管理员</span>`;
    if (roles.includes('moderator')) return `<span class="badge badge-role-mod">版主</span>`;
    return '';
  }

  function statusBadge(status) {
    const map = {
      pending: ['待处理', 'pending'],
      processing: ['处理中', 'processing'],
      resolved: ['已解决', 'resolved'],
      rejected: ['已驳回', 'rejected']
    };
    const [text, type] = map[status] || [status, 'pending'];
    return `<span class="badge badge-status-${type}">${text}</span>`;
  }

  function priorityBadge(priority) {
    const map = {
      high: ['高优先级', 'high'],
      medium: ['中优先级', 'medium'],
      low: ['低优先级', 'low']
    };
    const [text, type] = map[priority] || [priority, 'low'];
    return `<span class="badge badge-priority-${type}">${text}</span>`;
  }

  // --- Level Progress ---
  function levelProgress(exp, expNext, level) {
    const prevExp = level > 1 ? MockData.levels[level - 2]?.expRequired || 0 : 0;
    const currentRange = expNext - prevExp;
    const progress = currentRange > 0 ? ((exp - prevExp) / currentRange * 100) : 0;
    return `
      <div class="level-progress">
        <div class="level-progress-header">
          <span>LV.${level}</span>
          <span>${exp} / ${expNext} EXP</span>
        </div>
        <div class="level-progress-bar">
          <div class="level-progress-fill" style="width: ${Math.min(100, Math.max(0, progress))}%"></div>
        </div>
      </div>
    `;
  }

  // --- Navbar ---
  function navbar(activePath) {
    const user = Store.state.user;
    const unread = Store.getUnreadCount();
    const navItems = [
      { label: '首页', href: '#/' },
      { label: '文章', href: '#/articles' },
      { label: '板块', href: '#/boards' },
      { label: '标签', href: '#/tags' }
    ];

    function isActive(href) {
      const hashPath = href.replace('#', '');
      if (hashPath === '/') return activePath === '/';
      return activePath.startsWith(hashPath);
    }

    return `
      <header class="navbar">
        <div class="nav-container">
          <div class="nav-left">
            <button class="mobile-menu-btn" onclick="Store.toggleMobileDrawer()" aria-label="菜单">
              ${icon('menu', 20)}
            </button>
            <a href="#/" class="nav-logo">BBLBB</a>
            <nav class="nav-items">
              ${navItems.map(item => `
                <a href="${item.href}" class="nav-link ${isActive(item.href) ? 'active' : ''}">${item.label}</a>
              `).join('')}
            </nav>
          </div>
          <div class="nav-center">
            <form class="search-form" onsubmit="event.preventDefault(); Router.navigate('/search?q=' + encodeURIComponent(this.querySelector('input').value));">
              ${icon('search', 16, 'class="search-icon"')}
              <input type="text" placeholder="搜索帖子、用户、标签..." class="search-input" />
            </form>
          </div>
          <div class="nav-right">
            <a href="#/publish" class="publish-btn">
              ${icon('pen-line', 16)}
              <span>发布</span>
            </a>
            <a href="#/notifications" class="nav-icon-btn" aria-label="通知">
              ${icon('bell', 20)}
              ${unread > 0 ? `<span class="notification-badge">${unread}</span>` : ''}
            </a>
            <div class="user-menu-wrapper">
              <button class="user-avatar-btn" onclick="event.stopPropagation(); Store.toggleUserMenu()" aria-label="用户菜单">
                ${avatar(user.name, 'md')}
              </button>
              ${Store.state.userMenuOpen ? userMenuDropdown(user) : ''}
            </div>
          </div>
        </div>
      </header>
      ${Store.state.mobileDrawerOpen ? mobileDrawer(user, navItems) : ''}
    `;
  }

  function userMenuDropdown(user) {
    return `
      <div class="user-menu-dropdown" onclick="event.stopPropagation()">
        <div class="user-menu-header">
          ${avatar(user.name, 'lg')}
          <div>
            <div class="user-menu-name">${user.name}</div>
            <div class="user-menu-level">LV.${user.level}</div>
          </div>
        </div>
        <div class="user-menu-divider"></div>
        <a href="#/users/${user.name}" class="user-menu-item" onclick="Store.closeUserMenu()">
          ${icon('user', 16)}
          <span>我的主页</span>
        </a>
        <a href="#/favorites" class="user-menu-item" onclick="Store.closeUserMenu()">
          ${icon('heart', 16)}
          <span>收藏</span>
        </a>
        <a href="#/notifications" class="user-menu-item" onclick="Store.closeUserMenu()">
          ${icon('bell', 16)}
          <span>通知</span>
        </a>
        <div class="user-menu-divider"></div>
        <a href="#/settings" class="user-menu-item" onclick="Store.closeUserMenu()">
          ${icon('settings', 16)}
          <span>账号设置</span>
        </a>
        <a href="#/admin" class="user-menu-item" onclick="Store.closeUserMenu()">
          ${icon('shield', 16)}
          <span>管理后台</span>
        </a>
        <div class="user-menu-divider"></div>
        <button class="user-menu-item danger" onclick="Store.closeUserMenu(); Router.navigate('/login');">
          ${icon('log-out', 16)}
          <span>退出登录</span>
        </button>
      </div>
    `;
  }

  function mobileDrawer(user, navItems) {
    return `
      <div class="mobile-drawer-overlay" onclick="Store.closeMobileDrawer()">
        <div class="mobile-drawer" onclick="event.stopPropagation()">
          <div class="drawer-header">
            <span class="drawer-title">菜单</span>
            <button class="drawer-close" onclick="Store.closeMobileDrawer()">
              ${icon('x', 20)}
            </button>
          </div>
          <div class="drawer-user">
            ${avatar(user.name, 'lg')}
            <div>
              <div class="drawer-username">${user.name}</div>
              <div class="drawer-userlevel">LV.${user.level} · ${user.coins} B币</div>
            </div>
          </div>
          <nav class="drawer-nav">
            ${navItems.map(item => `
              <a href="${item.href}" class="drawer-nav-item" onclick="Store.closeMobileDrawer()">${item.label}</a>
            `).join('')}
            <div class="drawer-divider"></div>
            <a href="#/users/${user.name}" class="drawer-nav-item" onclick="Store.closeMobileDrawer()">我的主页</a>
            <a href="#/favorites" class="drawer-nav-item" onclick="Store.closeMobileDrawer()">收藏</a>
            <a href="#/notifications" class="drawer-nav-item" onclick="Store.closeMobileDrawer()">通知</a>
            <a href="#/settings" class="drawer-nav-item" onclick="Store.closeMobileDrawer()">账号设置</a>
            <a href="#/admin" class="drawer-nav-item" onclick="Store.closeMobileDrawer()">管理后台</a>
            <div class="drawer-divider"></div>
            <button class="drawer-nav-item danger" onclick="Store.closeMobileDrawer(); Router.navigate('/login');">退出登录</button>
          </nav>
        </div>
      </div>
    `;
  }

  // --- Category Badge (colored square + name, Discourse-style) ---
  function categoryBadge(slug, variant) {
    const board = MockData.getBoard(slug);
    const color = board?.color || '#919191';
    const cls = 'category-badge' + (variant ? ' is-' + variant : '');
    return `
      <a href="#/boards/${slug}" class="${cls}" style="--cat-color: ${color};">
        <span class="category-badge-square"></span>
        <span>${board?.name || slug}</span>
      </a>
    `;
  }

  // --- Compact number formatting for list columns (1.2k) ---
  function formatCount(n) {
    if (typeof n !== 'number') return n;
    if (n >= 1000) {
      const v = n / 1000;
      return (v >= 10 ? Math.round(v) : v.toFixed(1).replace(/\.0$/, '')) + 'k';
    }
    return String(n);
  }

  // --- Topic List (Discourse-style table with column header) ---
  function postList(posts, options) {
    const opts = options || {};
    if (!posts || posts.length === 0) {
      return emptyState(opts.empty || {
        icon: 'message-square',
        title: '暂无帖子',
        desc: '成为第一个发帖的人吧！'
      });
    }
    return `
      <div class="post-list">
        <div class="post-list-head">
          <div class="post-list-head-cell is-main">主题</div>
          <div class="post-list-head-cell is-posters">参与者</div>
          <div class="post-list-head-cell">回复</div>
          <div class="post-list-head-cell">浏览</div>
          <div class="post-list-head-cell">活动</div>
        </div>
        ${posts.map(p => postRow(p)).join('')}
      </div>
    `;
  }

  // --- Post Row (Discourse-style aligned table row) ---
  function postRow(post) {
    // Poster cluster: author first, then up to 2 distinct repliers
    const posterNames = [post.author];
    if (post.lastReplyBy && post.lastReplyBy !== post.author) posterNames.push(post.lastReplyBy);
    const posters = posterNames.slice(0, 3);

    return `
      <div class="post-row">
        <div class="post-row-main">
          <div class="post-row-title">
            ${post.isPinned ? badge('置顶', 'pinned') : ''}
            ${post.isEssence ? badge('精华', 'essence') : ''}
            <a href="#/topics/${post.id}">${post.title}</a>
          </div>
          <div class="post-row-meta">
            ${categoryBadge(post.board)}
            ${post.tags && post.tags.length > 0
              ? post.tags.map(t => tag(t, null, '#/tags/' + encodeURIComponent(t))).join('')
              : ''}
          </div>
        </div>
        <div class="post-row-posters">
          ${posters.map(n => `<a href="#/users/${n}" title="${n}">${avatar(n, 'sm')}</a>`).join('')}
        </div>
        <div class="post-row-num ${post.replies >= 20 ? 'is-hot' : ''}">${formatCount(post.replies)}</div>
        <div class="post-row-num">${formatCount(post.views)}</div>
        <div class="post-row-activity">${post.lastReplyAt || post.createdAt}</div>
      </div>
    `;
  }

  // --- Article Card ---
  function articleCard(post) {
    const author = MockData.getUser(post.author);
    return `
      <a href="#/topics/${post.id}" class="article-card">
        <div class="article-card-cover">
          ${icon('file-text', 48)}
        </div>
        <div class="article-card-body">
          <div class="article-card-title">${post.title}</div>
          <div class="article-card-summary">${post.summary}</div>
          <div class="article-card-footer">
            <div class="article-card-author">
              ${avatar(post.author, 'sm')}
              <span>${post.author}</span>
            </div>
            <span>${post.views} 阅读</span>
          </div>
        </div>
      </a>
    `;
  }

  // --- Board Card ---
  function boardCard(board) {
    return `
      <a href="#/boards/${board.slug}" class="board-card">
        <div class="board-card-header">
          <div class="board-card-icon">
            ${icon(board.icon, 20)}
          </div>
          <div>
            <div class="board-card-name">${board.name}</div>
            <div class="board-card-desc">${board.description}</div>
          </div>
        </div>
        <div class="board-card-stats">
          <span class="board-card-stat"><strong>${board.postCount}</strong> 帖子</span>
          <span class="board-card-stat"><strong>${board.todayCount}</strong> 今日</span>
        </div>
      </a>
    `;
  }

  // --- Pagination ---
  function pagination(current, total, onChange = 'Router.setPage') {
    const pages = [];
    const maxVisible = 5;
    let start = Math.max(1, current - Math.floor(maxVisible / 2));
    let end = Math.min(total, start + maxVisible - 1);
    if (end - start + 1 < maxVisible) {
      start = Math.max(1, end - maxVisible + 1);
    }

    let html = '<div class="pagination">';
    html += `<button class="page-btn" ${current === 1 ? 'disabled' : ''} onclick="${onChange}(${current - 1})">${icon('chevron-left', 14)}</button>`;
    
    if (start > 1) {
      html += `<button class="page-btn" onclick="${onChange}(1)">1</button>`;
      if (start > 2) html += `<span style="padding: 0 4px; color: var(--color-text-tertiary);">...</span>`;
    }
    
    for (let i = start; i <= end; i++) {
      html += `<button class="page-btn ${i === current ? 'active' : ''}" onclick="${onChange}(${i})">${i}</button>`;
    }
    
    if (end < total) {
      if (end < total - 1) html += `<span style="padding: 0 4px; color: var(--color-text-tertiary);">...</span>`;
      html += `<button class="page-btn" onclick="${onChange}(${total})">${total}</button>`;
    }
    
    html += `<button class="page-btn" ${current === total ? 'disabled' : ''} onclick="${onChange}(${current + 1})">${icon('chevron-right', 14)}</button>`;
    html += '</div>';
    return html;
  }

  // --- Empty State ---
  function emptyState({ icon: iconName = 'inbox', title = '暂无内容', desc = '' }) {
    return `
      <div class="empty-state">
        <div class="empty-icon">${icon(iconName, 64)}</div>
        <div class="empty-title">${title}</div>
        ${desc ? `<div class="empty-desc">${desc}</div>` : ''}
      </div>
    `;
  }

  // --- Skeleton ---
  function skeleton(width = '100%', height = '16px') {
    return `<div class="skeleton" style="width: ${width}; height: ${height};"></div>`;
  }

  // --- Breadcrumb ---
  function breadcrumb(items) {
    return `
      <div class="breadcrumb">
        ${items.map((item, i) => `
          ${i > 0 ? `<span class="breadcrumb-sep">${icon('chevron-right', 12)}</span>` : ''}
          ${i === items.length - 1
            ? `<span class="breadcrumb-current">${item.label}</span>`
            : `<a href="${item.href}">${item.label}</a>`
          }
        `).join('')}
      </div>
    `;
  }

  // --- User Info Card (sidebar) ---
  function userInfoCard(user) {
    return `
      <div class="card">
        <div class="user-card">
          ${avatar(user.name, 'xl')}
          <div class="user-card-name">${user.name}</div>
          <div class="user-card-bio">${user.bio}</div>
          ${levelBadge(user.level)}
          ${levelProgress(user.exp, user.expNext, user.level)}
          <div class="user-card-stats">
            <div class="user-card-stat">
              <div class="user-card-stat-num">${user.coins}</div>
              <div class="user-card-stat-label">B币</div>
            </div>
            <div class="user-card-stat">
              <div class="user-card-stat-num">${user.contribution}</div>
              <div class="user-card-stat-label">贡献值</div>
            </div>
            <div class="user-card-stat">
              <div class="user-card-stat-num">${MockData.posts.filter(p => p.author === user.name).length}</div>
              <div class="user-card-stat-label">帖子</div>
            </div>
          </div>
        </div>
      </div>
    `;
  }

  // --- Tabs ---
  function tabs(items, active, onChange = 'Router.setTab') {
    return `
      <div class="tabs">
        ${items.map(item => `
          <button class="tab ${item.key === active ? 'active' : ''}" onclick="${onChange}('${item.key}')">
            ${item.icon ? icon(item.icon, 16) : ''}
            <span>${item.label}</span>
            ${item.count !== undefined ? `<span style="font-size: 11px; color: var(--color-text-tertiary);">${item.count}</span>` : ''}
          </button>
        `).join('')}
      </div>
    `;
  }

  // --- Restricted Content Card ---
  function restrictedCard(post) {
    const r = post.restricted;
    const user = Store.state.user;

    // Check if unlocked
    let unlocked = false;
    if (r.type === 'reply' && Store.isReplyUnlocked(post.id)) unlocked = true;
    if (r.type === 'paid' && Store.isPaidUnlocked(post.id)) unlocked = true;
    if (r.type === 'level' && user.level >= r.level) unlocked = true;

    if (unlocked) {
      return `
        <div class="restricted-unlocked">
          <div class="prose">${renderMarkdown(r.content)}</div>
        </div>
      `;
    }

    let body = '';
    if (r.type === 'reply') {
      body = `
        <p class="restricted-text">回复本主题后可见</p>
        <button class="btn btn-primary" onclick="document.querySelector('.reply-editor textarea').focus();">
          ${icon('message-square', 16)}
          回复后查看
        </button>
      `;
    } else if (r.type === 'level') {
      const diff = Math.max(0, r.level - user.level);
      body = `
        <div class="restricted-level-info">
          ${icon('shield', 16)}
          <span>达到 LV.${r.level} 或回复后可见</span>
        </div>
        <p class="restricted-text">当前等级 LV.${user.level}，还差 ${diff} 级</p>
        <button class="btn btn-primary" onclick="document.querySelector('.reply-editor textarea').focus();">
          ${icon('message-square', 16)}
          去回复解锁
        </button>
      `;
    } else if (r.type === 'paid') {
      const canAfford = user.coins >= r.price;
      body = `
        <p class="restricted-text">支付 ${r.price} B币永久解锁</p>
        <div style="font-size: var(--text-sm); color: var(--color-text-secondary);">
          当前余额：<strong style="color: var(--color-text-primary);">${user.coins} B币</strong>
        </div>
        <button class="btn btn-primary" ${!canAfford ? 'disabled' : ''} onclick="handlePayUnlock(${post.id}, ${r.price})">
          ${icon('coins', 16)}
          支付 ${r.price} B币解锁
        </button>
        ${!canAfford ? `<p class="restricted-error">余额不足，还差 ${r.price - user.coins} B币</p>` : ''}
      `;
    }

    return `
      <div class="restricted-card">
        <div class="restricted-icon">${icon('lock', 24)}</div>
        ${body}
      </div>
    `;
  }

  // --- Simple Markdown Renderer ---
  function renderMarkdown(text) {
    if (!text) return '';
    let html = text.trim();
    
    // Code blocks
    html = html.replace(/```(\w*)\n([\s\S]*?)```/g, (match, lang, code) => {
      return `<pre><code class="language-${lang}">${escapeHtml(code.trim())}</code></pre>`;
    });
    
    // Inline code
    html = html.replace(/`([^`]+)`/g, '<code>$1</code>');
    
    // Headers
    html = html.replace(/^### (.+)$/gm, '<h3>$1</h3>');
    html = html.replace(/^## (.+)$/gm, '<h2>$1</h2>');
    html = html.replace(/^# (.+)$/gm, '<h1>$1</h1>');
    
    // Bold
    html = html.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
    
    // Blockquote
    html = html.replace(/^> (.+)$/gm, '<blockquote>$1</blockquote>');
    
    // Tables (simple)
    html = html.replace(/\|(.+)\|\n\|[-| :]+\|\n((?:\|.+\|\n?)+)/g, (match, header, body) => {
      const headers = header.split('|').map(h => h.trim()).filter(Boolean);
      const rows = body.trim().split('\n').map(row => 
        row.split('|').map(c => c.trim()).filter(Boolean)
      );
      return `<table>
        <thead><tr>${headers.map(h => `<th>${h}</th>`).join('')}</tr></thead>
        <tbody>${rows.map(r => `<tr>${r.map(c => `<td>${c}</td>`).join('')}</tr>`).join('')}</tbody>
      </table>`;
    });
    
    // Lists
    html = html.replace(/^- (.+)$/gm, '<li>$1</li>');
    html = html.replace(/^(\d+)\. (.+)$/gm, '<li value="$1">$2</li>');
    
    // Wrap paragraphs (lines that aren't tags and not empty)
    const lines = html.split('\n');
    let inList = false;
    let inBlockquote = false;
    let result = [];
    let paraBuffer = [];
    
    function flushPara() {
      if (paraBuffer.length > 0) {
        result.push('<p>' + paraBuffer.join('<br>') + '</p>');
        paraBuffer = [];
      }
    }
    
    for (let line of lines) {
      const trimmed = line.trim();
      if (!trimmed) {
        flushPara();
        continue;
      }
      if (trimmed.startsWith('<h') || trimmed.startsWith('<pre') || trimmed.startsWith('<table') ||
          trimmed.startsWith('</pre') || trimmed.startsWith('</table') || trimmed.startsWith('<li') ||
          trimmed.startsWith('<blockquote') || trimmed.startsWith('</blockquote') ||
          trimmed.startsWith('<thead') || trimmed.startsWith('</thead') ||
          trimmed.startsWith('<tbody') || trimmed.startsWith('</tbody') ||
          trimmed.startsWith('<tr') || trimmed.startsWith('</tr')) {
        flushPara();
        result.push(line);
        continue;
      }
      paraBuffer.push(trimmed);
    }
    flushPara();
    
    // Wrap consecutive <li> in <ul>
    let finalHtml = result.join('\n');
    finalHtml = finalHtml.replace(/(<li>.+<\/li>\n?)+/g, match => `<ul>${match}</ul>`);
    
    return finalHtml;
  }

  function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }

  // --- Admin Sidebar ---
  function adminSidebar(activePath) {
    const navItems = [
      { label: '仪表盘', href: '#/admin', icon: 'layout-dashboard' },
      { label: '举报与审核', href: '#/admin/reports', icon: 'flag' },
      { label: '积分与货币', href: '#/admin/points', icon: 'coins' },
      { label: '等级管理', href: '#/admin/levels', icon: 'trophy' },
      { label: '主题管理', href: '#/admin/themes', icon: 'palette' },
      { label: '插件管理', href: '#/admin/plugins', icon: 'puzzle' },
      { label: 'OAuth 客户端', href: '#/admin/oauth', icon: 'key' },
      { label: '系统设置', href: '#/admin/settings', icon: 'settings' }
    ];

    function isActive(href) {
      const hashPath = href.replace('#', '');
      if (hashPath === '/admin') return activePath === '/admin';
      return activePath.startsWith(hashPath);
    }

    return `
      <aside class="admin-sidebar">
        <div class="sidebar-header">
          <a href="#/" class="sidebar-logo">BBLBB</a>
          <span class="sidebar-label">管理后台</span>
        </div>
        <nav class="sidebar-nav">
          ${navItems.map(item => `
            <a href="${item.href}" class="sidebar-item ${isActive(item.href) ? 'active' : ''}">
              ${icon(item.icon, 18)}
              <span>${item.label}</span>
            </a>
          `).join('')}
        </nav>
      </aside>
    `;
  }

  // --- Switch ---
  function switchEl(on, onClick = '') {
    return `
      <div class="switch ${on ? 'on' : ''}" onclick="${onClick}">
        <div class="switch-thumb"></div>
      </div>
    `;
  }

  return {
    avatar,
    icon,
    button,
    tag,
    badge,
    levelBadge,
    roleBadge,
    statusBadge,
    priorityBadge,
    levelProgress,
    navbar,
    postRow,
    postList,
    categoryBadge,
    formatCount,
    articleCard,
    boardCard,
    pagination,
    emptyState,
    skeleton,
    breadcrumb,
    userInfoCard,
    tabs,
    restrictedCard,
    renderMarkdown,
    adminSidebar,
    switchEl,
    escapeHtml
  };
})();

// Global handler for pay unlock
window.handlePayUnlock = function(postId, price) {
  const post = MockData.getPost(postId);
  const user = Store.state.user;
  
  Modal.open({
    title: '确认支付',
    content: `
      <p>确定支付 ${price} B币解锁此内容吗？解锁后永久可见。</p>
      <p class="pay-detail" style="margin-top: 16px; font-size: 13px; color: var(--color-text-secondary); line-height: 1.8;">
        当前余额：<span class="mono">${user.coins} B币</span><br>
        本次扣除：<span class="mono">-${price} B币</span><br>
        解锁后余额：<span class="mono">${user.coins - price} B币</span>
      </p>
    `,
    confirmText: '确认支付',
    variant: 'warning',
    onConfirm: () => {
      const success = Store.unlockPaid(postId, price);
      if (success) {
        Router.refresh();
      }
      return true;
    }
  });
};
