// ============================================================
// BBLBB Global Lazy Loader
// Route bundles + viewport resources, including dynamically added nodes.
// ============================================================
window.LazyLoader = (function () {
  'use strict';

  const bundlePromises = new Map();
  const observed = new WeakSet();
  let resourceObserver = null;
  let mutationObserver = null;

  const routeBundles = [
    {
      test: (path) => path === '/publish' || path.startsWith('/users/') ||
        ['/login', '/register', '/forgot-password', '/notifications', '/favorites', '/search', '/settings', '/shop', '/activity', '/me/closet'].includes(path),
      src: 'js/pages2.js',
      ready: () => typeof window.Pages?.publish === 'function'
    },
    {
      test: (path) => path === '/admin' || path.startsWith('/admin/'),
      src: 'js/pages3.js',
      ready: () => typeof window.Pages?.adminDashboard === 'function'
    }
  ];

  function loadScript(src, ready) {
    if (ready?.()) return Promise.resolve();
    if (bundlePromises.has(src)) return bundlePromises.get(src);

    const promise = new Promise((resolve, reject) => {
      const existing = document.querySelector(`script[data-lazy-src="${src}"]`);
      const script = existing || document.createElement('script');
      const done = () => ready?.()
        ? resolve()
        : reject(new Error(`懒加载模块未完成注册：${src}`));

      if (existing) {
        existing.addEventListener('load', done, { once: true });
        existing.addEventListener('error', () => reject(new Error(`懒加载模块失败：${src}`)), { once: true });
        return;
      }

      script.src = src;
      script.async = true;
      script.dataset.lazySrc = src;
      script.addEventListener('load', done, { once: true });
      script.addEventListener('error', () => reject(new Error(`懒加载模块失败：${src}`)), { once: true });
      document.head.appendChild(script);
    }).catch((error) => {
      bundlePromises.delete(src);
      throw error;
    });

    bundlePromises.set(src, promise);
    return promise;
  }

  function ensureRoute(path) {
    const bundle = routeBundles.find((item) => item.test(path));
    if (!bundle || bundle.ready()) return null;
    return loadScript(bundle.src, bundle.ready);
  }

  function markLoaded(element) {
    element.classList.remove('is-lazy-loading');
    element.classList.add('is-lazy-loaded');
  }

  function markFailed(element) {
    element.classList.remove('is-lazy-loading');
    element.classList.add('is-lazy-error');
  }

  function reveal(element) {
    if (!element || element.classList.contains('is-lazy-loaded')) return;
    resourceObserver?.unobserve(element);
    element.classList.add('is-lazy-loading');

    if (element.dataset.src) {
      element.addEventListener('load', () => markLoaded(element), { once: true });
      element.addEventListener('error', () => markFailed(element), { once: true });
      if (element.dataset.srcset) element.srcset = element.dataset.srcset;
      element.src = element.dataset.src;
      delete element.dataset.src;
      delete element.dataset.srcset;
      return;
    }

    if (element.dataset.bg) {
      const image = new Image();
      image.onload = () => {
        element.style.backgroundImage = `url("${element.dataset.bg.replace(/"/g, '%22')}")`;
        delete element.dataset.bg;
        markLoaded(element);
      };
      image.onerror = () => markFailed(element);
      image.src = element.dataset.bg;
      return;
    }

    markLoaded(element);
  }

  function prepare(element) {
    if (!(element instanceof Element) || observed.has(element)) return;
    observed.add(element);

    if (element.matches('img:not([data-eager])')) {
      if (!element.hasAttribute('loading')) element.loading = 'lazy';
      if (!element.hasAttribute('decoding')) element.decoding = 'async';
    }
    if (element.matches('iframe:not([data-eager])') && !element.hasAttribute('loading')) {
      element.loading = 'lazy';
    }

    if (!element.matches('[data-src], [data-bg]')) return;
    element.classList.add('is-lazy-pending');
    if (resourceObserver) resourceObserver.observe(element);
    else reveal(element);
  }

  function scan(root = document) {
    if (!root?.querySelectorAll) return;
    if (root instanceof Element) prepare(root);
    root.querySelectorAll('img, iframe, [data-src], [data-bg]').forEach(prepare);
  }

  function init() {
    if ('IntersectionObserver' in window) {
      resourceObserver = new IntersectionObserver((entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) reveal(entry.target);
        });
      }, { rootMargin: '240px 0px', threshold: 0.01 });
    }

    scan(document);
    if ('MutationObserver' in window) {
      mutationObserver = new MutationObserver((records) => {
        records.forEach((record) => record.addedNodes.forEach((node) => {
          if (node.nodeType === Node.ELEMENT_NODE) scan(node);
        }));
      });
      mutationObserver.observe(document.body, { childList: true, subtree: true });
    }
  }

  return { init, scan, reveal, ensureRoute, loadScript };
})();
