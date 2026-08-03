// ============================================
// BBLBB Page Renderers
// ============================================

window.Pages = (function() {
  const C = Components;

  // ============================================
  // Helper: Page wrapper with navbar
  // ============================================
  function pageLayout(content, path) {
    return `
      ${C.navbar(path)}
      <div class="page-wrapper">
        ${content}
      </div>
    `;
  }

  // ============================================
  // 1. Home Page (/)
  // ============================================
  function home() {
    const featuredArticles = MockData.getArticles().slice(0, 3);
    const latestPosts = [...MockData.posts].sort((a, b) =>
      new Date(b.updatedAt) - new Date(a.updatedAt)
    ).slice(0, 8);
    const activeBoards = MockData.boards.slice(0, 3);
    const hotTags = [...MockData.tags].sort((a, b) => b.count - a.count);
    const user = Store.state.user;

    const content = `
      <div class="container">
        <div class="page-content content-grid home-content">
          <div class="main-col home-main">
            <!-- Latest Discussions: the primary task on a community home -->
            <section class="content-section home-discussions">
              <div class="section-header">
                <div>
                  <h1 class="section-title">最新讨论</h1>
                  <p class="section-desc">社区正在发生的技术讨论</p>
                </div>
                <a href="#/boards" class="section-more">浏览全部</a>
              </div>
              <div class="section-surface">
                ${C.postList(latestPosts)}
              </div>
            </section>

            <!-- Featured Articles -->
            <section class="content-section">
              <div class="section-header">
                <div>
                  <h2 class="section-title">精选文章</h2>
                  <p class="section-desc">值得花时间阅读的实践与思考</p>
                </div>
                <a href="#/articles" class="section-more">查看全部</a>
              </div>
              <div class="articles-grid home-articles">
                ${featuredArticles.map((p) => C.articleCard(p)).join('')}
              </div>
            </section>
          </div>

          <div class="side-col">
            ${C.userInfoCard(user)}

            <div class="card">
              <div class="card-header">
                <span class="card-title">活跃板块</span>
              </div>
              <div class="card-body">
                <div class="boards-grid" style="grid-template-columns: 1fr;">
                  ${activeBoards.map((b) => C.boardCard(b)).join('')}
                </div>
              </div>
            </div>

            <div class="card">
              <div class="card-header">
                <span class="card-title">热门标签</span>
              </div>
              <div class="card-body">
                <div class="tag-cloud">
                  ${hotTags.map((t) => C.tag(t.name, t.count, '#/tags/' + encodeURIComponent(t.name))).join('')}
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    `;

    return pageLayout(content, '/');
  }

  // ============================================
  // 2. Articles List (/articles)
  // ============================================
  function articlesList(params) {
    const page = parseInt(params.page) || 1;
    const sort = params.sort || 'latest';
    const board = params.board || '';
    
    let articles = MockData.getArticles();
    if (board) articles = articles.filter(p => p.board === board);
    if (sort === 'hot') articles.sort((a, b) => b.views - a.views);
    else articles.sort((a, b) => new Date(b.createdAt) - new Date(a.createdAt));

    const perPage = 6;
    const totalPages = Math.ceil(articles.length / perPage);
    const pageArticles = articles.slice((page - 1) * perPage, page * perPage);

    const content = `
      <div class="container">
        ${C.breadcrumb([
          { label: '首页', href: '#/' },
          { label: '专栏文章' }
        ])}
        
        <div class="card">
          <div class="filter-bar">
            <div class="filter-left">
              <select class="filter-select" onchange="Router.updateParams({ sort: this.value })">
                <option value="latest" ${sort === 'latest' ? 'selected' : ''}>最新发布</option>
                <option value="hot" ${sort === 'hot' ? 'selected' : ''}>最多阅读</option>
              </select>
              <select class="filter-select" onchange="Router.updateParams({ board: this.value, page: 1 })">
                <option value="">全部板块</option>
                ${MockData.boards.map(b => `<option value="${b.slug}" ${board === b.slug ? 'selected' : ''}>${b.name}</option>`).join('')}
              </select>
            </div>
            <div class="filter-right">
              <span class="text-secondary" style="font-size: var(--text-sm);">共 ${articles.length} 篇文章</span>
            </div>
          </div>
          <div class="card-body">
            ${pageArticles.length > 0 
              ? `<div class="articles-grid">${pageArticles.map(p => C.articleCard(p)).join('')}</div>`
              : C.emptyState({ icon: 'file-text', title: '暂无文章', desc: '该分类下暂无文章，换个筛选条件试试。' })
            }
          </div>
          ${totalPages > 1 ? C.pagination(page, totalPages, 'Router.setPage') : ''}
        </div>
      </div>
    `;

    return pageLayout(content, '/articles');
  }

  // ============================================
  // 3. Boards Overview (/boards)
  // ============================================
  function boardsList() {
    const content = `
      <div class="container">
        ${C.breadcrumb([
          { label: '首页', href: '#/' },
          { label: '板块总览' }
        ])}
        
        <div class="card">
          <div class="card-header">
            <span class="card-title">全部板块</span>
            <span class="text-secondary" style="font-size: var(--text-sm);">共 ${MockData.boards.length} 个板块</span>
          </div>
          <div class="card-body">
            <div class="boards-grid">
              ${MockData.boards.map(b => C.boardCard(b)).join('')}
            </div>
          </div>
        </div>
      </div>
    `;

    return pageLayout(content, '/boards');
  }

  // ============================================
  // Board Detail (/boards/[slug])
  // ============================================
  function boardDetail(slug, params) {
    const board = MockData.getBoard(slug);
    if (!board) return notFound();

    const page = parseInt(params.page) || 1;
    const tab = params.tab || 'latest';
    
    let posts = MockData.getPostsByBoard(slug);
    if (tab === 'hot') posts.sort((a, b) => b.views - a.views);
    else if (tab === 'essence') posts = posts.filter(p => p.isEssence);
    else posts.sort((a, b) => new Date(b.updatedAt) - new Date(a.updatedAt));

    const perPage = 10;
    const totalPages = Math.max(1, Math.ceil(posts.length / perPage));
    const pagePosts = posts.slice((page - 1) * perPage, page * perPage);

    const tabs = [
      { key: 'latest', label: '最新' },
      { key: 'hot', label: '热门' },
      { key: 'essence', label: '精华' }
    ];

    const content = `
      <div class="container">
        <div class="page-content content-grid">
          <div class="main-col">
            ${C.breadcrumb([
              { label: '首页', href: '#/' },
              { label: '板块', href: '#/boards' },
              { label: board.name }
            ])}

            <div class="board-header">
              <div class="board-icon">
                ${C.icon(board.icon, 28)}
              </div>
              <div class="board-info">
                <h1 class="board-name">${board.name}</h1>
                <p class="board-desc">${board.description}</p>
                <div class="board-stats">
                  <span><strong>${board.postCount}</strong> 帖子</span>
                  <span><strong>${board.todayCount}</strong> 今日</span>
                </div>
              </div>
              <div>
                ${C.button({ text: '发布新帖', variant: 'primary', icon: 'pen-line', href: '#/publish' })}
              </div>
            </div>

            <div class="card">
              ${C.tabs(tabs, tab, 'Router.setBoardTab')}

              <div>
                ${C.postList(pagePosts, { empty: { icon: 'message-square', title: '暂无帖子', desc: '成为第一个发帖的人吧！' } })}
              </div>

              ${totalPages > 1 ? C.pagination(page, totalPages, 'Router.setPage') : ''}
            </div>
          </div>

          <aside class="side-col">
            <div class="card">
              <div class="card-header"><span class="card-title">板块规则</span></div>
              <div class="card-body">
                <ul class="board-rules">
                  ${board.rules.map((r) => `<li>${r}</li>`).join('')}
                </ul>
              </div>
            </div>
            <div class="card">
              <div class="card-header"><span class="card-title">版主</span></div>
              <div class="card-body">
                <div class="board-mods">
                  ${board.mods.map((m) => `
                    <a href="#/users/${m}" class="board-mod">
                      ${C.avatar(m, 'sm')}
                      <span>${m}</span>
                    </a>`).join('')}
                </div>
              </div>
            </div>
            <div class="card">
              <div class="card-header"><span class="card-title">板块标签</span></div>
              <div class="card-body">
                <div class="tag-cloud">
                  ${MockData.tags.slice(0, 8).map((t) => C.tag(t.name, t.count, '#/tags/' + encodeURIComponent(t.name))).join('')}
                </div>
              </div>
            </div>
          </aside>
        </div>
      </div>
    `;

    return pageLayout(content, '/boards');
  }

  // ============================================
  // 4. Tags Overview (/tags)
  // ============================================
  function tagsList() {
    const tags = [...MockData.tags].sort((a, b) => b.count - a.count);
    const content = `
      <div class="container">
        ${C.breadcrumb([
          { label: '首页', href: '#/' },
          { label: '标签总览' }
        ])}
        
        <div class="card">
          <div class="card-header">
            <span class="card-title">全部标签</span>
            <span class="text-secondary" style="font-size: var(--text-sm);">共 ${tags.length} 个标签</span>
          </div>
          <div class="card-body">
            <div class="tag-cloud">
              ${tags.map(t => C.tag(t.name, t.count, '#/tags/' + encodeURIComponent(t.name))).join('')}
            </div>
          </div>
        </div>
      </div>
    `;

    return pageLayout(content, '/tags');
  }

  // ============================================
  // Tag Detail (/tags/[name])
  // ============================================
  function tagDetail(name, params) {
    const tag = MockData.tags.find(t => t.name === name);
    if (!tag) return notFound();

    const page = parseInt(params.page) || 1;
    let posts = MockData.getPostsByTag(name);
    posts.sort((a, b) => new Date(b.updatedAt) - new Date(a.updatedAt));

    const perPage = 10;
    const totalPages = Math.max(1, Math.ceil(posts.length / perPage));
    const pagePosts = posts.slice((page - 1) * perPage, page * perPage);

    const content = `
      <div class="container">
        ${C.breadcrumb([
          { label: '首页', href: '#/' },
          { label: '标签', href: '#/tags' },
          { label: name }
        ])}
        
        <div class="card">
          <div class="card-header">
            <div>
              <span class="card-title"># ${name}</span>
              <span class="text-tertiary" style="font-size: var(--text-sm); margin-left: var(--space-3);">${tag.count} 篇相关内容</span>
            </div>
          </div>
          <div>
            ${C.postList(pagePosts, { empty: { icon: 'tag', title: '暂无相关内容', desc: '该标签下暂无帖子。' } })}
          </div>
          ${totalPages > 1 ? C.pagination(page, totalPages, 'Router.setPage') : ''}
        </div>
      </div>
    `;

    return pageLayout(content, '/tags');
  }

  // ============================================
  // 5. Topic Detail (/topics/[id])
  // ============================================
  function topicDetail(id, params) {
    const post = Store.getPost(parseInt(id));
    
    // 404 trigger: /topics/999
    if (!post || id === '999') {
      return notFound('帖子不存在', '你访问的帖子可能已被删除或不存在。');
    }

    const author = MockData.getUser(post.author);
    const board = MockData.getBoard(post.board);
    const replies = Store.getReplies(parseInt(id));
    const user = Store.state.user;
    const isMod = user.modBoards.includes(post.board) || user.roles.includes('admin');
    const isFav = Store.isFavorite(post.id);
    const isLiked = Store.isPostLiked(post.id);
    const restrictedUnlocked = !post.restricted ||
      (post.restricted.type === 'reply' && Store.isReplyUnlocked(post.id)) ||
      (post.restricted.type === 'paid' && Store.isPaidUnlocked(post.id)) ||
      (post.restricted.type === 'level' && user.level >= post.restricted.level);
    const headings = C.extractHeadings(post.content);

    const content = `
      <div class="container">
        <div class="page-content">
          ${C.breadcrumb([
            { label: '首页', href: '#/' },
            { label: board?.name || post.board, href: '#/boards/' + post.board },
            { label: post.title }
          ])}

          <div class="topic-layout">
            <div class="topic-layout-main">
              <div class="card">
                ${isMod ? `
                  <div class="mod-bar">
                    <span class="mod-bar-label">${C.icon('shield', 14)} 版主操作</span>
                    ${C.button({ text: '置顶', variant: 'ghost', size: 'sm', icon: 'pin', onClick: "Toast.show('已置顶', 'success')" })}
                    ${C.button({ text: '加精', variant: 'ghost', size: 'sm', icon: 'star', onClick: "Toast.show('已加精', 'success')" })}
                    ${C.button({ text: '锁定', variant: 'ghost', size: 'sm', icon: 'lock', onClick: "Toast.show('已锁定', 'warning')" })}
                    ${C.button({ text: '隐藏', variant: 'danger', size: 'sm', icon: 'eye-off', onClick: "Toast.show('已隐藏', 'warning')" })}
                  </div>
                ` : ''}

                <div class="topic-header">
                  <h1 class="topic-title">
                    ${post.isEssence ? C.badge('精华', 'essence') + ' ' : ''}
                    ${post.title}
                  </h1>
                  <div class="topic-meta">
                    <div class="topic-meta-author">
                      ${C.avatar(post.author, 'sm')}
                      <a href="#/users/${post.author}">${post.author}</a>
                      ${C.levelBadge(author?.level || 1)}
                      ${C.roleBadge(author?.roles || [])}
                    </div>
                    <span class="dot">·</span>
                    <a href="#/boards/${post.board}">${board?.name || post.board}</a>
                    <span class="dot">·</span>
                    <span>${post.createdAt}</span>
                    <span class="dot">·</span>
                    <span>${post.views} 阅读</span>
                  </div>
                </div>

                <div class="topic-body">
                  <div class="topic-detail prose">${C.renderMarkdown(post.content)}</div>

                  ${post.restricted ? (restrictedUnlocked
                    ? `<div class="prose restricted-unlocked">${C.renderMarkdown(post.restricted.content)}</div>`
                    : C.restrictedCard(post)) : ''}

                  ${post.tags.length > 0 ? `
                    <div class="topic-tags">
                      ${post.tags.map((t) => C.tag(t, null, '#/tags/' + encodeURIComponent(t))).join('')}
                    </div>
                  ` : ''}
                </div>

                <div class="topic-actions">
                  <button class="action-btn ${isFav ? 'active' : ''}" onclick="handleFavorite(${post.id})">
                    ${C.icon('heart', 16, isFav ? 'fill="currentColor"' : '')}
                    <span>${isFav ? '已收藏' : '收藏'}</span>
                  </button>
                  <button class="action-btn ${isLiked ? 'active' : ''}" onclick="handlePostLike(${post.id})">
                    ${C.icon('thumbs-up', 16, isLiked ? 'fill="currentColor"' : '')}
                    <span>${post.likes}</span>
                  </button>
                  <button class="action-btn" onclick="openShareDialog(${post.id})">
                    ${C.icon('share-2', 16)}
                    <span>分享</span>
                  </button>
                  <button class="action-btn" onclick="openReportDialog(${post.id})">
                    ${C.icon('flag', 16)}
                    <span>举报</span>
                  </button>
                </div>

                <div class="replies-head">全部回复 (${replies.length})</div>

                <div>
                  ${replies.length > 0
                    ? replies.map((r) => replyItem(r, post)).join('')
                    : C.emptyState({ icon: 'message-circle', title: '暂无回复', desc: '快来抢沙发吧！' })
                  }
                </div>

                <!-- Reply Editor -->
                <div class="reply-editor" id="reply-editor">
                  <div class="reply-editor-header">
                    <span class="reply-editor-title">回复</span>
                    <span class="text-tertiary">支持 Markdown</span>
                  </div>
                  <div id="reply-quote-container"></div>
                  <textarea placeholder="写下你的回复..." id="reply-textarea"></textarea>
                  <div class="reply-editor-footer">
                    <div class="reply-editor-tools">
                      <button class="btn btn-ghost btn-sm" title="粗体" onclick="insertMd('**', '**')"><b>B</b></button>
                      <button class="btn btn-ghost btn-sm" title="斜体" onclick="insertMd('*', '*')"><i>I</i></button>
                      <button class="btn btn-ghost btn-sm" title="链接" onclick="insertMd('[', '](url)')">${C.icon('link', 14)}</button>
                      <button class="btn btn-ghost btn-sm" title="代码" onclick="insertMdCode()">${C.icon('code', 14)}</button>
                    </div>
                    ${C.button({ text: '发表回复', variant: 'primary', onClick: `submitReply(${post.id})` })}
                  </div>
                </div>
              </div>
            </div>

            <aside class="topic-layout-side">
              ${headings.length ? C.toc(headings) : ''}
              <div class="card">
                <div class="card-body">
                  <div class="sidebar-card-user">
                    ${C.avatar(post.author, 'xl')}
                    <div class="sidebar-card-name">${post.author}</div>
                    <div class="sidebar-card-badges">${C.roleBadge(author?.roles || [])}${C.levelBadge(author?.level || 1)}</div>
                  </div>
                  <div class="sidebar-card-stats">
                    <div class="sidebar-card-stat"><span class="stat-num">${author?.posts || 0}</span><span class="stat-label">帖子</span></div>
                    <div class="sidebar-card-stat"><span class="stat-num">${author?.likes || 0}</span><span class="stat-label">获赞</span></div>
                    <div class="sidebar-card-stat"><span class="stat-num">${author?.replies || 0}</span><span class="stat-label">回复</span></div>
                  </div>
                  <a href="#/users/${post.author}" class="btn btn-secondary btn-sm btn-block" style="margin-top: var(--space-3);">查看主页</a>
                </div>
              </div>
              <div class="card">
                <div class="card-body">
                  <div class="board-card-name">${board?.name || post.board}</div>
                  <div class="board-card-desc" style="margin-top: 2px;">${board?.description || ''}</div>
                  <a href="#/boards/${post.board}" class="btn btn-ghost btn-sm" style="margin-top: var(--space-2);">进入板块</a>
                </div>
              </div>
            </aside>
          </div>
        </div>
      </div>
    `;

    return pageLayout(content, '/topics');
  }

  function replyItem(reply, post) {
    const author = MockData.getUser(reply.author);
    return `
      <div class="reply-item" id="reply-${reply.id}">
        <div class="reply-avatar">
          <a href="#/users/${reply.author}">${C.avatar(reply.author, 'md')}</a>
        </div>
        <div class="reply-content">
          <div class="reply-header">
            <a href="#/users/${reply.author}" class="reply-author">${reply.author}</a>
            ${C.levelBadge(author?.level || 1)}
            ${reply.isAuthor ? '<span class="badge badge-level" style="background: var(--color-success-soft); color: var(--color-success);">楼主</span>' : ''}
            <span class="reply-floor">#${reply.floor} 楼</span>
            <span class="reply-time">${reply.createdAt}</span>
          </div>
          <div class="reply-body">${reply.content.replace(/\n/g, '<br>')}</div>
          <div class="reply-actions">
            <button class="reply-action ${Store.isReplyLiked(post.id, reply.id) ? 'is-liked' : ''}" onclick="handleReplyLike(${post.id}, ${reply.id})">
              ${C.icon('thumbs-up', 12)}
              <span>${reply.likes + (Store.isReplyLiked(post.id, reply.id) && reply.id < 1000 ? 1 : 0)}</span>
            </button>
            <button class="reply-action" onclick="quoteReply(${reply.id}, '${reply.author}', ${reply.floor})">
              ${C.icon('reply', 12)}
              <span>回复</span>
            </button>
            <button class="reply-action" onclick="openReportDialog(${post.id}, ${reply.id})">
              ${C.icon('flag', 12)}
              <span>举报</span>
            </button>
          </div>
        </div>
      </div>
    `;
  }

  // ============================================
  // 404 Page
  // ============================================
  function notFound(title = '页面不存在', desc = '你访问的页面可能已被删除或链接有误。') {
    const content = `
      <div class="error-page">
        <div class="error-code">404</div>
        <div class="error-title">${title}</div>
        <div class="error-desc">${desc}</div>
        ${C.button({ text: '返回首页', variant: 'primary', size: 'lg', href: '#/' })}
      </div>
    `;
    return pageLayout(content, '');
  }

  // ============================================
  // 403 Page
  // ============================================
  function forbidden() {
    const content = `
      <div class="error-page">
        <div class="error-code" style="color: var(--color-warning);">403</div>
        <div class="error-title">无权限访问</div>
        <div class="error-desc">你没有权限访问此页面。如需访问，请联系管理员提升权限。</div>
        ${C.button({ text: '返回首页', variant: 'primary', size: 'lg', href: '#/' })}
      </div>
    `;
    return pageLayout(content, '');
  }

  // ============================================
  // 429 Page
  // ============================================
  function tooManyRequests() {
    const content = `
      <div class="error-page">
        <div class="error-code" style="color: var(--color-danger);">429</div>
        <div class="error-title">请求过于频繁</div>
        <div class="error-desc">你操作得太快了，请稍后再试。</div>
        ${C.button({ text: '返回首页', variant: 'primary', size: 'lg', href: '#/' })}
      </div>
    `;
    return pageLayout(content, '');
  }

  return {
    home,
    articlesList,
    boardsList,
    boardDetail,
    tagsList,
    tagDetail,
    topicDetail,
    notFound,
    forbidden,
    tooManyRequests,
    pageLayout
  };
})();

