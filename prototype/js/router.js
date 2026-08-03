// ============================================
// BBLBB Hash Router
// ============================================

window.Router = (function() {
  let currentPath = '/';
  let currentParams = {};

  function parseHash() {
    const hash = window.location.hash.slice(1) || '/';
    const [path, queryString] = hash.split('?');
    const params = {};
    
    if (queryString) {
      queryString.split('&').forEach(pair => {
        const [key, value] = pair.split('=');
        if (key) params[decodeURIComponent(key)] = decodeURIComponent(value || '');
      });
    }
    
    return { path: path || '/', params };
  }

  function matchRoute(path) {
    // Exact matches
    const routes = [
      { pattern: '/', handler: () => Pages.home() },
      { pattern: '/articles', handler: (params) => Pages.articlesList(params) },
      { pattern: '/boards', handler: () => Pages.boardsList() },
      { pattern: '/tags', handler: () => Pages.tagsList() },
      { pattern: '/publish', handler: (params) => Pages.publish(params) },
      { pattern: '/notifications', handler: (params) => Pages.notifications(params) },
      { pattern: '/favorites', handler: (params) => Pages.favorites(params) },
      { pattern: '/search', handler: (params) => Pages.search(params) },
      { pattern: '/settings', handler: (params) => Pages.settings(params) },
      { pattern: '/login', handler: () => Pages.login() },
      { pattern: '/register', handler: () => Pages.register() },
      { pattern: '/forgot-password', handler: () => Pages.forgotPassword() },
      { pattern: '/403', handler: () => Pages.forbidden() },
      { pattern: '/404', handler: () => Pages.notFound() },
      { pattern: '/429', handler: () => Pages.tooManyRequests() },
      // Admin
      { pattern: '/admin', handler: () => Pages.adminDashboard() },
      { pattern: '/admin/reports', handler: (params) => Pages.adminReports(params) },
      { pattern: '/admin/points', handler: () => Pages.adminPoints() },
      { pattern: '/admin/levels', handler: () => Pages.adminLevels() },
      { pattern: '/admin/themes', handler: () => Pages.adminThemes() },
      { pattern: '/admin/plugins', handler: () => Pages.adminPlugins() },
      { pattern: '/admin/oauth', handler: () => Pages.adminOAuth() },
      { pattern: '/admin/settings', handler: () => Pages.adminSettings() }
    ];

    for (const route of routes) {
      if (route.pattern === path) {
        return route.handler;
      }
    }

    // Dynamic routes
    // /boards/[slug]
    const boardMatch = path.match(/^\/boards\/([^/]+)$/);
    if (boardMatch) {
      return (params) => Pages.boardDetail(boardMatch[1], params);
    }

    // /tags/[name]
    const tagMatch = path.match(/^\/tags\/([^/]+)$/);
    if (tagMatch) {
      return (params) => Pages.tagDetail(decodeURIComponent(tagMatch[1]), params);
    }

    // /topics/[id]
    const topicMatch = path.match(/^\/topics\/(\d+)$/);
    if (topicMatch) {
      return (params) => Pages.topicDetail(topicMatch[1], params);
    }

    // /users/[name]
    const userMatch = path.match(/^\/users\/([^/]+)$/);
    if (userMatch) {
      return (params) => Pages.userProfile(decodeURIComponent(userMatch[1]), params);
    }

    // /admin/reports/[id]
    const reportMatch = path.match(/^\/admin\/reports\/([^/]+)$/);
    if (reportMatch) {
      return () => Pages.adminReportDetail(reportMatch[1]);
    }

    return null;
  }

  function render() {
    const { path, params } = parseHash();
    currentPath = path;
    currentParams = params;

    const handler = matchRoute(path);
    const app = document.getElementById('app');
    
    if (handler) {
      app.innerHTML = handler(params);
    } else {
      app.innerHTML = Pages.notFound();
    }

    // Re-render lucide icons
    if (window.lucide) {
      lucide.createIcons();
    }

    // Scroll to top
    window.scrollTo(0, 0);

    // Close user menu on route change
    if (Store.state.userMenuOpen) Store.closeUserMenu();
    if (Store.state.mobileDrawerOpen) Store.closeMobileDrawer();
  }

  function navigate(path) {
    if (path.startsWith('#')) {
      window.location.hash = path.slice(1);
    } else if (path.startsWith('/')) {
      window.location.hash = path;
    } else {
      window.location.hash = '/' + path;
    }
  }

  function refresh() {
    render();
  }

  function updateParams(newParams) {
    const { path, params } = parseHash();
    const merged = { ...params, ...newParams };
    const queryString = Object.entries(merged)
      .filter(([k, v]) => v !== '' && v !== undefined && v !== null)
      .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
      .join('&');
    window.location.hash = path + (queryString ? '?' + queryString : '');
  }

  function setPage(page) {
    updateParams({ page });
  }

  function setTab(tab) {
    updateParams({ tab, page: 1 });
  }

  // Specific tab setters (for different pages)
  function setBoardTab(tab) {
    updateParams({ tab, page: 1 });
  }

  function setUserTab(tab) {
    updateParams({ tab });
  }

  function setNotifTab(tab) {
    updateParams({ tab });
  }

  function setFavTab(tab) {
    updateParams({ tab });
  }

  function setSearchTab(tab) {
    updateParams({ tab, page: 1 });
  }

  function updateAdminParams(newParams) {
    updateParams(newParams);
  }

  function getPath() { return currentPath; }
  function getParams() { return currentParams; }

  function init() {
    window.addEventListener('hashchange', render);
    
    // Close user menu when clicking outside
    document.addEventListener('click', () => {
      if (Store.state.userMenuOpen) Store.closeUserMenu();
    });

    // Initial render
    if (!window.location.hash) {
      window.location.hash = '/';
    } else {
      render();
    }
  }

  return {
    init,
    navigate,
    refresh,
    updateParams,
    setPage,
    setTab,
    setBoardTab,
    setUserTab,
    setNotifTab,
    setFavTab,
    setSearchTab,
    updateAdminParams,
    getPath,
    getParams,
    render
  };
})();
