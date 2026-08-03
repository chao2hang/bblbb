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
        plugins: state.plugins
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
  }

  function closeUserMenu() {
    state.userMenuOpen = false;
    notify();
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
    markNotificationRead,
    markAllNotificationsRead,
    getUnreadCount,
    removeDevice,
    updateReport,
    addReportHistory,
    adjustCoins,
    addOAuthClient,
    togglePlugin,
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
    toast.innerHTML = `
      <div class="toast-icon"><i data-lucide="${icons[type] || 'info'}" size="18"></i></div>
      <div class="toast-message">${message}</div>
      <button class="toast-close" onclick="Toast.dismiss(${id})"><i data-lucide="x" size="14"></i></button>
    `;
    container.appendChild(toast);
    if (window.lucide) lucide.createIcons({ root: toast });

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
    modal.innerHTML = `
      <div class="modal">
        <div class="modal-header">
          <div class="modal-title">${title || ''}</div>
          <button class="modal-close" data-action="close"><i data-lucide="x" size="18"></i></button>
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
    if (window.lucide) lucide.createIcons({ root: modal });

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