// ============================================
// Global interactive handlers
// ============================================
window.handleFavorite = function(postId) {
  Store.toggleFavorite(postId);
  Router.refresh();
};

window.handlePostLike = function(postId) {
  Store.togglePostLike(postId);
  Toast.show(Store.isPostLiked(postId) ? '已点赞' : '已取消点赞', 'success');
  Router.refresh();
};

window.handleReplyLike = function(topicId, replyId) {
  Store.toggleReplyLike(topicId, replyId);
  Router.refresh();
};

window.openShareDialog = function(postId) {
  document.body.insertAdjacentHTML('beforeend', Components.shareDialog(`/topics/${postId}`, '分享主题'));
};

window.openReportDialog = function(topicId, replyId = null) {
  Modal.open({
    title: replyId ? '举报回复' : '举报主题',
    content: `
      <div class="input-wrapper">
        <label class="input-label" for="report-reason">举报原因</label>
        <select class="input-field" id="report-reason">
          <option value="广告 / 垃圾信息">广告 / 垃圾信息</option>
          <option value="人身攻击">人身攻击</option>
          <option value="违法违规">违法违规</option>
          <option value="侵犯隐私">侵犯隐私</option>
          <option value="其他">其他</option>
        </select>
      </div>
      <div class="input-wrapper" style="margin-top:var(--space-3);">
        <label class="input-label" for="report-detail">补充说明</label>
        <textarea class="input-field" id="report-detail" rows="4" placeholder="请提供有助于审核的信息"></textarea>
      </div>`,
    confirmText: '提交举报',
    onConfirm: () => {
      Store.createReport({ topicId, replyId, targetType: replyId ? 'reply' : 'post', reason: document.getElementById('report-reason').value, detail: document.getElementById('report-detail').value.trim() });
      Toast.show('已提交举报，等待处理', 'success');
    }
  });
};

