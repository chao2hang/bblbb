// ============================================
// BBLBB Page Renderers — Part 2
// Publish, User, Auth, Notifications, Favorites, Search, Settings, Admin
// ============================================

(function() {
  const C = Components;
  const P = Pages;

  // ============================================
  // 6. Publish Page (/publish)
  // ============================================
  P.publish = function(params) {
    const type = Store.state.publishType;
    const selectedBoard = params.board || '';
    const title = params.title || '';
    const editorTab = Store.state.editorTab;
    const content = params.content || '';
    
    const sampleContent = type === 'article' 
      ? '## 标题\n\n在这里写你的文章内容...\n\n### 小标题\n\n- 列表项 1\n- 列表项 2\n\n```js\nconsole.log("hello");\n```'
      : '在这里写下你的讨论内容...';

    const html = `
      <div class="container">
        ${C.breadcrumb([
          { label: '首页', href: '#/' },
          { label: '发布' }
        ])}

        <div class="publish-layout">
          <div class="publish-main">
            <div class="type-switch">
              <button class="${type === 'topic' ? 'active' : ''}" onclick="Store.setPublishType('topic'); Router.refresh();">
                ${C.icon('message-square', 14)}
                讨论帖
              </button>
              <button class="${type === 'article' ? 'active' : ''}" onclick="Store.setPublishType('article'); Router.refresh();">
                ${C.icon('file-text', 14)}
                专栏文章
              </button>
            </div>

            <input type="text" class="publish-title-input" placeholder="${type === 'article' ? '输入文章标题...' : '输入帖子标题...'}" value="${title}" id="publish-title" />

            <div class="editor-container">
              <div class="editor-tabs">
                <button class="editor-tab ${editorTab === 'write' ? 'active' : ''}" onclick="Store.setEditorTab('write'); Router.refresh();">
                  ${C.icon('edit-3', 14)} 编辑
                </button>
                <button class="editor-tab ${editorTab === 'preview' ? 'active' : ''}" onclick="Store.setEditorTab('preview'); Router.refresh();">
                  ${C.icon('eye', 14)} 预览
                </button>
              </div>
              ${editorTab === 'write' ? `
                <div class="editor-toolbar">
                  <button class="editor-toolbar-btn" title="粗体" onclick="wrapSelection('**','**')"><b>B</b></button>
                  <button class="editor-toolbar-btn" title="斜体" onclick="wrapSelection('*','*')"><i>I</i></button>
                  <button class="editor-toolbar-btn" title="标题" onclick="wrapSelection('## ','')">H</button>
                  <div class="editor-toolbar-divider"></div>
                  <button class="editor-toolbar-btn" title="链接">${C.icon('link', 14)}</button>
                  <button class="editor-toolbar-btn" title="图片">${C.icon('image', 14)}</button>
                  <button class="editor-toolbar-btn" title="代码" onclick="wrapCodeBlock()">${C.icon('code', 14)}</button>
                  <button class="editor-toolbar-btn" title="引用" onclick="wrapSelection('> ','')">${C.icon('quote', 14)}</button>
                  <div class="editor-toolbar-divider"></div>
                  <button class="editor-toolbar-btn" title="无序列表">${C.icon('list', 14)}</button>
                  <button class="editor-toolbar-btn" title="有序列表">${C.icon('list-ordered', 14)}</button>
                  <button class="editor-toolbar-btn" title="表格">${C.icon('table', 14)}</button>
                </div>
                <textarea class="editor-textarea" id="publish-content" placeholder="${type === 'article' ? '使用 Markdown 编写文章...' : '使用 Markdown 编写内容...'}">${content || sampleContent}</textarea>
              ` : `
                <div class="editor-preview prose">
                  ${C.renderMarkdown(content || sampleContent)}
                </div>
              `}
            </div>
          </div>

          <div class="publish-sidebar">
            <div class="card">
              <div class="card-header">
                <span class="card-title">发布设置</span>
              </div>
              <div class="card-body" style="display: flex; flex-direction: column; gap: var(--space-4);">
                <div class="input-wrapper">
                  <label class="input-label">选择板块</label>
                  <select class="input-field" id="publish-board">
                    <option value="">请选择板块</option>
                    ${MockData.boards.map(b => `<option value="${b.slug}" ${selectedBoard === b.slug ? 'selected' : ''}>${b.name}</option>`).join('')}
                  </select>
                </div>
                <div class="input-wrapper">
                  <label class="input-label">标签</label>
                  <div class="tag-input-wrapper" id="tag-input-wrapper">
                    <span class="tag-chip">
                      Rust
                      <button class="tag-chip-remove" onclick="this.parentElement.remove()">${C.icon('x', 10)}</button>
                    </span>
                    <input type="text" class="tag-input" placeholder="输入标签后回车" />
                  </div>
                  <div class="input-hint">最多添加 5 个标签</div>
                </div>
                ${type === 'topic' ? `
                  <div class="input-wrapper">
                    <label class="input-label">回复可见</label>
                    <div style="display: flex; align-items: center; justify-content: space-between;">
                      <span class="text-secondary" style="font-size: var(--text-sm);">设置部分内容回复后可见</span>
                      ${C.switchEl(false)}
                    </div>
                  </div>
                ` : ''}
              </div>
              <div class="card-footer" style="display: flex; gap: var(--space-2);">
                ${C.button({ text: '存草稿', variant: 'secondary', onClick: "Toast.show('草稿已保存', 'success')", style: 'flex: 1;' })}
                ${C.button({ text: '发布', variant: 'primary', onClick: 'submitPublish()', style: 'flex: 1;' })}
              </div>
            </div>

            <div class="card">
              <div class="card-header">
                <span class="card-title">发帖规范</span>
              </div>
              <div class="card-body">
                <ul style="font-size: var(--text-sm); color: var(--color-text-secondary); line-height: 2;">
                  <li>• 请选择合适的板块发布</li>
                  <li>• 标题请简明扼要</li>
                  <li>• 求助贴请提供复现步骤</li>
                  <li>• 禁止发布广告内容</li>
                  <li>• 友善讨论，理性交流</li>
                </ul>
              </div>
            </div>
          </div>
        </div>
      </div>
    `;

    return P.pageLayout(html, '/publish');
  };

  // ============================================
  // 7. User Profile (/users/[name])
  // ============================================
  P.userProfile = function(name, params) {
    const user = MockData.getUser(name);
    if (!user) return P.notFound('用户不存在', '该用户可能已注销或不存在。');

    const tab = params.tab || 'posts';
    const userPosts = MockData.posts.filter(p => p.author === name);
    const userReplies = Object.values(MockData.replies).flat().filter(r => r.author === name);
    const favorites = MockData.posts.filter(p => Store.isFavorite(p.id));

    const tabs = [
      { key: 'posts', label: '发布', count: userPosts.length },
      { key: 'replies', label: '回复', count: userReplies.length },
      { key: 'favorites', label: '收藏', count: favorites.length },
      { key: 'about', label: '关于' }
    ];

    let tabContent = '';
    if (tab === 'posts') {
      tabContent = C.postList(userPosts, { empty: { icon: 'file-text', title: '暂无帖子', desc: 'TA 还没有发布过帖子。' } });
    } else if (tab === 'replies') {
      tabContent = userReplies.length > 0
        ? userReplies.map(r => `
            <div class="simple-row">
              <div class="post-row-main">
                <div class="post-row-title" style="font-size: var(--text-sm);">
                  <a href="#/topics/${r.topicId}#reply-${r.id}">${MockData.getPost(r.topicId)?.title || '帖子'}</a>
                </div>
                <div class="post-row-meta">
                  <span>${r.content.substring(0, 80)}${r.content.length > 80 ? '...' : ''}</span>
                </div>
              </div>
              <div class="post-row-meta simple-row-aside">
                <span>${r.createdAt}</span>
              </div>
            </div>
          `).join('')
        : C.emptyState({ icon: 'message-square', title: '暂无回复', desc: 'TA 还没有回复过帖子。' });
    } else if (tab === 'favorites') {
      tabContent = C.postList(favorites, { empty: { icon: 'heart', title: '暂无收藏', desc: 'TA 还没有收藏任何内容。' } });
    } else if (tab === 'about') {
      tabContent = `
        <div style="padding: var(--space-6);">
          <div style="display: grid; grid-template-columns: 120px 1fr; gap: var(--space-4); font-size: var(--text-sm);">
            <div class="text-secondary">用户ID</div>
            <div>${user.name}</div>
            <div class="text-secondary">注册时间</div>
            <div>${user.joinedAt}</div>
            <div class="text-secondary">最后活跃</div>
            <div>${user.lastActive}</div>
            <div class="text-secondary">等级</div>
            <div>LV.${user.level} (${user.exp}/${user.expNext} EXP)</div>
            <div class="text-secondary">B币</div>
            <div>${user.coins}</div>
            <div class="text-secondary">贡献值</div>
            <div>${user.contribution}</div>
            <div class="text-secondary">角色</div>
            <div>${user.roles.includes('admin') ? '管理员' : user.roles.includes('moderator') ? '版主' : '普通会员'}</div>
            ${user.modBoards.length > 0 ? `
              <div class="text-secondary">管理板块</div>
              <div>${user.modBoards.map(slug => MockData.getBoard(slug)?.name || slug).join('、')}</div>
            ` : ''}
            <div class="text-secondary">个人简介</div>
            <div>${user.bio}</div>
          </div>
        </div>
      `;
    }

    const content = `
      <div class="container" style="padding-top: var(--space-6); padding-bottom: var(--space-6);">
        ${C.breadcrumb([
          { label: '首页', href: '#/' },
          { label: name }
        ])}

        <div class="card">
          <div class="profile-header">
            <div class="profile-avatar">
              ${C.avatar(name, 'xl')}
            </div>
            <div class="profile-info">
              <div class="profile-name">
                ${name}
                ${C.levelBadge(user.level)}
                ${C.roleBadge(user.roles)}
              </div>
              <div class="profile-bio">${user.bio}</div>
              <div class="profile-stats">
                <div class="profile-stat">
                  <span class="profile-stat-num">${userPosts.length}</span>
                  <span class="profile-stat-label">帖子</span>
                </div>
                <div class="profile-stat">
                  <span class="profile-stat-num">${userReplies.length}</span>
                  <span class="profile-stat-label">回复</span>
                </div>
                <div class="profile-stat">
                  <span class="profile-stat-num">${favorites.length}</span>
                  <span class="profile-stat-label">收藏</span>
                </div>
              </div>
              <div class="profile-currency">
                <div class="currency-item">
                  ${C.icon('coins', 16)}
                  <strong>${user.coins}</strong> B币
                </div>
                <div class="currency-item">
                  ${C.icon('award', 16)}
                  <strong>${user.contribution}</strong> 贡献值
                </div>
              </div>
              ${C.levelProgress(user.exp, user.expNext, user.level)}
            </div>
            <div style="display: flex; flex-direction: column; gap: var(--space-2);">
              ${name !== Store.state.user.name ? `
                ${C.button({ text: '关注', variant: 'secondary', icon: 'user-plus', onClick: "Toast.show('已关注', 'success')" })}
                ${C.button({ text: '私信', variant: 'ghost', icon: 'message-circle', onClick: "Toast.show('私信功能开发中', 'info')" })}
              ` : `
                ${C.button({ text: '编辑资料', variant: 'secondary', icon: 'edit-3', href: '#/settings' })}
              `}
            </div>
          </div>

          ${C.tabs(tabs, tab, 'Router.setUserTab')}

          <div>${tabContent}</div>
        </div>
      </div>
    `;

    return P.pageLayout(content, '/users');
  };

  // ============================================
  // 8. Auth Pages
  // ============================================
  P.login = function() {
    return `
      <div class="auth-wrapper">
        <div class="auth-card">
          <div class="auth-header">
            <div class="auth-logo">BBLBB</div>
            <div class="auth-title">欢迎回来</div>
            <div class="auth-subtitle">登录你的账号继续探索</div>
          </div>
          <div class="auth-body">
            <form class="auth-form" onsubmit="event.preventDefault(); handleLogin();">
              <div class="input-wrapper">
                <label class="input-label">用户名</label>
                <input type="text" class="input-field" id="login-username" value="Chaos" placeholder="输入用户名" />
              </div>
              <div class="input-wrapper">
                <label class="input-label">密码</label>
                <input type="password" class="input-field" id="login-password" value="123456" placeholder="输入密码" />
              </div>
              <div style="display: flex; justify-content: space-between; align-items: center; font-size: var(--text-sm);">
                <label style="display: flex; align-items: center; gap: 6px; cursor: pointer;">
                  <input type="checkbox" checked /> 记住我
                </label>
                <a href="#/forgot-password">忘记密码？</a>
              </div>
              ${C.button({ text: '登录', variant: 'primary', size: 'lg', onClick: 'handleLogin()' })}
              <div class="auth-hint">
                <strong>测试账号：</strong><br />
                用户名：Chaos / 密码：123456（LV.6 版主）<br />
                用户名：Echo / 密码：123456（LV.8 管理员）
              </div>
            </form>
          </div>
          <div class="auth-footer">
            还没有账号？<a href="#/register">立即注册</a>
          </div>
        </div>
      </div>
    `;
  };

  P.register = function() {
    return `
      <div class="auth-wrapper">
        <div class="auth-card">
          <div class="auth-header">
            <div class="auth-logo">BBLBB</div>
            <div class="auth-title">创建账号</div>
            <div class="auth-subtitle">加入我们，开启你的社区之旅</div>
          </div>
          <div class="auth-body">
            <form class="auth-form" onsubmit="event.preventDefault(); handleRegister();">
              <div class="input-wrapper">
                <label class="input-label">用户名</label>
                <input type="text" class="input-field" placeholder="3-20 个字符" id="reg-username" />
              </div>
              <div class="input-wrapper">
                <label class="input-label">邮箱</label>
                <input type="email" class="input-field" placeholder="用于找回密码" id="reg-email" />
              </div>
              <div class="input-wrapper">
                <label class="input-label">密码</label>
                <input type="password" class="input-field" placeholder="至少 6 位" id="reg-password" />
              </div>
              <div class="input-wrapper">
                <label class="input-label">确认密码</label>
                <input type="password" class="input-field" placeholder="再次输入密码" id="reg-confirm" />
              </div>
              <label style="display: flex; align-items: flex-start; gap: 6px; font-size: var(--text-sm); cursor: pointer;">
                <input type="checkbox" style="margin-top: 3px;" />
                <span class="text-secondary">我已阅读并同意 <a href="#">用户协议</a> 和 <a href="#">隐私政策</a></span>
              </label>
              ${C.button({ text: '注册', variant: 'primary', size: 'lg', onClick: 'handleRegister()' })}
            </form>
          </div>
          <div class="auth-footer">
            已有账号？<a href="#/login">立即登录</a>
          </div>
        </div>
      </div>
    `;
  };

  P.forgotPassword = function() {
    return `
      <div class="auth-wrapper">
        <div class="auth-card">
          <div class="auth-header">
            <div class="auth-logo">BBLBB</div>
            <div class="auth-title">找回密码</div>
            <div class="auth-subtitle">输入邮箱，我们会发送重置链接</div>
          </div>
          <div class="auth-body">
            <form class="auth-form" onsubmit="event.preventDefault(); Toast.show('重置链接已发送', 'success');">
              <div class="input-wrapper">
                <label class="input-label">邮箱地址</label>
                <input type="email" class="input-field" placeholder="请输入注册邮箱" />
              </div>
              ${C.button({ text: '发送重置邮件', variant: 'primary', size: 'lg', onClick: "Toast.show('重置链接已发送', 'success')" })}
            </form>
          </div>
          <div class="auth-footer">
            想起密码了？<a href="#/login">返回登录</a>
          </div>
        </div>
      </div>
    `;
  };

  // ============================================
  // 9. Notifications (/notifications)
  // ============================================
  P.notifications = function(params) {
    const tab = params.tab || 'all';
    let list = Store.state.notifications;
    if (tab === 'unread') list = list.filter(n => !n.read);
    if (tab === 'reply') list = list.filter(n => n.type === 'reply');
    if (tab === 'like') list = list.filter(n => n.type === 'like');
    if (tab === 'system') list = list.filter(n => n.type === 'system');
    if (tab === 'mention') list = list.filter(n => n.type === 'mention');

    const unreadCount = Store.state.notifications.filter(n => !n.read).length;
    const tabs = [
      { key: 'all', label: '全部' },
      { key: 'unread', label: '未读', count: unreadCount },
      { key: 'reply', label: '回复' },
      { key: 'like', label: '赞' },
      { key: 'mention', label: '@我的' },
      { key: 'system', label: '系统' }
    ];

    const iconMap = {
      reply: 'message-square',
      like: 'heart',
      system: 'bell',
      mention: 'at-sign'
    };

    const content = `
      <div class="container" style="padding-top: var(--space-6); padding-bottom: var(--space-6);">
        ${C.breadcrumb([
          { label: '首页', href: '#/' },
          { label: '通知' }
        ])}

        <div class="card">
          <div class="card-header">
            <span class="card-title">通知中心</span>
            <button class="btn btn-ghost btn-sm" onclick="Store.markAllNotificationsRead(); Router.refresh();">
              ${C.icon('check', 14)} 全部已读
            </button>
          </div>
          ${C.tabs(tabs, tab, 'Router.setNotifTab')}
          <div>
            ${list.length > 0
              ? list.map(n => `
                  <a href="${n.sourceUrl}" class="notification-item ${n.read ? '' : 'unread'}" onclick="Store.markNotificationRead(${n.id})">
                    <div class="notification-icon">
                      ${C.icon(iconMap[n.type] || 'bell', 18)}
                    </div>
                    <div class="notification-content">
                      <div class="notification-text">${n.content}</div>
                      <div class="notification-source">${n.source}</div>
                    </div>
                    <div class="notification-time">${n.time}</div>
                  </a>
                `).join('')
              : C.emptyState({ icon: 'bell', title: '暂无通知', desc: tab === 'unread' ? '没有未读通知' : '还没有任何通知' })
            }
          </div>
        </div>
      </div>
    `;

    return P.pageLayout(content, '/notifications');
  };

  // ============================================
  // Favorites (/favorites)
  // ============================================
  P.favorites = function(params) {
    const tab = params.tab || 'posts';
    const favPosts = MockData.posts.filter(p => Store.isFavorite(p.id));

    const tabs = [
      { key: 'posts', label: '帖子', count: favPosts.length },
      { key: 'users', label: '用户', count: 0 }
    ];

    const content = `
      <div class="container" style="padding-top: var(--space-6); padding-bottom: var(--space-6);">
        ${C.breadcrumb([
          { label: '首页', href: '#/' },
          { label: '我的收藏' }
        ])}

        <div class="card">
          <div class="card-header">
            <span class="card-title">我的收藏</span>
          </div>
          ${C.tabs(tabs, tab, 'Router.setFavTab')}
          <div>
            ${tab === 'posts'
              ? C.postList(favPosts, { empty: { icon: 'heart', title: '暂无收藏', desc: '去发现更多有趣的内容吧！' } })
              : C.emptyState({ icon: 'user', title: '暂无关注', desc: '关注你感兴趣的用户' })
            }
          </div>
        </div>
      </div>
    `;

    return P.pageLayout(content, '/favorites');
  };

  // ============================================
  // Search (/search)
  // ============================================
  P.search = function(params) {
    const q = params.q || '';
    const tab = params.tab || 'posts';
    const page = parseInt(params.page) || 1;

    let results = [];
    if (q) {
      const qLower = q.toLowerCase();
      results = MockData.posts.filter(p => 
        p.title.toLowerCase().includes(qLower) ||
        p.summary.toLowerCase().includes(qLower) ||
        p.tags.some(t => t.toLowerCase().includes(qLower))
      );
    }

    const userResults = q ? Object.values(MockData.users).filter(u => 
      u.name.toLowerCase().includes(q.toLowerCase())
    ) : [];

    const tagResults = q ? MockData.tags.filter(t => 
      t.name.toLowerCase().includes(q.toLowerCase())
    ) : [];

    const tabs = [
      { key: 'posts', label: '帖子', count: results.length },
      { key: 'users', label: '用户', count: userResults.length },
      { key: 'tags', label: '标签', count: tagResults.length }
    ];

    let tabContent = '';
    if (tab === 'posts') {
      tabContent = C.postList(results, { empty: { icon: 'search', title: '没有找到相关帖子', desc: `没有找到与 "${q}" 相关的帖子，换个关键词试试。` } });
    } else if (tab === 'users') {
      tabContent = userResults.length > 0
        ? userResults.map(u => `
            <div class="simple-row">
              ${C.avatar(u.name, 'md')}
              <div class="post-row-main">
                <div class="post-row-title">
                  <a href="#/users/${u.name}">${u.name}</a>
                  ${C.levelBadge(u.level)}
                </div>
                <div class="post-row-meta">
                  <span>${u.bio}</span>
                </div>
              </div>
              <button class="btn btn-secondary btn-sm" onclick="Toast.show('已关注', 'success')">关注</button>
            </div>
          `).join('')
        : C.emptyState({ icon: 'user', title: '没有找到相关用户', desc: `没有找到与 "${q}" 相关的用户。` });
    } else if (tab === 'tags') {
      tabContent = tagResults.length > 0
        ? `<div class="card-body"><div class="tag-cloud">${tagResults.map(t => C.tag(t.name, t.count, '#/tags/' + encodeURIComponent(t.name))).join('')}</div></div>`
        : C.emptyState({ icon: 'tag', title: '没有找到相关标签', desc: `没有找到与 "${q}" 相关的标签。` });
    }

    const content = `
      <div class="container" style="padding-top: var(--space-6); padding-bottom: var(--space-6);">
        ${C.breadcrumb([
          { label: '首页', href: '#/' },
          { label: '搜索' }
        ])}

        <div class="card">
          <div class="card-header">
            <div style="flex: 1; max-width: 500px;">
              <form class="search-form" onsubmit="event.preventDefault(); Router.navigate('/search?q=' + encodeURIComponent(this.querySelector('input').value) + '&tab=${tab}');">
                ${C.icon('search', 16, 'class="search-icon"')}
                <input type="text" class="search-input" value="${q}" placeholder="搜索帖子、用户、标签..." style="width: 100%;" />
              </form>
            </div>
          </div>
          ${q ? C.tabs(tabs, tab, 'Router.setSearchTab') : ''}
          <div>${tabContent}</div>
        </div>
      </div>
    `;

    return P.pageLayout(content, '/search');
  };

  // ============================================
  // Settings (/settings)
  // ============================================
  P.settings = function(params) {
    const tab = params.tab || 'profile';
    const user = Store.state.user;

    const navItems = [
      { key: 'profile', label: '个人资料', icon: 'user' },
      { key: 'security', label: '账号安全', icon: 'shield' },
      { key: 'devices', label: '登录设备', icon: 'monitor' },
      { key: 'notifications', label: '通知设置', icon: 'bell' },
      { key: 'oauth', label: 'OAuth 授权', icon: 'key' }
    ];

    let tabContent = '';

    if (tab === 'profile') {
      tabContent = `
        <div class="settings-section">
          <div class="settings-section-header">
            <div class="settings-section-title">基本信息</div>
          </div>
          <div class="settings-section-body">
            <div style="display: flex; align-items: center; gap: var(--space-4); margin-bottom: var(--space-5);">
              ${C.avatar(user.name, 'xl')}
              <div>
                ${C.button({ text: '更换头像', variant: 'secondary', size: 'sm', onClick: "Toast.show('头像上传功能开发中', 'info')" })}
                <p class="text-tertiary" style="font-size: var(--text-xs); margin-top: var(--space-2);">支持 JPG、PNG 格式，大小不超过 2MB</p>
              </div>
            </div>
            <div style="display: grid; grid-template-columns: 1fr 1fr; gap: var(--space-4);">
              <div class="input-wrapper">
                <label class="input-label">用户名</label>
                <input type="text" class="input-field" value="${user.name}" disabled />
                <div class="input-hint">用户名不可修改</div>
              </div>
              <div class="input-wrapper">
                <label class="input-label">昵称</label>
                <input type="text" class="input-field" value="${user.name}" />
              </div>
            </div>
            <div class="input-wrapper" style="margin-top: var(--space-4);">
              <label class="input-label">个人简介</label>
              <textarea class="input-field" rows="3" maxlength="200">${user.bio}</textarea>
              <div class="input-hint">最多 200 字</div>
            </div>
          </div>
          <div class="card-footer" style="display: flex; justify-content: flex-end; gap: var(--space-2);">
            ${C.button({ text: '取消', variant: 'secondary' })}
            ${C.button({ text: '保存修改', variant: 'primary', onClick: "Toast.show('资料已更新', 'success')" })}
          </div>
        </div>
      `;
    } else if (tab === 'security') {
      tabContent = `
        <div class="settings-section">
          <div class="settings-section-header">
            <div class="settings-section-title">账号安全</div>
          </div>
          <div class="settings-section-body">
            <div class="settings-row">
              <div class="settings-row-label">
                <div class="settings-row-title">密码</div>
                <div class="settings-row-desc">上次修改：2026-06-15</div>
              </div>
              ${C.button({ text: '修改密码', variant: 'secondary', size: 'sm' })}
            </div>
            <div class="settings-row">
              <div class="settings-row-label">
                <div class="settings-row-title">邮箱</div>
                <div class="settings-row-desc">chaos@example.com</div>
              </div>
              ${C.button({ text: '更换邮箱', variant: 'secondary', size: 'sm' })}
            </div>
            <div class="settings-row">
              <div class="settings-row-label">
                <div class="settings-row-title">两步验证</div>
                <div class="settings-row-desc">使用 TOTP 应用增强账号安全</div>
              </div>
              ${C.switchEl(false, "Toast.show('两步验证功能开发中', 'info')")}
            </div>
            <div class="settings-row">
              <div class="settings-row-label">
                <div class="settings-row-title">登录设备管理</div>
                <div class="settings-row-desc">管理你的登录设备</div>
              </div>
              <a href="#/settings?tab=devices" class="btn btn-secondary btn-sm">查看</a>
            </div>
          </div>
        </div>
      `;
    } else if (tab === 'devices') {
      const devices = Store.state.devices;
      tabContent = `
        <div class="settings-section">
          <div class="settings-section-header">
            <div class="settings-section-title">登录设备</div>
          </div>
          <div class="settings-section-body">
            ${devices.map(d => `
              <div class="device-card">
                <div class="device-icon">
                  ${C.icon(d.isCurrent ? 'monitor' : (d.os.includes('iOS') || d.os.includes('mac') ? 'smartphone' : 'laptop'), 20)}
                </div>
                <div class="device-info">
                  <div class="device-name">
                    ${d.name}
                    ${d.isCurrent ? '<span class="device-current">当前设备</span>' : ''}
                  </div>
                  <div class="device-detail">
                    ${d.os} · ${d.browser} · ${d.ip} · ${d.location}
                  </div>
                  <div class="device-detail">最后活跃：${d.lastActive}</div>
                </div>
                ${!d.isCurrent ? C.button({ text: '下线', variant: 'danger', size: 'sm', onClick: `removeDevice('${d.id}')` }) : ''}
              </div>
            `).join('')}
          </div>
        </div>
      `;
    } else if (tab === 'notifications') {
      tabContent = `
        <div class="settings-section">
          <div class="settings-section-header">
            <div class="settings-section-title">通知设置</div>
          </div>
          <div class="settings-section-body">
            <div class="settings-row">
              <div class="settings-row-label">
                <div class="settings-row-title">回复通知</div>
                <div class="settings-row-desc">有人回复你的帖子时通知</div>
              </div>
              ${C.switchEl(true, "Toast.show('设置已更新', 'success')")}
            </div>
            <div class="settings-row">
              <div class="settings-row-label">
                <div class="settings-row-title">点赞通知</div>
                <div class="settings-row-desc">有人赞了你的内容时通知</div>
              </div>
              ${C.switchEl(true, "Toast.show('设置已更新', 'success')")}
            </div>
            <div class="settings-row">
              <div class="settings-row-label">
                <div class="settings-row-title">@ 提及通知</div>
                <div class="settings-row-desc">有人在帖子中 @ 你时通知</div>
              </div>
              ${C.switchEl(true, "Toast.show('设置已更新', 'success')")}
            </div>
            <div class="settings-row">
              <div class="settings-row-label">
                <div class="settings-row-title">系统通知</div>
                <div class="settings-row-desc">接收系统公告和活动通知</div>
              </div>
              ${C.switchEl(false, "Toast.show('设置已更新', 'success')")}
            </div>
            <div class="settings-row">
              <div class="settings-row-label">
                <div class="settings-row-title">邮件通知</div>
                <div class="settings-row-desc">重要通知通过邮件发送</div>
              </div>
              ${C.switchEl(true, "Toast.show('设置已更新', 'success')")}
            </div>
          </div>
        </div>
      `;
    } else if (tab === 'oauth') {
      const clients = Store.state.oauthClients;
      tabContent = `
        <div class="settings-section">
          <div class="settings-section-header">
            <div class="settings-section-title">已授权的应用</div>
          </div>
          <div class="settings-section-body">
            ${clients.length > 0 ? clients.map(c => `
              <div class="oauth-app-card">
                <div class="oauth-app-icon">
                  ${C.icon('key', 20)}
                </div>
                <div class="oauth-app-info">
                  <div class="oauth-app-name">${c.name}</div>
                  <div class="oauth-app-id">${c.clientId}</div>
                  <div class="oauth-app-stats">
                    <span>权限：${c.scopes.join(', ')}</span>
                    <span>授权时间：${c.createdAt}</span>
                  </div>
                </div>
                ${C.button({ text: '撤销授权', variant: 'danger', size: 'sm', onClick: "Toast.show('授权已撤销', 'warning')" })}
              </div>
            `).join('') : C.emptyState({ icon: 'key', title: '暂无授权应用', desc: '你还没有授权任何第三方应用。' })}
          </div>
        </div>
      `;
    }

    const content = `
      <div class="container" style="padding-top: var(--space-6); padding-bottom: var(--space-6);">
        ${C.breadcrumb([
          { label: '首页', href: '#/' },
          { label: '账号设置' }
        ])}

        <div class="settings-layout">
          <nav class="settings-nav">
            ${navItems.map(item => `
              <a href="#/settings?tab=${item.key}" class="settings-nav-item ${tab === item.key ? 'active' : ''}">
                ${C.icon(item.icon, 16)}
                <span>${item.label}</span>
              </a>
            `).join('')}
          </nav>
          <div class="settings-content">
            ${tabContent}
          </div>
        </div>
      </div>
    `;

    return P.pageLayout(content, '/settings');
  };

  // ============================================
  // Helper: wrapSelection for editor
  // ============================================
  window.wrapCodeBlock = function() {
    wrapSelection('\n```\n', '\n```\n');
  };

  window.wrapSelection = function(before, after) {
    const ta = document.getElementById('publish-content');
    if (!ta) return;
    const start = ta.selectionStart;
    const end = ta.selectionEnd;
    const text = ta.value;
    const selected = text.substring(start, end);
    ta.value = text.substring(0, start) + before + selected + after + text.substring(end);
    ta.focus();
    ta.selectionStart = start + before.length;
    ta.selectionEnd = end + before.length;
  };

  window.submitPublish = function() {
    const title = document.getElementById('publish-title')?.value;
    const board = document.getElementById('publish-board')?.value;
    if (!title || !title.trim()) {
      Toast.show('请输入标题', 'warning');
      return;
    }
    if (!board) {
      Toast.show('请选择板块', 'warning');
      return;
    }
    Toast.show('发布成功', 'success');
    setTimeout(() => Router.navigate('/boards/' + board), 800);
  };

  window.handleLogin = function() {
    const username = document.getElementById('login-username')?.value;
    if (!username) {
      Toast.show('请输入用户名', 'warning');
      return;
    }
    Toast.show('登录成功', 'success');
    setTimeout(() => Router.navigate('/'), 500);
  };

  window.handleRegister = function() {
    const username = document.getElementById('reg-username')?.value;
    const password = document.getElementById('reg-password')?.value;
    const confirm = document.getElementById('reg-confirm')?.value;
    
    if (!username || username.length < 3) {
      Toast.show('用户名至少 3 个字符', 'danger');
      return;
    }
    if (!password || password.length < 6) {
      Toast.show('密码至少 6 位', 'danger');
      return;
    }
    if (password !== confirm) {
      Toast.show('两次密码不一致', 'danger');
      return;
    }
    Toast.show('注册成功', 'success');
    setTimeout(() => Router.navigate('/login'), 800);
  };

  window.removeDevice = function(deviceId) {
    Modal.open({
      title: '确认下线',
      content: '<p>确定要下线此设备吗？下线后需要重新登录。</p>',
      confirmText: '确认下线',
      variant: 'danger',
      onConfirm: () => {
        Store.removeDevice(deviceId);
        Router.refresh();
      }
    });
  };

})();
