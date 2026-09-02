/* ══════════════════════════════════════════════
   bblbb 管理后台 · 公共框架
   布局动效 / 弹窗 / Toast / 事件分发 / 跨页通用操作
   各页面定义 window.PAGE 后由 boot() 启动
   ══════════════════════════════════════════════ */
(function () {
'use strict';

var B = window.B;
var D = B.D, state = B.state;
var $ = B.$, $$ = B.$$;
var svg = B.svg, esc = B.esc, grad = B.grad;
var USER_STATUS = B.USER_STATUS, PROJ_STATUS = B.PROJ_STATUS, REV_STATUS = B.REV_STATUS,
      ANN_STATUS = B.ANN_STATUS, ORD_STATUS = B.ORD_STATUS;

/* ── Toast ── */
function toast(msg, type) {
  var el = document.createElement('div');
  el.className = 'toast ' + (type || 'success');
  var ico = type === 'danger' ? 'alert' : type === 'info' ? 'zap' : 'check';
  el.innerHTML = '<span class="t-ico">' + svg(ico, 15) + '</span><span>' + msg + '</span>';
  $('#toasts').appendChild(el);
  setTimeout(function () {
    el.classList.add('out');
    setTimeout(function () { el.remove(); }, 320);
  }, 3400);
}

/* ── Modal ── */
function openModal(title, body, foot) {
  $('#modal-root').innerHTML =
    '<div class="modal-overlay">' +
    '<div class="modal" role="dialog" aria-modal="true">' +
    '<div class="modal-head"><h3>' + title + '</h3><button class="btn-icon" data-action="modal-close" aria-label="关闭">' + svg('x') + '</button></div>' +
    '<div class="modal-body">' + body + '</div>' +
    (foot ? '<div class="modal-foot">' + foot + '</div>' : '') +
    '</div></div>';
  var f = $('#modal-root').querySelector('input:not([type=radio]):not([type=checkbox]), textarea, select, .btn');
  if (f) { try { f.focus(); } catch (e) {} }
}
function closeModal() { $('#modal-root').innerHTML = ''; }
function confirmBox(msgHtml, yesLabel, yesCls, onYes) {
  openModal('确认操作',
    '<div class="confirm-ico">' + svg('alert', 20) + '</div><div class="confirm-txt">' + msgHtml + '</div>',
    '<button class="btn btn-ghost" data-action="modal-close">取消</button>' +
    '<button class="btn ' + (yesCls || 'btn-danger') + '" data-action="modal-confirm">' + yesLabel + '</button>');
  state._onYes = onYes;
}
function emptyHtml(title, desc, btnHtml) {
  return '<div class="empty">' + svg('box', 30) + '<div class="t">' + title + '</div><div class="d">' + desc + '</div>' + (btnHtml || '') + '</div>';
}
function downloadCSV(name, rows) {
  var csv = rows.map(function (r) {
    return r.map(function (c) { return '"' + String(c == null ? '' : c).replace(/"/g, '""') + '"'; }).join(',');
  }).join('\n');
  var blob = new Blob(['\uFEFF' + csv], { type: 'text/csv;charset=utf-8' });
  var a = document.createElement('a');
  a.href = URL.createObjectURL(blob);
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(function () { URL.revokeObjectURL(a.href); }, 800);
}

/* ── 列表页通用组件 ── */
function seg(page, key, opts, val) {
  return '<div class="seg">' + opts.map(function (o) {
    return '<button data-seg="' + page + '" data-key="' + key + '" data-val="' + o[0] + '" class="' + (val === o[0] ? 'on' : '') + '">' + o[1] + '</button>';
  }).join('') + '</div>';
}
function searchBox(page, ph) {
  return '<div class="search-box grow">' + svg('search', 15) +
    '<input class="input" type="search" placeholder="' + ph + '" value="' + esc(state.f[page].q || '') + '" data-search="' + page + '"></div>';
}
function batchbar(kind) {
  var n = Object.keys(state.sel[kind]).filter(function (k) { return state.sel[kind][k]; }).length;
  if (!n) return '';
  if (kind === 'users') {
    return '<div class="batchbar"><span class="cnt">已选 <b>' + n + '</b> 位创作者</span><span class="btns">' +
      '<button class="btn btn-ghost btn-sm" data-action="sel-clear" data-kind="users">取消选择</button>' +
      '<button class="btn btn-ghost btn-sm" data-action="batch-user-export">' + svg('download', 12) + ' 导出 CSV</button>' +
      '<button class="btn btn-ghost btn-sm" data-action="batch-user-freeze">' + svg('lock', 12) + ' 冻结所选</button>' +
      '<button class="btn btn-danger btn-sm" data-action="batch-user-delete">' + svg('trash', 12) + ' 删除所选</button>' +
      '</span></div>';
  }
  return '<div class="batchbar"><span class="cnt">已选 <b>' + n + '</b> 个项目</span><span class="btns">' +
    '<button class="btn btn-ghost btn-sm" data-action="sel-clear" data-kind="projects">取消选择</button>' +
    '<button class="btn btn-danger btn-sm" data-action="batch-project-delete">' + svg('trash', 12) + ' 删除所选</button>' +
    '</span></div>';
}
function syncAllCb() {
  var all = $('#listwrap input[data-all]');
  if (!all) return;
  var ids = $$('#listwrap input[data-id]').map(function (i) { return i.dataset.id; });
  var kind = all.dataset.sel;
  var sel = ids.filter(function (id) { return state.sel[kind][id]; }).length;
  all.checked = ids.length > 0 && sel === ids.length;
}
function findUser(id) { return D.users.filter(function (u) { return u.id === id; })[0]; }
function findProject(id) { return D.projects.filter(function (p) { return p.id === id; })[0]; }
function findReview(id) { return D.reviews.filter(function (r) { return r.id === id; })[0]; }
function findAnn(id) { return D.anns.filter(function (a) { return a.id === id; })[0]; }
function findOrder(id) { return D.orders.filter(function (o) { return o.id === id; })[0]; }
/* 取现有 ID 最大序号 + 1,避免删除记录后新增撞号 */
function nextId(prefix, list, pad) {
  pad = pad || 3;
  var max = 0;
  list.forEach(function (x) {
    var n = parseInt(String(x.id).replace(/[^0-9]/g, ''), 10);
    if (!isNaN(n) && n > max) max = n;
  });
  var s = String(max + 1);
  while (s.length < pad) s = '0' + s;
  return prefix + '-' + s;
}

/* ── 渲染控制 ── */
function rerenderFull() {
  if (!window.PAGE) return;
  updateBadges();
  $('#page-root').innerHTML = PAGE.html();
  if (PAGE.after) PAGE.after();
}
function renderList() {
  if (!window.PAGE || !PAGE.list) { rerenderFull(); return; }
  var wrap = $('#listwrap');
  if (!wrap) { rerenderFull(); return; }
  wrap.innerHTML = PAGE.list();
  if (PAGE.afterList) PAGE.afterList(); else syncAllCb();
}

/* ── 背景粒子 ── */
var REDUCED = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
function initParticles() {
  if (REDUCED) return;
  var canvas = document.getElementById('particles');
  if (!canvas) return;
  var ctx = canvas.getContext('2d');
  var W, H, pts = [];
  function rs() { W = canvas.width = window.innerWidth; H = canvas.height = window.innerHeight; }
  rs();
  window.addEventListener('resize', rs);
  var N = Math.max(24, Math.min(46, Math.floor(window.innerWidth / 34)));
  for (var i = 0; i < N; i++) {
    pts.push({
      x: Math.random() * W, y: Math.random() * H,
      vx: (Math.random() - 0.5) * 0.3, vy: (Math.random() - 0.5) * 0.3,
      r: Math.random() * 1.3 + 0.4,
      c: Math.random() > 0.5 ? '124,58,237' : '0,229,160'
    });
  }
  (function loop() {
    ctx.clearRect(0, 0, W, H);
    var i, j;
    for (i = 0; i < N; i++) {
      var p = pts[i];
      p.x += p.vx; p.y += p.vy;
      if (p.x < -10) p.x = W + 10;
      if (p.x > W + 10) p.x = -10;
      if (p.y < -10) p.y = H + 10;
      if (p.y > H + 10) p.y = -10;
      ctx.beginPath();
      ctx.arc(p.x, p.y, p.r, 0, 6.283);
      ctx.fillStyle = 'rgba(' + p.c + ',0.22)';
      ctx.fill();
    }
    for (i = 0; i < N; i++) {
      for (j = i + 1; j < N; j++) {
        var a = pts[i], b = pts[j];
        var dx = a.x - b.x, dy = a.y - b.y;
        var d2 = dx * dx + dy * dy;
        if (d2 < 16900) {
          ctx.beginPath();
          ctx.moveTo(a.x, a.y);
          ctx.lineTo(b.x, b.y);
          ctx.strokeStyle = 'rgba(124,58,237,' + (0.07 * (1 - d2 / 16900)).toFixed(3) + ')';
          ctx.lineWidth = 0.5;
          ctx.stroke();
        }
      }
    }
    requestAnimationFrame(loop);
  })();
}

/* ── 共享弹窗：邀请创作者 ── */
function modalInvite() {
  openModal('邀请创作者',
    '<div class="field"><label>姓名</label><input class="input" id="inv-name" placeholder="例如：林晚"></div>' +
    '<div class="field"><label>邮箱</label><input class="input" id="inv-email" type="email" placeholder="name@example.com"></div>' +
    '<div class="field"><label>初始积分</label><input class="input" id="inv-pts" type="number" value="200" min="0" style="width:140px"></div>' +
    '<div class="field" style="margin:0"><div class="hint">邀请邮件将发送到对方邮箱；账号激活后初始状态为「待验证」。</div></div>',
    '<button class="btn btn-ghost" data-action="modal-close">取消</button>' +
    '<button class="btn btn-primary" data-action="invite-save">' + svg('send', 13) + ' 发送邀请</button>');
}

/* ── 共享弹窗：公告新建/编辑 ── */
function modalAnn(a) {
  var isEdit = !!a;
  state._annId = isEdit ? a.id : null;
  openModal(isEdit ? '编辑公告 · ' + esc(a.title) : '新建公告',
    '<div class="field"><label>标题 <span style="color:var(--red)">*</span></label><input class="input" id="an-title" value="' + esc(a ? a.title : '') + '" placeholder="一句话说明公告内容"></div>' +
    '<div class="field"><label>类型</label><select class="select" id="an-type">' +
    '<option' + (a && a.type === '系统' ? ' selected' : '') + '>系统</option>' +
    '<option' + (a && a.type === '活动' ? ' selected' : '') + '>活动</option>' +
    '<option' + (a && a.type === '更新' ? ' selected' : '') + '>更新</option>' +
    '</select></div>' +
    '<div class="field"><label>正文</label><textarea class="textarea" id="an-body" style="min-height:110px" placeholder="公告正文，支持换行">' + (a && a.body ? a.body : '') + '</textarea></div>' +
    '<div class="field" style="margin:0"><label>发布方式</label><div class="radio-row">' +
    '<label><input type="radio" name="an-mode" value="now" checked> 立即发布</label>' +
    '<label><input type="radio" name="an-mode" value="sched" data-ann-sched> 定时发布</label></div>' +
    '<div id="an-dt-wrap" style="display:none;margin-top:10px"><input class="input" type="datetime-local" id="an-dt" value="2026-08-01T09:00" style="width:240px"></div></div>',
    '<button class="btn btn-ghost" data-action="modal-close">取消</button>' +
    '<button class="btn btn-ghost" data-action="an-save" data-mode="draft">存为草稿</button>' +
    '<button class="btn btn-primary" data-action="an-save" data-mode="publish">' + svg('send', 13) + ' 发布</button>');
}

/* ── 共享弹窗：审核驳回 / 查看 ── */
function modalReviewReject(r) {
  openModal('驳回作品 · ' + esc(r.title),
    '<div style="display:flex;gap:12px;align-items:center;margin-bottom:16px">' +
    '<span class="cover" style="' + grad(r.grad) + '"><span>' + esc(r.title[0]) + '</span></span>' +
    '<div><div style="font-size:13px;font-weight:600">' + esc(r.title) + '</div><div class="cell-sub">' + esc(r.author) + ' · ' + r.type + ' · ' + r.time + '</div></div></div>' +
    '<div class="field"><label>驳回原因 <span style="color:var(--red)">*</span></label>' +
    '<textarea class="textarea" id="rj-reason" placeholder="将随通知发送给作者，请写清楚需要修改的地方"></textarea>' +
    '<div class="reason-chips">' +
    '<button type="button" data-action="rj-chip" data-cv="素材含未授权水印">素材含水印</button>' +
    '<button type="button" data-action="rj-chip" data-cv="违反社区内容规范">违反社区规范</button>' +
    '<button type="button" data-action="rj-chip" data-cv="内容不完整，缺少关键页面">内容不完整</button>' +
    '<button type="button" data-action="rj-chip" data-cv="清晰度不足，建议 2x 导出">清晰度不足</button>' +
    '</div></div>',
    '<button class="btn btn-ghost" data-action="modal-close">取消</button>' +
    '<button class="btn btn-danger" data-action="rj-save" data-id="' + r.id + '">' + svg('x', 13) + ' 确认驳回</button>');
}
function modalReviewView(r) {
  openModal('作品详情',
    '<div class="detail-cover" style="' + grad(r.grad) + '">' + esc(r.title[0]) + '</div>' +
    '<div style="font-size:16px;font-weight:700;margin-bottom:4px">' + esc(r.title) + '</div>' +
    '<div class="cell-sub mono" style="margin-bottom:14px">' + r.id + ' · ' + r.type + '</div>' +
    '<div class="info-row"><span class="k">作者</span><span class="v">' + esc(r.author) + '</span></div>' +
    '<div class="info-row"><span class="k">提交时间</span><span class="v">' + r.time + '</span></div>' +
    '<div class="info-row"><span class="k">状态</span><span class="v">' + B.tag(REV_STATUS, r.status) + (r.featured ? ' <span class="tag tag-purple">已推荐</span>' : '') + '</span></div>' +
    (r.reason ? '<div class="info-row"><span class="k">驳回原因</span><span class="v" style="font-weight:400;text-align:right;max-width:280px;white-space:normal">' + esc(r.reason) + '</span></div>' : ''),
    (r.status === 'rejected'
      ? '<button class="btn btn-primary" data-action="review-reopen" data-id="' + r.id + '">' + svg('refresh', 13) + ' 重新审核</button>'
      : '<button class="btn btn-ghost" data-action="modal-close">关闭</button>'));
}

/* ══ 跨页通用操作 ══ */
var COMMON = {
  bell: function () { $('#bell-menu').classList.toggle('open'); },
  'bell-read': function () {
    $('#bell-dot').style.display = 'none';
    $('#bell-menu').classList.remove('open');
    B.storeSet('notifs-read', '1');
    toast('通知已全部标记为已读', 'info');
  },
  'modal-close': function () { closeModal(); },
  'modal-confirm': function () { var fn = state._onYes; closeModal(); if (fn) fn(); },
  logout: function () {
    confirmBox('确定要退出当前管理员会话吗？', '退出登录', 'btn-danger', function () {
      toast('原型演示：未真正退出登录', 'info');
    });
  },
  'clear-filters': function () {
    state.f = {
      users: { q: '', status: 'all', plan: 'all' }, projects: { q: '', status: 'all' },
      reviews: { status: 'pending' }, orders: { q: '', status: 'all' },
      logs: { q: '', cat: 'all' }, anns: { status: 'all' }
    };
    rerenderFull();
  },
  'sel-clear': function (id, el) {
    state.sel[el.dataset.kind] = {};
    renderList();
  },

  /* ── 审核（总览与审核队列共用） ── */
  'review-approve': function (id) {
    var r = findReview(id);
    if (!r) return;
    r.status = 'approved';
    delete state.sel.reviews[id];
    B.saveAfter('审核通过', '「' + r.title + '」', '审核');
    toast('已通过，「' + r.title + '」已发布');
    rerenderFull();
  },
  'review-reject': function (id) { var r = findReview(id); if (r) modalReviewReject(r); },
  'rj-chip': function (id, el) { var ta = $('#rj-reason'); if (ta) ta.value = el.dataset.cv; },
  'rj-save': function (id) {
    var r = findReview(id), ta = $('#rj-reason');
    var reason = ta ? ta.value.trim() : '';
    if (!reason) { toast('请填写驳回原因，会随通知发给作者', 'danger'); return; }
    r.status = 'rejected';
    r.reason = reason;
    delete state.sel.reviews[id];
    B.saveAfter('审核驳回', '「' + r.title + '」', '审核');
    closeModal();
    toast('已驳回并通知 ' + r.author);
    rerenderFull();
  },
  'review-recommend': function (id) {
    var r = findReview(id);
    if (!r) return;
    r.status = 'approved';
    r.featured = true;
    delete state.sel.reviews[id];
    B.saveAfter('推荐作品', '「' + r.title + '」', '审核');
    toast('已推荐到发现页，「' + r.title + '」同时通过审核');
    rerenderFull();
  },
  'review-view': function (id) { var r = findReview(id); if (r) modalReviewView(r); },
  'review-reopen': function (id) {
    var r = findReview(id);
    if (!r) return;
    r.status = 'pending';
    r.time = '刚刚';
    delete r.reason;
    B.saveAfter('重新审核', '「' + r.title + '」', '审核');
    closeModal();
    toast('已移回待审核队列');
    rerenderFull();
  },
  'batch-review-approve': function () {
    var ids = Object.keys(state.sel.reviews).filter(function (k) { return state.sel.reviews[k]; });
    ids.forEach(function (id) { var r = findReview(id); if (r && r.status === 'pending') r.status = 'approved'; });
    B.saveAfter('批量审核通过', ids.length + ' 条作品', '审核');
    state.sel.reviews = {};
    toast('已批量通过 ' + ids.length + ' 条作品');
    rerenderFull();
  },

  /* ── 公告（总览与公告中心共用） ── */
  'ann-new': function () { modalAnn(null); },
  'ann-edit': function (id) { var a = findAnn(id); if (a) modalAnn(a); },
  'an-save': function (id, el) {
    var titleEl = $('#an-title');
    var title = titleEl ? titleEl.value : '';
    if (!title || !title.trim()) { toast('请填写公告标题', 'danger'); return; }
    var type = ($('#an-type') || {}).value;
    var body = ($('#an-body') || {}).value;
    var mode = el.dataset.mode;
    var sched = document.querySelector('input[name="an-mode"][value="sched"]');
    var dt = ($('#an-dt') || {}).value;
    var now = new Date();
    var today = now.getFullYear() + '-' + B.pad2(now.getMonth() + 1) + '-' + B.pad2(now.getDate()) + ' ' + B.pad2(now.getHours()) + ':' + B.pad2(now.getMinutes());
    if (state._annId) {
      var a = findAnn(state._annId);
      a.title = title.trim(); a.type = type; a.body = body;
      if (mode === 'draft') { a.status = 'draft'; a.time = '未发布'; a.reads = '—'; }
      else if (sched && sched.checked) { a.status = 'scheduled'; a.time = dt ? dt.replace('T', ' ') : '待定时'; }
      else { a.status = 'published'; a.time = today; }
      B.saveAfter('编辑公告', '「' + title.trim() + '」', '公告');
      toast(mode === 'draft' ? '草稿已保存' : sched && sched.checked ? '已重新定时发布（' + (dt || '') + '）' : '公告已更新并发布');
    } else {
      D.anns.unshift({
        id: nextId('A', D.anns, 3), title: title.trim(), type: type, body: body,
        status: mode === 'draft' ? 'draft' : (sched && sched.checked ? 'scheduled' : 'published'),
        time: mode === 'draft' ? '未发布' : (sched && sched.checked ? (dt ? dt.replace('T', ' ') : '待定时') : today),
        reads: '—'
      });
      B.saveAfter(mode === 'draft' ? '新建公告草稿' : '发布公告', '「' + title.trim() + '」', '公告');
      toast(mode === 'draft' ? '草稿已保存' : (sched && sched.checked ? '已定时发布（' + (dt || '') + '）' : '公告已发布，所有用户将收到通知'));
    }
    B.saveData();
    closeModal();
    rerenderFull();
  },
  'ann-publish': function (id) {
    var a = findAnn(id);
    if (!a) return;
    a.status = 'published';
    a.time = '刚刚';
    B.saveAfter('发布公告', '「' + a.title + '」', '公告');
    toast('公告已发布，所有用户将收到通知');
    rerenderFull();
  },
  'ann-withdraw': function (id) {
    var a = findAnn(id);
    if (!a) return;
    confirmBox('撤回公告 <b>「' + esc(a.title) + '」</b>？撤回后前台将不可见，阅读量保留。', '撤回公告', 'btn-danger', function () {
      a.status = 'draft';
      a.time = '未发布';
      B.saveAfter('撤回公告', '「' + a.title + '」', '公告');
      toast('公告已撤回，前台不可见', 'info');
      rerenderFull();
    });
  },
  'ann-delete': function (id) {
    var a = findAnn(id);
    if (!a) return;
    confirmBox('删除公告 <b>「' + esc(a.title) + '」</b>？删除后不可恢复。', '删除公告', 'btn-danger', function () {
      D.anns = D.anns.filter(function (x) { return x.id !== id; });
      B.saveAfter('删除公告', '「' + a.title + '」', '公告');
      B.saveData();
      toast('公告已删除', 'danger');
      rerenderFull();
    });
  },

  /* ── 邀请创作者（总览与创作者页共用） ── */
  'user-invite': function () { modalInvite(); },
  'invite-save': function () {
    var name = ($('#inv-name') || {}).value, email = ($('#inv-email') || {}).value,
      pts = (($('#inv-pts') || {}).value || 200);
    if (!name || !email) { toast('请填写姓名和邮箱再发送', 'danger'); return; }
    var today = B.fmtToday().slice(0, 10);
    D.users.unshift({
      id: nextId('U', D.users, 4), name: name, handle: name.toLowerCase().replace(/\s+/g, ''),
      email: email, hue: Math.floor(Math.random() * 360), points: parseInt(pts, 10) || 0,
      status: 'pending', plan: 'Free', joined: today, active: '刚刚', projects: 0
    });
    B.saveAfter('邀请创作者', email, '用户');
    B.saveData();
    closeModal();
    toast('邀请邮件已发送至 ' + email);
    rerenderFull();
  }
};

/* ── 事件分发 ── */
function findAction(name) {
  if (window.PAGE && PAGE.actions && PAGE.actions[name]) return PAGE.actions[name];
  return COMMON[name];
}
document.addEventListener('click', function (e) {
  var menu = $('#bell-menu');
  if (menu && menu.classList.contains('open') && !e.target.closest('.bell-wrap')) {
    menu.classList.remove('open');
  }
  var t = e.target.closest('[data-action]');
  if (t) {
    e.preventDefault();
    var fn = findAction(t.dataset.action);
    if (fn) fn(t.dataset.id, t);
    return;
  }
  var s = e.target.closest('[data-seg]');
  if (s) {
    e.preventDefault();
    var page = s.dataset.seg, key = s.dataset.key, val = s.dataset.val;
    $$('#page-root [data-seg="' + page + '"]').forEach(function (b) { b.classList.toggle('on', b === s); });
    if (key === 'tab') {
      if (window.PAGE && PAGE.beforeTab) PAGE.beforeTab();
      state.settingsTab = val; rerenderFull(); return;
    }
    if (state.f[page]) state.f[page][key] = val;
    if (window.PAGE && PAGE.fullSeg) { rerenderFull(); return; }
    renderList();
    return;
  }
  if (e.target.classList && e.target.classList.contains('modal-overlay')) {
    closeModal();
  }
});
document.addEventListener('input', function (e) {
  var t = e.target;
  if (t.dataset && t.dataset.search) {
    state.f[t.dataset.search].q = t.value;
    renderList();
  }
});
document.addEventListener('change', function (e) {
  var t = e.target;
  if (!t.dataset) return;
  if (t.dataset.plan) { state.f.users.plan = t.value; renderList(); return; }
  if (t.dataset.sel) {
    var kind = t.dataset.sel;
    if (t.dataset.all) {
      $$('#listwrap input[data-id]').forEach(function (i) {
        if (t.checked) state.sel[kind][i.dataset.id] = true; else delete state.sel[kind][i.dataset.id];
      });
    } else if (t.dataset.id) {
      if (t.checked) state.sel[kind][t.dataset.id] = true; else delete state.sel[kind][t.dataset.id];
    }
    renderList();
    return;
  }
  if (t.dataset.annSched) {
    var wrap = $('#an-dt-wrap');
    if (wrap) wrap.style.display = t.checked ? 'block' : 'none';
  }
});
document.addEventListener('keydown', function (e) {
  if (e.key === 'Escape') {
    var menu = $('#bell-menu');
    if (menu && menu.classList.contains('open')) menu.classList.remove('open');
    else if ($('#modal-root').innerHTML) closeModal();
  }
});

/* ── 启动 ── */
function markActive() {
  var f = location.pathname.split('/').pop() || 'index.html';
  $$('.nav-item').forEach(function (a) {
    a.classList.toggle('on', a.getAttribute('href') === f);
  });
}
function updateBadges() {
  var bu = $('#badge-users'), bp = $('#badge-projects'), br = $('#badge-reviews');
  if (bu) bu.textContent = D.users.length;
  if (bp) bp.textContent = D.projects.length;
  if (br) br.textContent = B.pendingCount();
}
function boot() {
  $('#date-chip').textContent = B.fmtToday();
  markActive();
  updateBadges();
  if (B.storeGet('notifs-read')) $('#bell-dot').style.display = 'none';
  initParticles();
  if (!window.PAGE) return;
  $('#page-title').textContent = PAGE.title;
  $('#page-sub').textContent = typeof PAGE.sub === 'function' ? PAGE.sub() : (PAGE.sub || '');
  $('#page-root').innerHTML = PAGE.html();
  if (PAGE.after) PAGE.after();
}
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', boot);
} else {
  boot();
}
/* 暴露给页面脚本 */
window.toast = toast;
window.downloadCSV = downloadCSV;
window.openModal = openModal;
window.closeModal = closeModal;
window.confirmBox = confirmBox;
window.rerenderFull = rerenderFull;
window.renderList = renderList;
window.batchbar = batchbar;
window.seg = seg;
window.searchBox = searchBox;
window.emptyHtml = emptyHtml;
window.findUser = findUser;
window.findProject = findProject;
window.findReview = findReview;
window.findAnn = findAnn;
window.findOrder = findOrder;
window.nextId = nextId;
window.syncAllCb = syncAllCb;
})();