window.quoteReply = function(replyId, author, floor) {
  const container = document.getElementById('reply-quote-container');
  if (container) {
    container.innerHTML = `
      <div class="reply-editor-quote">
        <span>回复 ${author} 的 #${floor} 楼</span>
        <button class="reply-editor-quote-close" onclick="this.parentElement.remove()">
          ${Components.icon('x', 14)}
        </button>
      </div>
    `;
  }
  const textarea = document.getElementById('reply-textarea');
  if (textarea) {
    textarea.focus();
    textarea.scrollIntoView({ behavior: 'smooth', block: 'center' });
  }
};

window.insertMdCode = function() {
  insertMd('`', '`');
};

window.insertMd = function(before, after) {
  const textarea = document.getElementById('reply-textarea');
  if (!textarea) return;
  const start = textarea.selectionStart;
  const end = textarea.selectionEnd;
  const text = textarea.value;
  const selected = text.substring(start, end);
  textarea.value = text.substring(0, start) + before + selected + after + text.substring(end);
  textarea.focus();
  textarea.selectionStart = start + before.length;
  textarea.selectionEnd = end + before.length;
};

window.submitReply = function(topicId) {
  const textarea = document.getElementById('reply-textarea');
  if (!textarea || !textarea.value.trim()) {
    Toast.show('请输入回复内容', 'warning');
    return;
  }
  
  const result = Store.addReply(topicId, textarea.value.trim());
  if (!result.ok && result.reason === 'rate_limit') {
    Toast.show('操作过于频繁，请稍后再试', 'warning');
    Router.navigate('/429');
    return;
  }
  Toast.show(result.unlocked ? '回复已发布，隐藏内容已解锁' : '回复已发布', 'success');
  Router.refresh();
};
