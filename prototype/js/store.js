// ============================================
// BBLBB Global Store & State Management
// ============================================

window.Store = (function() {
  // Deep clone initial state
  let state = {
    user: JSON.parse(JSON.stringify(MockData.currentUser)),
    theme: 'light',
    favorites: [],
    replyUnlocked: {},  // topicId -> true
    paidUnlocked: {},   // topicId -> true
    notifications: JSON.parse(JSON.stringify(MockData.notifications)),
    devices: JSON.parse(JSON.stringify(MockData.loginDevices)),
    reports: JSON.parse(JSON.stringify(MockData.reports)),
    oauthClients: JSON.parse(JSON.stringify(MockData.oauthClients)),
    plugins: JSON.parse(JSON.stringify(MockData.plugins)),
    postOverrides: {},
    createdPosts: [],
    drafts: {},
    dynamicReplies: {},
    likedPosts: [],
    likedReplies: [],
    boardFollows: [],
    userFollows: [],
    storageConfig: {
      backend: 'local',
      localPath: '/var/lib/bblbb/uploads',
      endpoint: 'https://s3.amazonaws.com',
      region: 'ap-southeast-1',
      bucket: 'bblbb-attachments',
      accessKeyId: '',
      secretConfigured: false,
      publicBaseUrl: '',
      pathStyle: false,
      presignedUploads: true,
      signedUrlTtl: 300,
      maxUploadMb: 20,
      defaultAttachmentTtlDays: 30,
      maxAttachmentTtlDays: 365,
      bucketPrivate: true,
      connectionStatus: 'untested',
      lastTestedAt: ''
    },
    lastReplyAt: {},
    mobileDrawerOpen: false,
    userMenuOpen: false,
    publishType: 'topic',
    editorTab: 'write'
  };

  // Load persisted state
  try {
    const saved = localStorage.getItem('bblbb_state');
    if (saved) {
      const parsed = JSON.parse(saved);
      state = { ...state, ...parsed };
    }
  } catch (e) {}

  // Load theme from localStorage or system preference
  try {
    const savedTheme = localStorage.getItem('bblbb_theme');
    if (savedTheme) {
      state.theme = savedTheme;
    } else if (window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches) {
      state.theme = 'dark';
    }
  } catch (e) {}

  // Apply theme
  function applyTheme() {
    if (state.theme === 'dark') {
      document.documentElement.classList.add('dark');
    } else {
      document.documentElement.classList.remove('dark');
    }
  }
  applyTheme();

  // Persist state
  function persist() {
    try {
      const toSave = {
        favorites: state.favorites,
        replyUnlocked: state.replyUnlocked,
        paidUnlocked: state.paidUnlocked,
        user: state.user,
        devices: state.devices,
        reports: state.reports,
        oauthClients: state.oauthClients,
        plugins: state.plugins,
        postOverrides: state.postOverrides,
        createdPosts: state.createdPosts,
        drafts: state.drafts,
        dynamicReplies: state.dynamicReplies,
        likedPosts: state.likedPosts,
        likedReplies: state.likedReplies,
        boardFollows: state.boardFollows,
        userFollows: state.userFollows,
        storageConfig: state.storageConfig,
        lastReplyAt: state.lastReplyAt
      };
      localStorage.setItem('bblbb_state', JSON.stringify(toSave));
      localStorage.setItem('bblbb_theme', state.theme);
    } catch (e) {}
  }

  // Listeners
  const listeners = new Set();

  function subscribe(fn) {
    listeners.add(fn);
    return () => listeners.delete(fn);
  }

  function notify() {
    listeners.forEach(fn => fn(state));
  }

  // Actions
  function toggleTheme() {
    state.theme = state.theme === 'light' ? 'dark' : 'light';
    applyTheme();
    persist();
    notify();
  }

  function setTheme(theme) {
    state.theme = theme;
    applyTheme();
    persist();
    notify();
  }

  function toggleFavorite(postId) {
    const idx = state.favorites.indexOf(postId);
    if (idx === -1) {
      state.favorites.push(postId);
      Toast.show('已收藏', 'success');
    } else {
      state.favorites.splice(idx, 1);
      Toast.show('已取消收藏', 'info');
    }
    persist();
    notify();
  }

  function isFavorite(postId) {
    return state.favorites.includes(postId);
  }

  function unlockReply(topicId) {
    state.replyUnlocked[topicId] = true;
    persist();
    notify();
  }

  function isReplyUnlocked(topicId) {
    return !!state.replyUnlocked[topicId];
  }

  function unlockPaid(topicId, price) {
    if (state.user.coins < price) {
      Toast.show('B币余额不足', 'danger');
      return false;
    }
    state.user.coins -= price;
    state.paidUnlocked[topicId] = true;
    Toast.show(`支付成功，扣除 ${price} B币`, 'success');
    persist();
    notify();
    return true;
  }

  function isPaidUnlocked(topicId) {
    return !!state.paidUnlocked[topicId];
  }

  function getPost(postId) {
    const base = state.createdPosts.find(item => item.id === Number(postId)) || MockData.getPost(Number(postId));
    return base ? { ...base, ...(state.postOverrides[postId] || {}) } : null;
  }

  function getAllPosts() {
    return [...state.createdPosts, ...MockData.posts].map(post => ({ ...post, ...(state.postOverrides[post.id] || {}) }));
  }

  function saveDraft(draft) {
    state.drafts[draft.type || 'topic'] = { ...draft, savedAt: new Date().toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' }) };
    persist();
    notify();
    return state.drafts[draft.type || 'topic'];
  }

  function createPost(data, status = 'published') {
    const id = Math.max(300, ...getAllPosts().map(post => Number(post.id))) + 1;
    const now = new Date().toLocaleString('zh-CN', { hour12: false }).replace(/\//g, '-');
    const post = {
      id, type: data.type || 'topic', title: data.title, summary: data.summary || data.content.slice(0, 100),
      content: data.content, author: state.user.name, board: data.board, tags: data.tags || [],
      views: 0, replies: 0, likes: 0, createdAt: now, updatedAt: now, lastReplyAt: now,
      isPinned: false, isEssence: false, status,
      restricted: data.restricted || null
    };
    state.createdPosts.unshift(post);
    delete state.drafts[data.type || 'topic'];
    persist();
    notify();
    return post;
  }

  function getReplies(topicId) {
    return [...MockData.getReplies(Number(topicId)), ...(state.dynamicReplies[topicId] || [])];
  }

  function togglePostLike(postId) {
    const post = getPost(postId);
    if (!post) return;
    const index = state.likedPosts.indexOf(postId);
    const liked = index === -1;
    if (liked) state.likedPosts.push(postId); else state.likedPosts.splice(index, 1);
    state.postOverrides[postId] = { ...(state.postOverrides[postId] || {}), likes: Math.max(0, post.likes + (liked ? 1 : -1)) };
    persist();
    notify();
  }

  function isPostLiked(postId) { return state.likedPosts.includes(postId); }

  function toggleReplyLike(topicId, replyId) {
    const key = `${topicId}:${replyId}`;
    const index = state.likedReplies.indexOf(key);
    const liked = index === -1;
    if (liked) state.likedReplies.push(key); else state.likedReplies.splice(index, 1);
    const dynamic = state.dynamicReplies[topicId] || [];
    const reply = dynamic.find(item => item.id === replyId);
    if (reply) reply.likes = Math.max(0, reply.likes + (liked ? 1 : -1));
    persist();
    notify();
  }

  function isReplyLiked(topicId, replyId) { return state.likedReplies.includes(`${topicId}:${replyId}`); }

  function addReply(topicId, content) {
    const now = Date.now();
    if (state.lastReplyAt[topicId] && now - state.lastReplyAt[topicId] < 3000) return { ok: false, reason: 'rate_limit' };
    const replies = getReplies(topicId);
    const reply = {
      id: 1000 + now,
      topicId,
      floor: replies.length + 1,
      author: state.user.name,
      content,
      likes: 0,
      createdAt: new Date(now).toLocaleString('zh-CN', { hour12: false }).replace(/\//g, '-'),
      isAuthor: getPost(topicId)?.author === state.user.name
    };
    if (!state.dynamicReplies[topicId]) state.dynamicReplies[topicId] = [];
    state.dynamicReplies[topicId].push(reply);
    state.lastReplyAt[topicId] = now;
    const post = getPost(topicId);
    if (post) state.postOverrides[topicId] = { ...(state.postOverrides[topicId] || {}), replies: post.replies + 1 };
    if (post?.restricted?.type === 'reply') state.replyUnlocked[topicId] = true;
    persist();
    notify();
    return { ok: true, reply, unlocked: post?.restricted?.type === 'reply' };
  }

  function createReport({ topicId, reason, detail = '', targetType = 'post', replyId = null }) {
    const post = getPost(topicId);
    if (!post) return null;
    const id = `R-${1100 + state.reports.length}`;
    const report = {
      id, reason, priority: reason === '违法违规' ? 'high' : 'medium', status: 'pending',
      reporter: state.user.name, reportedUser: post.author, board: post.board,
      content: replyId ? `回复 #${replyId}` : post.title, evidence: detail || '用户未补充说明',
      contentUrl: `#/topics/${topicId}`, createdAt: new Date().toLocaleString('zh-CN', { hour12: false }).replace(/\//g, '-'),
      history: [{ id: 1, type: 'report', operator: state.user.name, time: new Date().toLocaleString('zh-CN', { hour12: false }).replace(/\//g, '-') }]
    };
    state.reports.unshift(report);
    persist();
    notify();
    return report;
  }

  function markNotificationRead(id) {
    const n = state.notifications.find(n => n.id === id);
    if (n) {
      n.read = true;
      persist();
      notify();
    }
  }

  function markAllNotificationsRead() {
    state.notifications.forEach(n => n.read = true);
    Toast.show('已全部标记为已读', 'success');
    persist();
    notify();
  }

  function getUnreadCount() {
    return state.notifications.filter(n => !n.read).length;
  }

  function removeDevice(deviceId) {
    state.devices = state.devices.filter(d => d.id !== deviceId);
    Toast.show('设备已下线', 'success');
    persist();
    notify();
  }

  function updateReport(reportId, updates) {
    const r = state.reports.find(r => r.id === reportId);
    if (r) {
      Object.assign(r, updates);
      persist();
      notify();
    }
  }

  function addReportHistory(reportId, action) {
    const r = state.reports.find(r => r.id === reportId);
    if (r) {
      r.history.push({
        id: r.history.length + 1,
        ...action,
        time: new Date().toLocaleString('zh-CN', { hour12: false }).replace(/\//g, '-')
      });
      persist();
      notify();
    }
  }

  function adjustCoins(amount, reason) {
    state.user.coins += amount;
    Toast.show(`${amount > 0 ? '增加' : '扣除'} ${Math.abs(amount)} B币`, amount > 0 ? 'success' : 'warning');
    persist();
    notify();
  }

  function addOAuthClient(client) {
    const newClient = {
      id: 'oc' + (state.oauthClients.length + 1),
      ...client,
      recentAuthUsers: 0,
      createdAt: new Date().toISOString().split('T')[0]
    };
    state.oauthClients.push(newClient);
    Toast.show('应用创建成功', 'success');
    persist();
    notify();
    return newClient;
  }

  function togglePlugin(pluginId) {
    const p = state.plugins.find(p => p.id === pluginId);
    if (p) {
      if (p.status === 'error') {
        Toast.show('插件存在错误，无法启用', 'danger');
        return;
      }
      p.status = p.status === 'enabled' ? 'disabled' : 'enabled';
      Toast.show(p.status === 'enabled' ? '插件已启用' : '插件已禁用', 'success');
      persist();
      notify();
    }
  }

  function updateStorageConfig(config) {
    const allowed = ['local', 's3'];
    const backend = allowed.includes(config.backend) ? config.backend : 'local';
    state.storageConfig = {
      ...state.storageConfig,
      ...config,
      backend,
      maxUploadMb: Math.max(1, Math.min(1024, Number(config.maxUploadMb) || 20)),
      defaultAttachmentTtlDays: Math.max(1, Math.min(365, Number(config.defaultAttachmentTtlDays) || 30)),
      maxAttachmentTtlDays: Math.max(1, Math.min(3650, Number(config.maxAttachmentTtlDays) || 365)),
      signedUrlTtl: Math.max(60, Math.min(3600, Number(config.signedUrlTtl) || 300)),
      secretConfigured: state.storageConfig.secretConfigured || !!config.secretAccessKey,
      connectionStatus: config.backend === 's3' ? 'untested' : 'ready'
    };
    delete state.storageConfig.secretAccessKey;
    persist();
    notify();
    return state.storageConfig;
  }

  function testStorageConnection() {
    const cfg = state.storageConfig;
    const valid = cfg.backend === 'local' ? !!cfg.localPath : !!(cfg.endpoint && cfg.region && cfg.bucket && cfg.accessKeyId && cfg.secretConfigured);
    state.storageConfig.connectionStatus = valid ? 'connected' : 'error';
    state.storageConfig.lastTestedAt = new Date().toLocaleString('zh-CN', { hour12: false }).replace(/\//g, '-');
    persist();
    notify();
    return valid;
  }

  function setPublishType(type) {
    state.publishType = type;
    notify();
  }

  function setEditorTab(tab) {
    state.editorTab = tab;
    notify();
  }

  function toggleMobileDrawer() {
    state.mobileDrawerOpen = !state.mobileDrawerOpen;
    notify();
  }

  function closeMobileDrawer() {
    state.mobileDrawerOpen = false;
    notify();
  }

  function toggleUserMenu() {
    state.userMenuOpen = !state.userMenuOpen;
    notify();
    // Navbar is rendered from state; refresh it immediately so the menu appears.
    if (window.Router && typeof Router.refresh === 'function') Router.refresh();
  }

  function closeUserMenu(refreshView = true) {
    if (!state.userMenuOpen) return;
    state.userMenuOpen = false;
    notify();
    if (refreshView && window.Router && typeof Router.refresh === 'function') Router.refresh();
  }

  return {
    get state() { return state; },
    subscribe,
    toggleTheme,
    setTheme,
    toggleFavorite,
    isFavorite,
    unlockReply,
    isReplyUnlocked,
    unlockPaid,
    isPaidUnlocked,
    getPost,
    getAllPosts,
    saveDraft,
    createPost,
    getReplies,
    togglePostLike,
    isPostLiked,
    toggleReplyLike,
    isReplyLiked,
    addReply,
    createReport,
    markNotificationRead,
    markAllNotificationsRead,
    getUnreadCount,
    removeDevice,
    updateReport,
    addReportHistory,
    adjustCoins,
    addOAuthClient,
    togglePlugin,
    updateStorageConfig,
    testStorageConnection,
    setPublishType,
    setEditorTab,
    toggleMobileDrawer,
    closeMobileDrawer,
    toggleUserMenu,
    closeUserMenu
  };
})();

// ============================================
// Toast System
// ============================================
window.Toast = (function() {
  let container;
  let idCounter = 0;

  function init() {
    container = document.getElementById('toast-container');
  }

  function show(message, type = 'info', duration = 3000) {
    if (!container) init();
    if (!container) return;

    const id = ++idCounter;
    const icons = {
      success: 'check-circle',
      warning: 'alert-triangle',
      danger: 'x-circle',
      info: 'info'
    };

    const toast = document.createElement('div');
    toast.className = `toast toast-${type}`;
    toast.dataset.id = id;
    const iconSvg = (n, s) => `<svg class="icon icon-${n}" width="${s}" height="${s}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false">${window.Icons[n] || ''}</svg>`;
    toast.innerHTML = `
      <div class="toast-icon">${iconSvg(icons[type] || 'info', 18)}</div>
      <div class="toast-message">${message}</div>
      <button class="toast-close" onclick="Toast.dismiss(${id})">${iconSvg('x', 14)}</button>
    `;
    container.appendChild(toast);

    if (duration > 0) {
      setTimeout(() => dismiss(id), duration);
    }

    return id;
  }

  function dismiss(id) {
    const toast = container.querySelector(`[data-id="${id}"]`);
    if (!toast) return;
    toast.classList.add('toast-out');
    setTimeout(() => toast.remove(), 200);
  }

  return { show, dismiss, init };
})();

// ============================================
// Modal System
// ============================================
window.Modal = (function() {
  let container;

  function init() {
    container = document.getElementById('modal-container');
  }

  function open({ title, content, confirmText = '确认', cancelText = '取消', variant = 'primary', onConfirm, onCancel, footer = true }) {
    if (!container) init();
    if (!container) return;

    const modal = document.createElement('div');
    modal.className = 'modal-overlay';
    const iconSvg = (n, s) => `<svg class="icon icon-${n}" width="${s}" height="${s}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false">${window.Icons[n] || ''}</svg>`;
    modal.innerHTML = `
      <div class="modal" role="dialog" aria-modal="true" aria-label="${title || '对话框'}">
        <div class="modal-header">
          <div class="modal-title">${title || ''}</div>
          <button class="modal-close" data-action="close" aria-label="关闭">${iconSvg('x', 18)}</button>
        </div>
        <div class="modal-body">${content || ''}</div>
        ${footer ? `
        <div class="modal-footer">
          <button class="btn btn-secondary" data-action="cancel">${cancelText}</button>
          <button class="btn btn-${variant}" data-action="confirm">${confirmText}</button>
        </div>
        ` : ''}
      </div>
    `;
    container.appendChild(modal);

    function close() {
      modal.remove();
      if (onCancel) onCancel();
    }

    modal.querySelector('[data-action="close"]').addEventListener('click', close);
    modal.querySelector('[data-action="cancel"]')?.addEventListener('click', close);
    modal.querySelector('[data-action="confirm"]')?.addEventListener('click', () => {
      if (onConfirm) {
        const result = onConfirm();
        if (result !== false) modal.remove();
      } else {
        modal.remove();
      }
    });
    modal.addEventListener('click', (e) => {
      if (e.target === modal) close();
    });

    return { close: () => modal.remove() };
  }

  function closeAll() {
    if (container) container.innerHTML = '';
  }

  return { open, closeAll, init };
})();
