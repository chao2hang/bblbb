// ============================================
// BBLBB Page Renderers — Part 2
// Publish, User, Auth, Notifications, Favorites, Search, Settings, Admin
// ============================================

(function() {
  const C = Components;
  const P = Pages;
  const articleCoverState = window.PublishArticleCover || (window.PublishArticleCover = {
    file: null,
    name: '',
    url: ''
  });

  // ============================================
  // 6. Publish Page (/publish)
  // ============================================
  P.publish = function(params) {
    const type = ['topic', 'article'].includes(params.type) ? params.type : Store.state.publishType;
    const selectedBoard = params.board || '';
    const title = params.title || '';
    const editorTab = Store.state.editorTab;
    const content = params.content || '';
    const levelQuota = Store.state.attachmentLevelQuotas[Store.state.user.level] || { maxFileMb: 2, totalCapacityMb: 20 };
    const effectiveMaxMb = Math.min(Store.state.storageConfig.maxUploadMb, levelQuota.maxFileMb);
    const usedAttachmentMb = 638;
    
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
              <button class="${type === 'topic' ? 'active' : ''}" onclick="switchPublishType('topic')">
                ${C.icon('message-square', 14)}
                讨论帖
              </button>
              <button class="${type === 'article' ? 'active' : ''}" onclick="switchPublishType('article')">
                ${C.icon('file-text', 14)}
                专栏文章
              </button>
            </div>

            <div class="publish-title-field">
              <label for="publish-title">${type === 'article' ? '文章标题' : '讨论标题'}</label>
              <div class="publish-title-control">
                <input type="text" class="input-field publish-title-input" placeholder="${type === 'article' ? '写下一个值得阅读的标题…' : '一句话说清你想讨论什么…'}" value="${title}" id="publish-title" maxlength="80" autocomplete="off" oninput="this.nextElementSibling.textContent = this.value.length + ' / 80'" />
                <span class="publish-title-hint">${title.length} / 80</span>
              </div>
            </div>

            ${type === 'article' ? `
              <div class="article-cover-field ${articleCoverState.url ? 'has-cover' : ''}" id="article-cover-field">
                <div class="article-cover-heading">
                  <label class="input-label" for="article-cover-input">文章封面 <span class="required-mark">必填</span></label>
                  <span class="input-hint">建议 16:9，JPG / PNG / WebP，最大 8 MB</span>
                </div>
                <input type="file" class="file-picker-input" id="article-cover-input" accept="image/jpeg,image/png,image/webp" onchange="handleArticleCover(this)" />
                <label class="article-cover-picker" for="article-cover-input">
                  <span class="article-cover-empty">
                    <span class="article-cover-icon">${C.icon('image', 22)}</span>
                    <strong>选择文章封面</strong>
                    <span>一张清晰的横图有助于文章被发现</span>
                  </span>
                  <img class="article-cover-preview" id="article-cover-preview" src="${C.escapeHtml(articleCoverState.url)}" alt="文章封面预览" ${articleCoverState.url ? '' : 'hidden'} />
                </label>
                <div class="article-cover-meta" ${articleCoverState.url ? '' : 'hidden'}>
                  <span class="article-cover-name" id="article-cover-name">${C.escapeHtml(articleCoverState.name)}</span>
                  <div class="article-cover-actions">
                    <label class="text-link" for="article-cover-input">更换封面</label>
                    <button type="button" class="text-link text-danger" onclick="removeArticleCover()">移除</button>
                  </div>
                </div>
              </div>
            ` : ''}

            <div class="editor-container">
              <div class="editor-tabs">
                <button class="editor-tab ${editorTab === 'write' ? 'active' : ''}" onclick="switchEditorTab('write')">
                  ${C.icon('edit-3', 14)} 编辑
                </button>
                <button class="editor-tab ${editorTab === 'preview' ? 'active' : ''}" onclick="switchEditorTab('preview')">
                  ${C.icon('eye', 14)} 预览
                </button>
              </div>
              ${editorTab === 'write' ? `
                <div class="editor-toolbar">
                  <button class="editor-toolbar-btn" title="粗体" onclick="wrapSelection('**','**')"><b>B</b></button>
                  <button class="editor-toolbar-btn" title="斜体" onclick="wrapSelection('*','*')"><i>I</i></button>
                  <button class="editor-toolbar-btn" title="标题" onclick="wrapSelection('## ','')">H</button>
                  <div class="editor-toolbar-divider"></div>
                  <button class="editor-toolbar-btn" title="链接" onclick="wrapSelection('[', '](https://)')">${C.icon('link', 14)}</button>
                  <button class="editor-toolbar-btn" title="图片" onclick="wrapSelection('![图片描述](', ')')">${C.icon('image', 14)}</button>
                  <button class="editor-toolbar-btn" title="插入视频 URL" onclick="openVideoInsertDialog()">${C.icon('video', 14)}</button>
                  <button class="editor-toolbar-btn" title="代码" onclick="wrapCodeBlock()">${C.icon('code', 14)}</button>
                  <button class="editor-toolbar-btn" title="引用" onclick="wrapSelection('> ','')">${C.icon('quote', 14)}</button>
                  <div class="editor-toolbar-divider"></div>
                  <button class="editor-toolbar-btn" title="无序列表" onclick="wrapSelection('- ', '')">${C.icon('list', 14)}</button>
                  <button class="editor-toolbar-btn" title="有序列表" onclick="wrapSelection('1. ', '')">${C.icon('list-ordered', 14)}</button>
                  <button class="editor-toolbar-btn" title="表格" onclick="wrapSelection('| 列 1 | 列 2 |\n| --- | --- |\n| ', ' | 内容 |')">${C.icon('table', 14)}</button>
                </div>
                <textarea class="editor-textarea" id="publish-content" placeholder="${type === 'article' ? '使用 Markdown 编写文章...' : '使用 Markdown 编写内容...'}">${content || sampleContent}</textarea>
                <div style="display:flex;justify-content:flex-end;gap:var(--space-2);padding:var(--space-3) var(--space-4);border-top:var(--border-default);">
                  ${C.button({ text: 'AI 格式化', variant: 'secondary', size: 'sm', icon: 'wand-2', onClick: 'requestAiFormat()' })}
                  ${C.button({ text: 'AI 生成 SEO', variant: 'ghost', size: 'sm', icon: 'search', onClick: 'requestAiSeo()' })}
                </div>
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
                  <label class="input-label" for="publish-visibility-level">最低可见等级</label>
                  <select class="input-field" id="publish-visibility-level">
                    <option value="1">公开 · 所有成员可见</option>
                    ${MockData.levels.filter(level => level.level >= 2 && level.level <= Store.state.user.level).map(level => `<option value="${level.level}">LV.${level.level} ${level.name}及以上可见</option>`).join('')}
                  </select>
                  <div class="input-hint">未达到等级时，标题、摘要、作者和正文均不会显示。最高可设置为你的 LV.${Store.state.user.level}。</div>
                </div>
                <div class="input-wrapper">
                  <label class="input-label">标签</label>
                  <div class="tag-input-wrapper" id="tag-input-wrapper">
                    <span class="tag-chip">
                      Rust
                      <button type="button" class="tag-chip-remove" onclick="this.parentElement.remove()">${C.icon('x', 10)}</button>
                    </span>
                    <input type="text" class="tag-input" placeholder="输入标签后回车" onkeydown="if(event.key === 'Enter'){event.preventDefault(); addPublishTag(this)}" />
                  </div>
                  <div class="input-hint">最多添加 5 个标签</div>
                </div>
                <div class="input-wrapper">
                  <label class="input-label" for="publish-attachment">添加附件</label>
                  <div class="file-picker">
                    <input type="file" class="file-picker-input" id="publish-attachment" onchange="validatePublishAttachment(this, ${effectiveMaxMb})" />
                    <label class="file-picker-button" for="publish-attachment">${C.icon('paperclip', 14)} 选择文件</label>
                    <span class="file-picker-name" id="publish-attachment-name">未选择文件</span>
                  </div>
                  <div class="input-hint">LV.${Store.state.user.level}：单附件最多 ${effectiveMaxMb} MB，已用 ${usedAttachmentMb} MB / ${levelQuota.totalCapacityMb >= 1024 ? `${levelQuota.totalCapacityMb / 1024} GB` : `${levelQuota.totalCapacityMb} MB`}。</div>
                  <div class="input-hint">附件对象不会因公开链接过期而删除；访问时会重新生成链接。</div>
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
              <div class="card-footer publish-actions">
                ${C.button({ text: '存草稿', variant: 'secondary', onClick: 'savePublishDraft()', extraClass: 'btn-block' })}
                ${C.button({ text: '提交审核', variant: 'secondary', onClick: "submitPublish('pending')" })}
                ${C.button({ text: '立即发布', variant: 'primary', onClick: "submitPublish('published')" })}
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
  // Internal Shop, Closet & Activity
  // ============================================
  P.shop = function() {
    const products = Store.state.shopProducts;
    const html = `<div class="container page-content">${C.breadcrumb([{label:'首页',href:'#/'},{label:'积分商城'}])}<div class="admin-page-header"><div><h1 class="admin-page-title">积分商城</h1><p class="admin-page-desc">用 B币购买昵称颜色、头像挂件和社区互动小玩意儿</p></div><div style="display:flex;align-items:center;gap:var(--space-3);"><span class="badge badge-warning">${C.icon('coins',14)} ${Store.state.user.coins} B币</span>${C.button({text:'我的装扮',variant:'secondary',icon:'sparkles',href:'#/me/closet'})}</div></div><div class="card" style="padding:var(--space-4);margin-bottom:var(--space-5);display:flex;gap:var(--space-3);">${C.icon('shield-check',18)}<div><strong>安全装扮，全局展示</strong><div class="text-secondary">商品只使用内置展示 Token，不执行 CSS/HTML/脚本。装扮不会改变权限、审核、排序或内容可见性。</div></div></div><div class="stats-grid" style="grid-template-columns:repeat(auto-fit,minmax(220px,1fr));">${products.map(product => `<article class="card" style="padding:var(--space-5);display:flex;flex-direction:column;gap:var(--space-3);"><div style="display:flex;justify-content:space-between;align-items:flex-start;"><span style="width:44px;height:44px;display:grid;place-items:center;border-radius:var(--radius-md);background:var(--color-brand-soft);color:var(--color-brand);">${C.icon(product.kind==='reaction_pack'?'heart':product.kind.includes('avatar')?'user':'sparkles',22)}</span>${product.owned?C.badge('已拥有','success'):C.badge(product.kind==='reaction_pack'?'消耗品':'永久装扮','neutral')}</div><div><h2 style="font-size:var(--text-lg);margin:0 0 var(--space-1);">${C.escapeHtml(product.title)}</h2><p class="text-secondary" style="margin:0;min-height:42px;">${C.escapeHtml(product.description)}</p></div><div style="display:flex;justify-content:space-between;align-items:center;margin-top:auto;"><strong>${product.price} B币</strong>${product.owned&&product.kind!=='reaction_pack'?C.button({text:'去装备',variant:'secondary',size:'sm',href:'#/me/closet'}):C.button({text:'立即购买',variant:'primary',size:'sm',onClick:`buyShopProduct('${product.id}')`})}</div></article>`).join('')}</div></div>`;
    return P.pageLayout(html, '/shop');
  };

  P.closet = function() {
    const entitlements = Store.state.shopEntitlements.map(ent => ({...ent, product:Store.state.shopProducts.find(p=>p.id===ent.productId)})).filter(item=>item.product);
    const html = `<div class="container page-content">${C.breadcrumb([{label:'首页',href:'#/'},{label:'积分商城',href:'#/shop'},{label:'我的装扮'}])}<div class="admin-page-header"><div><h1 class="admin-page-title">我的装扮</h1><p class="admin-page-desc">选择全局展示的昵称、头像与徽章外观</p></div>${C.button({text:'返回商城',variant:'secondary',icon:'shopping-bag',href:'#/shop'})}</div><div class="closet-layout"><div class="card closet-preview-card"><h2 class="closet-section-title">当前效果</h2><div class="closet-preview"><div class="closet-preview-avatar">${C.avatar(Store.state.user.name,'xl')}</div><div class="closet-preview-identity"><strong class="closet-preview-name ${Store.state.presentation.nicknameColor==='nickname-blue'?'shop-nickname-blue':''}">${C.escapeHtml(Store.state.user.name)}</strong><div class="closet-preview-badges">${Store.state.presentation.profileBadges.map(()=>C.badge('热心居民','success')).join('')||'<span class="text-secondary">暂未装备徽章</span>'}</div></div></div><div class="closet-preview-meta text-secondary">当前头像挂件：${Store.state.presentation.avatarAttachment||'无'} · 昵称颜色：${Store.state.presentation.nicknameColor==='default'?'默认':'海盐蓝'}</div></div><div class="card closet-items-card"><h2 class="closet-section-title">已拥有</h2><div class="closet-items">${entitlements.map(item=>`<div class="settings-row"><div class="settings-row-label"><div class="settings-row-title">${C.escapeHtml(item.product.title)}</div><div class="settings-row-desc">${item.status==='equipped'?'已全局装备':'可装备'} · 剩余 ${item.remainingQuantity}</div></div>${item.product.slot==='reaction_pack'?C.badge('使用中','neutral'):item.status==='equipped'?C.button({text:'卸下',variant:'ghost',size:'sm',onClick:`unequipShopSlot('${item.product.slot}')`}):C.button({text:'装备',variant:'primary',size:'sm',onClick:`equipShopItem('${item.id}')`})}</div>`).join('')||C.emptyState({icon:'shopping-bag',title:'还没有装扮',desc:'去商城看看吧'})}</div></div></div></div>`;
    return P.pageLayout(html, '/me/closet');
  };

  P.activity = function() {
    const a=Store.state.activity;
    const html=`<div class="container page-content">${C.breadcrumb([{label:'首页',href:'#/'},{label:'签到与活跃'}])}<div class="admin-page-header"><div><h1 class="admin-page-title">社区活跃</h1><p class="admin-page-desc">每日首次有效访问自动签到；完成友善互动可获得有限且可审计的社区奖励</p></div>${C.badge(a.checkedInToday?'今日已自动签到':'等待首次有效访问','success')}</div><div class="stats-grid"><div class="stat-card"><div class="stat-card-label">连续签到</div><div class="stat-card-value">${a.streak} 天</div></div><div class="stat-card"><div class="stat-card-label">今日获得</div><div class="stat-card-value">${a.todayEarned} B币</div></div><div class="stat-card"><div class="stat-card-label">本周活跃榜</div><div class="stat-card-value">#${a.weeklyRank}</div></div></div><div class="data-table-wrapper" style="margin-top:var(--space-5);"><div style="padding:var(--space-4);font-weight:var(--weight-semibold);">本周任务</div><table class="data-table"><thead><tr><th>任务</th><th>进度</th><th>奖励</th><th>规则</th></tr></thead><tbody><tr><td>发布一篇优质内容</td><td>0 / 1</td><td>+20 B币</td><td>通过审核后领取</td></tr><tr><td>收到 5 次真实互动</td><td>3 / 5</td><td>+10 B币</td><td>自我互动不计入</td></tr><tr><td>连续签到 7 天</td><td>${Math.min(a.streak,7)} / 7</td><td>限定徽章</td><td>每周期一次</td></tr></tbody></table><div style="padding:var(--space-4);" class="text-secondary">所有奖励受每日上限、冷却和反刷规则保护；删除后重发、批量反应和重复编辑不会重复计奖。</div></div></div>`;
    return P.pageLayout(html, '/activity');
  };

  window.buyShopProduct=function(id){const p=Store.state.shopProducts.find(x=>x.id===id);if(!p)return;Modal.open({title:'确认购买',content:`<p>使用 <strong>${p.price} B币</strong>购买「${C.escapeHtml(p.title)}」？</p><p class="text-secondary">当前余额 ${Store.state.user.coins}，购买后余额 ${Store.state.user.coins-p.price}。数字装扮默认不可退款。</p>`,confirmText:'确认购买',onConfirm:()=>{const r=Store.buyShopProduct(id);if(!r.ok){Toast.show(r.reason==='insufficient_funds'?'B币余额不足':'商品当前不可购买','warning');return false;}Toast.show('购买成功，已放入我的装扮','success');Router.refresh();return true;}});};
  window.equipShopItem=function(id){Store.equipShopEntitlement(id);Toast.show('装扮已全局生效','success');Router.refresh();};
  window.unequipShopSlot=function(slot){Store.unequipShopSlot(slot);Toast.show('装扮已卸下','info');Router.refresh();};
  window.handleProfileCover=function(input){const file=input?.files?.[0];if(!file)return;const allowed=['image/jpeg','image/png','image/webp'];if(!allowed.includes(file.type)){Toast.show('Cover 仅支持 JPG、PNG 或 WebP','warning');input.value='';return;}if(file.size>5*1024*1024){Toast.show('Cover 不能超过 5MB','warning');input.value='';return;}const reader=new FileReader();reader.onload=()=>{Store.setProfileCover(reader.result);Toast.show('个人资料背景已更新','success');Router.refresh();};reader.readAsDataURL(file);};
  window.removeProfileCover=function(){Store.removeProfileCover();Toast.show('个人资料背景已移除','info');Router.refresh();};

  // ============================================
  // 7. User Profile (/users/[name])
  // ============================================
  P.userProfile = function(name, params) {
    const user = MockData.getUser(name);
    if (!user) return P.notFound('用户不存在', '该用户可能已注销或不存在。');

    const isOwnProfile = name === Store.state.user.name;
    const requestedTab = params.tab || 'posts';
    const allowedTabs = ['posts', 'replies', 'favorites', 'about', ...(isOwnProfile ? ['points'] : [])];
    const tab = allowedTabs.includes(requestedTab) ? requestedTab : 'posts';
    const userPosts = MockData.posts.filter(p => p.author === name);
    const userReplies = Object.values(MockData.replies).flat().filter(r => r.author === name);
    const favorites = MockData.posts.filter(p => Store.isFavorite(p.id));

    const tabs = [
      { key: 'posts', label: '发布', count: userPosts.length },
      { key: 'replies', label: '回复', count: userReplies.length },
      { key: 'favorites', label: '收藏', count: favorites.length },
      ...(isOwnProfile ? [{ key: 'points', label: '我的积分' }] : []),
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
                  ${Store.canViewPost(MockData.getPost(r.topicId))
                    ? `<a href="#/topics/${r.topicId}?reply=${r.id}">${C.escapeHtml(MockData.getPost(r.topicId)?.title || '帖子')}</a>`
                    : `<a href="#/topics/${r.topicId}" class="level-locked-title">${C.icon('lock', 13)} 该内容仅 LV.${MockData.getPost(r.topicId)?.visibilityLevel || 1} 及以上可见</a>`}
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
    } else if (tab === 'points') {
      const expPercent = Math.min(100, Math.round((user.exp / user.expNext) * 100));
      const pointRecords = [
        { time: '2026-08-02 18:30', icon: 'message-square', title: '发布优质回复', desc: '主题：Rust 异步运行时该怎么选？', type: '经验值', amount: 12 },
        { time: '2026-08-01 15:20', icon: 'star', title: '内容被设为精华', desc: '社区内容奖励', type: 'B币', amount: 50 },
        { time: '2026-07-30 10:08', icon: 'award', title: '参与板块管理', desc: '完成本周版务任务', type: '贡献值', amount: 8 },
        { time: '2026-07-28 14:20', icon: 'coins', title: '解锁付费内容', desc: '主题内容消费', type: 'B币', amount: -20 },
        { time: '2026-07-25 09:12', icon: 'log-in', title: '连续签到 7 天', desc: '连续活跃奖励', type: '经验值', amount: 30 }
      ];
      tabContent = `
        <div class="points-page">
          <div class="points-summary-grid">
            <div class="points-summary-card is-coins">
              <div class="points-summary-icon">${C.icon('coins', 20)}</div>
              <div><div class="points-summary-label">B币余额</div><div class="points-summary-value">${user.coins}</div><div class="points-summary-desc">可用于解锁付费内容</div></div>
            </div>
            <div class="points-summary-card is-exp">
              <div class="points-summary-icon">${C.icon('trophy', 20)}</div>
              <div class="points-summary-main">
                <div class="points-summary-label">经验值</div>
                <div class="points-summary-value">${user.exp}<small> / ${user.expNext}</small></div>
                <div class="points-progress"><span style="width: ${expPercent}%"></span></div>
                <div class="points-summary-desc">距离 LV.${user.level + 1} 还差 ${Math.max(0, user.expNext - user.exp)} 经验</div>
              </div>
            </div>
            <div class="points-summary-card is-contribution">
              <div class="points-summary-icon">${C.icon('award', 20)}</div>
              <div><div class="points-summary-label">贡献值</div><div class="points-summary-value">${user.contribution}</div><div class="points-summary-desc">来自优质内容与社区治理</div></div>
            </div>
          </div>

          <div class="points-content-grid">
            <section class="points-records">
              <div class="points-section-header"><div><h2>最近明细</h2><p>最近 30 天的积分与货币变动</p></div><button class="btn btn-ghost btn-sm" onclick="Toast.show('已加载全部明细', 'info')">查看全部</button></div>
              <div class="points-record-list">
                ${pointRecords.map(record => `
                  <div class="points-record-row">
                    <div class="points-record-icon">${C.icon(record.icon, 16)}</div>
                    <div class="points-record-main"><div class="points-record-title">${record.title}</div><div class="points-record-desc">${record.desc} · ${record.time}</div></div>
                    <div class="points-record-change ${record.amount > 0 ? 'is-positive' : 'is-negative'}"><strong>${record.amount > 0 ? '+' : ''}${record.amount}</strong><span>${record.type}</span></div>
                  </div>`).join('')}
              </div>
            </section>

            <aside class="points-rules">
              <h2>如何获得</h2>
              <ul>
                <li><span>${C.icon('edit-3', 16)}</span><div><strong>发布与回复</strong><small>优质内容可获得经验值和 B币</small></div></li>
                <li><span>${C.icon('star', 16)}</span><div><strong>内容被推荐</strong><small>精华内容可获得额外奖励</small></div></li>
                <li><span>${C.icon('shield', 16)}</span><div><strong>参与社区治理</strong><small>版务和举报处理增加贡献值</small></div></li>
              </ul>
              <a href="#/users/${name}?tab=about" class="points-rules-link">查看等级与账号信息 ${C.icon('chevron-right', 14)}</a>
            </aside>
          </div>
        </div>
      `;
    } else if (tab === 'about') {
      tabContent = `
        <section class="profile-about-panel">
          <header class="profile-about-heading">
            <div><h2>账号资料</h2><p>公开展示的社区身份与活跃信息</p></div>
            ${isOwnProfile ? C.button({ text: '编辑资料', variant: 'ghost', size: 'sm', icon: 'edit-3', href: '#/settings' }) : ''}
          </header>
          <dl class="profile-about-list">
            <div class="profile-about-item"><dt>用户 ID</dt><dd>${C.escapeHtml(user.name)}</dd></div>
            <div class="profile-about-item"><dt>社区角色</dt><dd>${user.roles.includes('admin') ? '管理员' : user.roles.includes('moderator') ? '版主' : '普通会员'}</dd></div>
            <div class="profile-about-item"><dt>注册时间</dt><dd>${user.joinedAt}</dd></div>
            <div class="profile-about-item"><dt>最后活跃</dt><dd>${user.lastActive}</dd></div>
            <div class="profile-about-item"><dt>当前等级</dt><dd>LV.${user.level}<span class="profile-about-note">${user.exp} / ${user.expNext} EXP</span></dd></div>
            <div class="profile-about-item"><dt>社区资产</dt><dd>${user.coins} B币<span class="profile-about-note">${user.contribution} 贡献值</span></dd></div>
            ${user.modBoards.length > 0 ? `<div class="profile-about-item"><dt>管理板块</dt><dd>${user.modBoards.map(slug => MockData.getBoard(slug)?.name || slug).join('、')}</dd></div>` : ''}
            <div class="profile-about-item is-wide"><dt>个人简介</dt><dd>${C.escapeHtml(user.bio)}</dd></div>
          </dl>
        </section>
      `;
    }

    const content = `
      <div class="container" style="padding-top: var(--space-6); padding-bottom: var(--space-6);">
        ${C.breadcrumb([
          { label: '首页', href: '#/' },
          { label: name }
        ])}

        <div class="card profile-page-card">
          <div class="profile-cover ${((isOwnProfile ? Store.state.profileCover : user.profileCover) || '') ? 'has-image' : ''}" ${((isOwnProfile ? Store.state.profileCover : user.profileCover) || '') ? `style="background-image:url('${C.escapeHtml((isOwnProfile ? Store.state.profileCover : user.profileCover) || '')}')"` : ''} role="img" aria-label="${C.escapeHtml(name)} 的个人资料背景"></div>
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
              <div class="profile-overview">
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
                <div class="profile-level">${C.levelProgress(user.exp, user.expNext, user.level)}</div>
              </div>
            </div>
            <div class="profile-actions">
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
                <input type="checkbox" id="reg-agreement" style="margin-top: 3px;" />
                <span class="text-secondary">我已阅读并同意 <button type="button" class="text-link" onclick="Toast.show('用户协议预览已打开', 'info')">用户协议</button> 和 <button type="button" class="text-link" onclick="Toast.show('隐私政策预览已打开', 'info')">隐私政策</button></span>
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
    const requestedTab = params.tab || 'all';
    const tab = ['all', 'unread', 'reply', 'like', 'mention', 'system'].includes(requestedTab) ? requestedTab : 'all';
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
    const requestedTab = params.tab || 'posts';
    const tab = ['posts', 'users'].includes(requestedTab) ? requestedTab : 'posts';
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
    const requestedTab = params.tab || 'posts';
    const tab = ['posts', 'users', 'tags'].includes(requestedTab) ? requestedTab : 'posts';
    const page = Math.max(1, parseInt(params.page) || 1);

    let results = [];
    if (q) {
      const qLower = q.toLowerCase();
      results = MockData.posts.filter(p => {
        if (!Store.canViewPost(p)) return false;
        return p.title.toLowerCase().includes(qLower) ||
          p.summary.toLowerCase().includes(qLower) ||
          p.tags.some(t => t.toLowerCase().includes(qLower));
      });
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
              <form class="search-form" role="search" onsubmit="event.preventDefault(); Router.navigate('/search?q=' + encodeURIComponent(this.querySelector('input').value.trim()) + '&tab=${tab}');">
                <button type="submit" class="search-submit" aria-label="提交搜索">${C.icon('search', 17)}</button>
                <input type="search" name="q" class="search-input" value="${q}" placeholder="搜索帖子、用户、标签..." aria-label="搜索帖子、用户和标签" autocomplete="off" />
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
    const user = Store.state.user;

    const navItems = [
      { key: 'profile', label: '个人资料', icon: 'user' },
      { key: 'security', label: '账号安全', icon: 'shield' },
      { key: 'devices', label: '登录设备', icon: 'monitor' },
      { key: 'notifications', label: '通知设置', icon: 'bell' },
      { key: 'oauth', label: 'OAuth 授权', icon: 'key' }
    ];
    const requestedTab = params.tab || 'profile';
    const tab = navItems.some(item => item.key === requestedTab) ? requestedTab : 'profile';

    let tabContent = '';

    if (tab === 'profile') {
      tabContent = `
        <div class="settings-section">
          <div class="settings-section-header">
            <div class="settings-section-title">基本信息</div>
          </div>
          <div class="settings-section-body">
            <div class="profile-cover-editor">
              <div class="profile-cover-editor-preview ${Store.state.profileCover ? 'has-image' : ''}" id="profile-cover-preview" ${Store.state.profileCover ? `style="background-image:url('${C.escapeHtml(Store.state.profileCover)}')"` : ''}>
                <div class="profile-cover-editor-placeholder">${C.icon('image',24)}<span>个人资料背景</span></div>
              </div>
              <div class="profile-cover-editor-actions">
                <div><strong>主页 Cover</strong><p>建议使用 1600 × 480 横图；支持 JPG、PNG、WebP，最大 5MB。</p></div>
                <div class="profile-cover-editor-buttons">
                  <input type="file" class="file-picker-input" id="profile-cover-input" accept="image/jpeg,image/png,image/webp" onchange="handleProfileCover(this)" />
                  <label class="btn btn-secondary btn-sm" for="profile-cover-input">${C.icon('image',14)}<span>${Store.state.profileCover ? '更换背景' : '上传背景'}</span></label>
                  ${Store.state.profileCover ? C.button({ text:'移除', variant:'ghost', size:'sm', onClick:'removeProfileCover()' }) : ''}
                </div>
              </div>
            </div>
            <div style="display: flex; align-items: center; gap: var(--space-4); margin-bottom: var(--space-5);">
              ${C.avatar(user.name, 'xl')}
              <div>
                ${C.button({ text: '更换头像', variant: 'secondary', size: 'sm', onClick: "Toast.show('头像上传功能开发中', 'info')" })}
                <p class="text-tertiary" style="font-size: var(--text-xs); margin-top: var(--space-2);">支持 JPG、PNG 格式，大小不超过 2MB</p>
              </div>
            </div>
            <div class="form-grid form-grid-2">
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
            ${C.button({ text: '取消', variant: 'secondary', onClick: 'Router.refresh()' })}
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
              ${C.button({ text: '修改密码', variant: 'secondary', size: 'sm', onClick: "Toast.show('密码修改面板已打开', 'info')" })}
            </div>
            <div class="settings-row">
              <div class="settings-row-label">
                <div class="settings-row-title">邮箱</div>
                <div class="settings-row-desc">chaos@example.com</div>
              </div>
              ${C.button({ text: '更换邮箱', variant: 'secondary', size: 'sm', onClick: "Toast.show('邮箱修改面板已打开', 'info')" })}
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
          <div class="settings-section-body device-list">
            ${devices.map(d => `
              <div class="device-card ${d.isCurrent ? 'is-current' : ''}">
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
              <a href="#/settings?tab=${item.key}" class="settings-nav-item ${tab === item.key ? 'is-active' : ''}">
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

  function classifyVideoUrl(rawUrl) {
    try {
      const url = new URL(rawUrl);
      if (url.protocol !== 'https:' || url.username || url.password || url.port) return { ok: false, label: '仅允许无凭据、无自定义端口的 HTTPS URL' };
      const host = url.hostname.toLowerCase();
      const path = url.pathname.toLowerCase();
      if (host === 'ixigua.com' || host.endsWith('.ixigua.com')) return { ok: true, type: 'xigua', label: '西瓜视频公开页面（后端确认官方嵌入或降级外链）' };
      if (path.endsWith('.m3u8')) return { ok: true, type: 'hls', label: 'HLS playlist（后端将校验分片、Key、Map 与来源）' };
      if (/\.(mp4|webm|ogv|ogg|mov)$/.test(path)) return { ok: true, type: 'direct', label: '直接视频 URL（最终类型以后端 MIME 探测为准）' };
      return { ok: true, type: 'unknown', label: '扩展名未知，将交由后端受限探测；不保证可嵌入' };
    } catch (_) {
      return { ok: false, label: '请输入完整、有效的 HTTPS URL' };
    }
  }

  window.previewVideoUrl = function() {
    const result = classifyVideoUrl(document.getElementById('video-insert-url')?.value.trim() || '');
    const status = document.getElementById('video-insert-status');
    if (!status) return;
    status.className = `notice ${result.ok ? 'notice-info' : 'notice-warning'}`;
    status.innerHTML = `${C.icon(result.ok ? 'info' : 'alert-triangle', 16)} ${C.escapeHtml(result.label)}`;
  };

  window.openVideoInsertDialog = function() {
    const cfg = Store.state.videoConfig;
    if (!cfg.enabled) {
      Toast.show('视频插件已由管理员停用', 'warning');
      return;
    }
    Modal.open({
      title: '插入视频 URL',
      confirmText: '确认插入',
      content: `<div class="input-wrapper"><label class="input-label" for="video-insert-url">视频地址</label><input type="url" class="input-field" id="video-insert-url" placeholder="https://example.com/video.mp4 或 stream.m3u8" oninput="previewVideoUrl()" autocomplete="off" /><div class="input-hint">支持 MP4、WebM、OGV、经探测的 MOV、HLS (.m3u8) 和西瓜视频公开页面。</div></div><div id="video-insert-status" class="notice notice-info" style="margin-top:var(--space-3);">${C.icon('shield-check',16)} 前端仅提供格式提示；发布时由后端重新执行白名单、SSRF、DNS、重定向和媒体策略校验。</div><label style="display:flex;gap:var(--space-2);align-items:flex-start;margin-top:var(--space-3);font-size:var(--text-sm);"><input type="checkbox" id="video-rights-confirm" /> <span>我确认有权分享该地址，并理解平台视频可能降级为外链且不会被本站下载或转存。</span></label>`,
      onConfirm: () => {
        const rawUrl = document.getElementById('video-insert-url')?.value.trim() || '';
        const rightsConfirmed = document.getElementById('video-rights-confirm')?.checked;
        const result = classifyVideoUrl(rawUrl);
        if (!result.ok) {
          Toast.show(result.label, 'warning');
          return false;
        }
        if (!rightsConfirmed) {
          Toast.show('请先确认分享权利与第三方平台限制', 'warning');
          return false;
        }
        if (result.type === 'xigua' && !cfg.xiguaEnabled || result.type === 'hls' && !cfg.hlsEnabled || result.type === 'direct' && !cfg.directEnabled) {
          Toast.show('该视频类型当前已由管理员停用', 'warning');
          return false;
        }
        wrapSelection(`\n@[video](${rawUrl})\n`, '');
        Toast.show('视频引用已插入，发布时将由后端重新验证', 'success');
        return true;
      }
    });
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

  function preservePublishParams(extra) {
    const data = collectPublishForm();
    Router.updateParams({
      type: extra.type || data.type,
      title: data.title,
      board: data.board,
      content: data.content,
      ...extra
    });
  }

  window.switchPublishType = function(type) {
    Store.setPublishType(type);
    preservePublishParams({ type });
  };

  window.switchEditorTab = function(tab) {
    Store.setEditorTab(tab);
    preservePublishParams({});
  };

  window.addPublishTag = function(input) {
    const value = input.value.trim();
    if (!value) return;
    const wrapper = input.closest('.tag-input-wrapper');
    const existing = Array.from(wrapper.querySelectorAll('.tag-chip')).map(el => el.childNodes[0]?.textContent.trim().toLowerCase());
    if (existing.includes(value.toLowerCase())) {
      Toast.show('标签已存在', 'warning');
      return;
    }
    if (existing.length >= 5) {
      Toast.show('最多添加 5 个标签', 'warning');
      return;
    }
    const chip = document.createElement('span');
    chip.className = 'tag-chip';
    chip.append(document.createTextNode(value + ' '));
    const remove = document.createElement('button');
    remove.type = 'button';
    remove.className = 'tag-chip-remove';
    remove.setAttribute('aria-label', '移除标签');
    remove.innerHTML = C.icon('x', 10);
    remove.onclick = () => chip.remove();
    chip.append(remove);
    wrapper.insertBefore(chip, input);
    input.value = '';
  };

  window.handleArticleCover = function(input) {
    const file = input.files?.[0];
    if (!file) return;
    if (!['image/jpeg', 'image/png', 'image/webp'].includes(file.type)) {
      input.value = '';
      Toast.show('封面仅支持 JPG、PNG 或 WebP', 'warning');
      return;
    }
    if (file.size > 8 * 1024 * 1024) {
      input.value = '';
      Toast.show('封面图片不能超过 8 MB', 'warning');
      return;
    }

    const reader = new FileReader();
    reader.onload = () => {
      articleCoverState.file = file;
      articleCoverState.name = file.name;
      articleCoverState.url = String(reader.result || '');
      const field = document.getElementById('article-cover-field');
      const preview = document.getElementById('article-cover-preview');
      const meta = field?.querySelector('.article-cover-meta');
      const name = document.getElementById('article-cover-name');
      field?.classList.add('has-cover');
      if (preview) {
        preview.src = articleCoverState.url;
        preview.hidden = false;
      }
      if (meta) meta.hidden = false;
      if (name) name.textContent = file.name;
      Toast.show('文章封面已选择', 'success');
    };
    reader.onerror = () => Toast.show('无法读取封面图片，请重新选择', 'danger');
    reader.readAsDataURL(file);
  };

  window.removeArticleCover = function() {
    articleCoverState.file = null;
    articleCoverState.name = '';
    articleCoverState.url = '';
    const input = document.getElementById('article-cover-input');
    if (input) input.value = '';
    const field = document.getElementById('article-cover-field');
    const preview = document.getElementById('article-cover-preview');
    const meta = field?.querySelector('.article-cover-meta');
    field?.classList.remove('has-cover');
    if (preview) {
      preview.removeAttribute('src');
      preview.hidden = true;
    }
    if (meta) meta.hidden = true;
  };

  window.validatePublishAttachment = function(input, maxMb) {
    const file = input.files?.[0];
    const name = document.getElementById('publish-attachment-name');
    if (!file) {
      if (name) name.textContent = '未选择文件';
      return;
    }
    if (file.size > maxMb * 1024 * 1024) {
      input.value = '';
      if (name) name.textContent = '未选择文件';
      Toast.show(`该文件超过当前等级的 ${maxMb} MB 上限`, 'warning');
      return;
    }
    if (name) {
      name.textContent = file.name;
      name.title = file.name;
    }
    Toast.show('附件已选择，发布后按权限生成临时访问链接', 'success');
  };

  function collectPublishForm() {
    return {
      type: document.querySelector('.type-switch button.active')?.textContent.includes('专栏') ? 'article' : 'topic',
      title: document.getElementById('publish-title')?.value.trim() || '',
      board: document.getElementById('publish-board')?.value || '',
      content: document.getElementById('publish-content')?.value.trim() || '',
      visibilityLevel: Number(document.getElementById('publish-visibility-level')?.value || 1),
      cover: articleCoverState.url,
      coverName: articleCoverState.name,
      tags: Array.from(document.querySelectorAll('.tag-chip')).map(el => el.childNodes[0]?.textContent.trim()).filter(Boolean)
    };
  }

  window.requestAiFormat = function() {
    const textarea = document.getElementById('publish-content');
    if (!textarea?.value.trim()) {
      Toast.show('请先输入需要格式化的内容', 'warning');
      return;
    }
    Modal.open({
      title: 'AI 格式化预览',
      content: `<div class="notice notice-warning" style="margin-bottom:var(--space-3);">${C.icon('shield-check', 16)} 内容会按后台脱敏策略通过后端 AI Gateway 处理；模型不会直接发布或覆盖原文。</div><p>建议调整标题层级、列表间距和代码块语言标记，不改变原意。</p><div class="data-table-wrapper" style="margin-top:var(--space-3);padding:var(--space-3);"><strong>变更摘要</strong><ul style="margin-top:var(--space-2);line-height:1.8;"><li>• 统一 Markdown 标题层级</li><li>• 修复列表与代码块间距</li><li>• 保留链接、附件和受限内容标记</li></ul></div>`,
      confirmText: '采纳建议',
      onConfirm: () => Toast.show('已采纳 AI 格式建议，发布前仍会执行安全校验', 'success')
    });
  };

  window.requestAiSeo = function() {
    Modal.open({
      title: 'AI SEO 建议',
      content: `<div class="form-grid form-grid-1"><div class="input-wrapper"><label class="input-label">SEO 标题建议</label><input class="input-field" value="${C.escapeHtml(document.getElementById('publish-title')?.value || '文章标题')}｜BBLBB" /></div><div class="input-wrapper"><label class="input-label">Meta Description</label><textarea class="input-field" rows="3">根据公开内容生成的摘要草稿；发布后仍需进行隐藏内容泄漏检查。</textarea></div></div><div class="input-hint" style="margin-top:var(--space-3);">模型结果仅为草稿，不会自动写入 sitemap、OpenGraph 或 JSON-LD。</div>`,
      confirmText: '保存为草稿',
      onConfirm: () => Toast.show('SEO 建议已保存为草稿', 'success')
    });
  };

  window.savePublishDraft = function() {
    const draft = Store.saveDraft(collectPublishForm());
    Toast.show(`草稿已于 ${draft.savedAt} 保存`, 'success');
  };

  window.submitPublish = function(status = 'published') {
    const data = collectPublishForm();
    if (!data.title) {
      Toast.show('请输入标题', 'warning');
      document.getElementById('publish-title')?.focus();
      return;
    }
    if (!data.board) {
      Toast.show('请选择板块', 'warning');
      document.getElementById('publish-board')?.focus();
      return;
    }
    if (data.type === 'article' && !data.cover) {
      Toast.show('专栏文章必须设置封面图', 'warning');
      document.getElementById('article-cover-input')?.focus();
      return;
    }
    const currentLevel = Math.max(1, Number(Store.state.user.level) || 1);
    if (data.visibilityLevel > currentLevel) {
      Toast.show(`最低可见等级不能高于你的当前等级 LV.${currentLevel}`, 'warning');
      document.getElementById('publish-visibility-level')?.focus();
      return;
    }
    if (!data.content) {
      Toast.show('请输入正文内容', 'warning');
      return;
    }
    Modal.open({
      title: status === 'pending' ? '提交审核' : '确认发布',
      content: `<p>确定${status === 'pending' ? '提交审核' : '立即发布'}《${C.escapeHtml(data.title)}》吗？</p>`,
      confirmText: status === 'pending' ? '提交审核' : '发布',
      onConfirm: () => {
        const result = Store.createPost(data, status);
        if (!result.ok) {
          Toast.show(result.reason === 'visibility_level_exceeds_author' ? `最低可见等级不能高于你的当前等级 LV.${Store.state.user.level}` : '发布失败，请检查发布设置', 'warning');
          return false;
        }
        Toast.show(status === 'pending' ? '已提交审核' : '发布成功', 'success');
        Router.navigate('/topics/' + result.post.id);
      }
    });
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
    const email = document.getElementById('reg-email')?.value.trim();
    const password = document.getElementById('reg-password')?.value;
    const confirm = document.getElementById('reg-confirm')?.value;
    const agreed = document.getElementById('reg-agreement')?.checked;
    
    if (!username || username.length < 3) {
      Toast.show('用户名至少 3 个字符', 'danger');
      return;
    }
    if (!email || !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) {
      Toast.show('请输入有效邮箱', 'danger');
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
    if (!agreed) {
      Toast.show('请先同意用户协议和隐私政策', 'warning');
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
