/* ══ 总览 ══ */
(function () {
'use strict';
var B = window.B;
var D = B.D, state = B.state;
var $ = B.$, $$ = B.$$;
var svg = B.svg, esc = B.esc, grad = B.grad, tag = B.tag;
var REV_STATUS = B.REV_STATUS;
var REDUCED = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

var FEED = [
  { t: '8 分钟前', c: 'green', txt: '林晚 更新了作品「像素农场小游戏」，新增 2 个房间' },
  { t: '26 分钟前', c: 'purple', txt: '新创作者 maker_042 完成注册，来自上海' },
  { t: '1 小时前', c: 'green', txt: '陈屿 的「城市呼吸」新增 128 次点赞' },
  { t: '2 小时前', c: 'amber', txt: '订单 O-1099 等待确认支付' },
  { t: '5 小时前', c: 'purple', txt: '江晚明 提交了 3 条作品待审核' },
  { t: '昨天 21:02', c: 'dim', txt: '系统完成每日备份（2.3 GB）' },
  { t: '昨天 18:40', c: 'green', txt: '公告「系统维护通知」阅读量突破 5,000' },
  { t: '2 天前', c: 'red', txt: '白鹿 的账号因违规被冻结' }
];
var CHANNELS = [
  ['直接访问', 46], ['社交平台', 28], ['搜索引擎', 16], ['创作者邀请', 10]
];

function kpi(label, num, delta, deltaCls, actTitle, href, cls) {
  return '<div class="card kpi ' + (cls || '') + '">' +
    '<div class="k-label">' + label + '</div>' +
    '<div class="k-num">' + num + '</div>' +
    '<div class="k-delta ' + (deltaCls || '') + '">' + delta + '</div>' +
    '<span class="k-act"><a class="btn-icon" title="' + actTitle + '" href="' + href + '">' + svg('arrow', 13) + '</a></span>' +
    '</div>';
}

/* ── 创作脉搏：24h 活动曲线 ── */
var PULSE_DATA = [12, 8, 6, 5, 4, 6, 10, 18, 26, 34, 41, 38, 36, 40, 44, 42, 39, 45, 52, 58, 61, 54, 38, 22];
function drawPulse() {
  var cv = document.getElementById('pulseCanvas');
  if (!cv) return;
  var dpr = window.devicePixelRatio || 1;
  var w = cv.clientWidth || 400, h = 96;
  cv.width = w * dpr;
  cv.height = h * dpr;
  var ctx = cv.getContext('2d');
  ctx.scale(dpr, dpr);
  var data = PULSE_DATA, max = 66, pad = 4;
  function pt(i, v) {
    return [pad + (w - pad * 2) * i / (data.length - 1), h - pad - (h - pad * 2) * v / max];
  }
  function draw(prog) {
    ctx.clearRect(0, 0, w, h);
    var n = Math.max(2, Math.floor(data.length * prog));
    var i, p;
    ctx.strokeStyle = 'rgba(255,255,255,0.05)';
    ctx.lineWidth = 1;
    for (var g = 1; g <= 3; g++) {
      ctx.beginPath(); ctx.moveTo(0, h * g / 4); ctx.lineTo(w, h * g / 4); ctx.stroke();
    }
    ctx.beginPath();
    for (i = 0; i < n; i++) {
      p = pt(i, data[i]);
      if (i === 0) ctx.moveTo(p[0], p[1]); else ctx.lineTo(p[0], p[1]);
    }
    ctx.lineTo(pt(n - 1, 0)[0], h);
    ctx.lineTo(pt(0, 0)[0], h);
    ctx.closePath();
    var fill = ctx.createLinearGradient(0, 0, 0, h);
    fill.addColorStop(0, 'rgba(124,58,237,0.28)');
    fill.addColorStop(1, 'rgba(124,58,237,0)');
    ctx.fillStyle = fill;
    ctx.fill();
    ctx.beginPath();
    for (i = 0; i < n; i++) {
      p = pt(i, data[i]);
      if (i === 0) ctx.moveTo(p[0], p[1]); else ctx.lineTo(p[0], p[1]);
    }
    var grd = ctx.createLinearGradient(0, 0, w, 0);
    grd.addColorStop(0, '#7C3AED');
    grd.addColorStop(1, '#00E5A0');
    ctx.strokeStyle = grd;
    ctx.lineWidth = 2;
    ctx.lineJoin = 'round';
    ctx.stroke();
    var lp = pt(n - 1, data[n - 1]);
    ctx.beginPath();
    ctx.arc(lp[0], lp[1], 3.5, 0, 6.283);
    ctx.fillStyle = '#00E5A0';
    ctx.fill();
  }
  if (REDUCED) { draw(1); return; }
  var t0 = performance.now();
  (function anim(t) {
    var pr = Math.min((t - t0) / 900, 1);
    draw(1 - Math.pow(1 - pr, 3));
    if (pr < 1) requestAnimationFrame(anim);
  })(t0);
}
var onlineTimer = null;
function startOnlineTicker() {
  if (onlineTimer || REDUCED) return;
  var v = 1284;
  onlineTimer = setInterval(function () {
    var el = document.getElementById('online-num');
    if (!el) { clearInterval(onlineTimer); onlineTimer = null; return; }
    v += Math.floor(Math.random() * 9) - 4;
    if (v < 1230) v = 1230;
    if (v > 1360) v = 1360;
    el.textContent = v.toLocaleString();
  }, 2600);
}

window.PAGE = {
  title: '总览',
  sub: function () {
    var pend = B.pendingCount();
    return '待处理审核 ' + pend + ' 条 · 最近 7 日整体运行平稳';
  },
  html: function () {
    var pend = D.reviews.filter(function (r) { return r.status === 'pending'; }).slice(0, 4);
    var mini = pend.map(function (r) {
      return '<div class="mini-review">' +
        '<div class="mini-cover" style="' + grad(r.grad) + '"><span>' + esc(r.title[0]) + '</span></div>' +
        '<div class="mi-body"><div class="mi-title">' + esc(r.title) + '</div><div class="mi-sub">' + esc(r.author) + ' · ' + r.type + ' · ' + r.time + '</div></div>' +
        '<div class="mi-acts">' +
        '<button class="btn btn-green btn-sm" data-action="review-approve" data-id="' + r.id + '">' + svg('check', 12) + ' 通过</button>' +
        '<button class="btn btn-danger btn-sm" data-action="review-reject" data-id="' + r.id + '">驳回</button>' +
        '</div></div>';
    }).join('');
    if (!pend.length) mini = '<div class="empty" style="padding:36px 20px">' + svg('check', 26) + '<div class="t">队列已清空</div><div class="d">所有作品都已处理完毕</div></div>';

    var feed = FEED.map(function (f) {
      return '<li><span class="dot ' + f.c + '"></span><div>' + f.txt + '<time>' + f.t + '</time></div></li>';
    }).join('');

    var chans = CHANNELS.map(function (c) {
      return '<div class="chan-row"><span class="nm">' + c[0] + '</span><span class="chan-bar"><i data-w="' + c[1] + '"></i></span><span class="pc">' + c[1] + '%</span></div>';
    }).join('');

    var pc = B.pendingCount();
    return '<div class="card pulse">' +
      '<div class="pulse-online">' +
      '<div class="live"><span class="live-dot"></span>此刻在线创作者</div>' +
      '<div class="num" id="online-num">1,284</div>' +
      '<div class="hint">较昨日同时段 <b style="color:var(--green)">+6.4%</b></div>' +
      '</div>' +
      '<div class="pulse-chart">' +
      '<div class="chart-cap"><span>近 24 小时创作活动（次/时）</span><b>峰值 61 · 19:00</b></div>' +
      '<canvas id="pulseCanvas"></canvas>' +
      '</div>' +
      '<div class="pulse-side">' +
      '<div class="row"><span>今日新建作品</span><b class="g">128</b></div>' +
      '<div class="row"><span>今日发布项目</span><b>34</b></div>' +
      '<div class="row"><span>今日注册创作者</span><b class="p">23</b></div>' +
      '<div class="row"><span>今日在线峰值</span><b>1,512</b></div>' +
      '</div></div>' +

      '<div class="kpi-row">' +
      kpi('创作者', '10,240', '本周 +128', '', '管理创作者', 'users.html') +
      kpi('进行中项目', '50,312', '本周 +342', '', '查看项目', 'projects.html') +
      kpi('待审核', String(pc), pc > 3 ? '2 条提交超过 1 小时' : '队列正常', pc > 3 ? 'warn' : '', '处理审核', 'reviews.html', pc > 3 ? 'warn' : '') +
      kpi('今日收入', '¥12,480', '+18.2% 较昨日', '', '查看订单', 'orders.html', 'green') +
      '</div>' +

      '<div class="dash-cols">' +
      '<div class="card">' +
      '<div class="card-head"><h3>审核队列</h3><span class="spacer"></span><a class="link-btn" href="reviews.html">查看全部 ' + pc + ' 条 →</a></div>' +
      mini +
      '</div>' +
      '<div class="card">' +
      '<div class="card-head"><h3>实时动态</h3><span class="spacer"></span><span class="cell-sub">自动刷新</span></div>' +
      '<ul class="feed">' + feed + '</ul>' +
      '</div></div>' +

      '<div class="dash-cols-2">' +
      '<div class="card">' +
      '<div class="card-head"><h3>流量来源</h3><span class="spacer"></span><span class="cell-sub">近 7 日</span></div>' +
      chans +
      '</div>' +
      '<div class="card">' +
      '<div class="card-head"><h3>快捷操作</h3></div>' +
      '<div class="quick-acts">' +
      '<button class="btn btn-primary" data-action="ann-new">' + svg('plus') + ' 新建公告</button>' +
      '<button class="btn btn-ghost" data-action="user-invite">' + svg('send', 13) + ' 邀请创作者</button>' +
      '<button class="btn btn-ghost" data-action="export-report">' + svg('download') + ' 导出周报</button>' +
      '</div>' +
      '<div class="foot-note" style="padding-top:14px;font-size:12px;color:var(--t3)">本周已处理审核 96 条 · 平均用时 14 分钟 · 申诉 1 条</div>' +
      '</div></div>';
  },
  after: function () {
    drawPulse();
    $$('#page-root .chan-bar i').forEach(function (bar) {
      setTimeout(function () { bar.style.width = bar.dataset.w + '%'; }, 60);
    });
    startOnlineTicker();
  },
  actions: {
    'export-report': function () {
      var rows = [
        ['指标', '本周', '上周', '变化'],
        ['新增创作者', '128', '96', '+33.3%'],
        ['新建项目', '342', '311', '+9.9%'],
        ['发布作品', '1,204', '1,088', '+10.7%'],
        ['处理审核', '96', '81', '+18.5%'],
        ['GMV（¥）', '14358', '12140', '+18.3%']
      ];
      downloadCSV('bblbb-weekly-report.csv', rows);
      toast('周报已生成并下载（CSV）');
    }
  }
};
})();
