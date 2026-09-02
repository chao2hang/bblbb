/* BBLBB 页面加载器
 * 页面主体拆分在 pages/*.html（与 index.html 共用 assets/*.css），
 * 启动时并行拉取并注入对应 <section class="page">，完成后动态加载唯一路由引擎 prototype-app.js。
 * 其余路由（消息/搜索/商城/后台等）由引擎在运行时动态挂载，不依赖静态骨架。 */
(function () {
  'use strict';

  var FRAGMENTS = [
    ['home', 'pages/home.html'],
    ['discover', 'pages/discover.html'],
    ['design', 'pages/design.html']
  ];

  function inject(id, html) {
    var el = document.getElementById('page-' + id);
    if (!el) return;
    /* pages/*.html 是完整文档：只抽取 section.page 的内容注入宿主，避免嵌套 main/section */
    var parsed = new DOMParser().parseFromString(html, 'text/html');
    var section = parsed.querySelector('section.page');
    el.innerHTML = section ? section.innerHTML : html;
  }

  function loadFragment(entry) {
    return fetch(entry[1])
      .then(function (res) {
        if (!res.ok) throw new Error('HTTP ' + res.status);
        return res.text();
      })
      .then(function (html) { inject(entry[0], html); })
      .catch(function () {
        inject(entry[0], '<div class="app-empty"><b>页面加载失败</b><span>请刷新重试（需要通过 http 访问，而非 file:// 打开）。</span></div>');
      });
  }

  Promise.all(FRAGMENTS.map(loadFragment)).then(function () {
    var script = document.createElement('script');
    script.src = 'assets/prototype-app.js';
    script.onload = function () { window.__bblbbCompletionReady = true; };
    script.onerror = function () { document.title = 'BBLBB 加载失败'; };
    document.body.appendChild(script);
  });
}());
