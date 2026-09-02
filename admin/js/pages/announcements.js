/* ══ 公告中心 ══ */
(function () {
'use strict';
var B = window.B;
var D = B.D, state = B.state;
var $ = B.$, $$ = B.$$;
var svg = B.svg, esc = B.esc;
var ANN_STATUS = B.ANN_STATUS;

function filteredAnns() {
  var st = state.f.anns.status;
  if (st === 'all') return D.anns;
  return D.anns.filter(function (a) { return a.status === st; });
}

window.PAGE = {
  title: '公告中心',
  fullSeg: true,
  sub: function () {
    return D.anns.length + ' 条公告 · ' + B.countBy(D.anns, 'status', 'published') + ' 条已发布';
  },
  html: function () {
    var list = filteredAnns();
    var items = list.map(function (a) {
      var typeIcon = a.type === '活动' ? 'star' : a.type === '更新' ? 'zap' : 'alert';
      var typeColor = a.type === '活动' ? 'var(--purple-light)' : a.type === '更新' ? 'var(--green)' : 'var(--amber)';
      var acts =
        '<button class="btn-icon" title="编辑" data-action="ann-edit" data-id="' + a.id + '">' + svg('pen') + '</button>';
      if (a.status === 'published') acts += '<button class="btn btn-ghost btn-sm" data-action="ann-withdraw" data-id="' + a.id + '">撤回</button>';
      else if (a.status === 'scheduled') acts += '<button class="btn btn-green btn-sm" data-action="ann-publish" data-id="' + a.id + '">' + svg('send', 12) + ' 立即发布</button>';
      else acts += '<button class="btn btn-primary btn-sm" data-action="ann-publish" data-id="' + a.id + '">' + svg('send', 12) + ' 发布</button>';
      acts += '<button class="btn-icon warn" title="删除" data-action="ann-delete" data-id="' + a.id + '">' + svg('trash') + '</button>';
      return '<div class="card ann-item">' +
      '<span class="a-type" style="background:rgba(255,255,255,0.04);color:' + typeColor + '">' + svg(typeIcon, 17) + '</span>' +
      '<div class="a-body"><div class="a-title">' + esc(a.title) + '</div>' +
      '<div class="a-sub">' + a.type + ' · ' + ANN_STATUS[a.status][0] + ' · ' + a.time + ' · 阅读 ' + a.reads + '</div></div>' +
      '<div class="a-acts">' + acts + '</div></div>';
    }).join('');
    if (!list.length) {
      items = '<div class="card">' + emptyHtml('这里还没有公告', '点击「新建公告」发布第一条消息给所有创作者', '<button class="btn btn-primary btn-sm" data-action="ann-new">' + svg('plus', 13) + ' 新建公告</button>') + '</div>';
    }
    return '<div class="toolbar">' +
      seg('anns', 'status', [['all', '全部'], ['published', '已发布'], ['scheduled', '定时'], ['draft', '草稿']], state.f.anns.status) +
      '<div class="push"><button class="btn btn-primary" data-action="ann-new">' + svg('plus') + ' 新建公告</button></div>' +
      '</div>' +
      '<div class="ann-list">' + items + '</div>';
  }
};
})();
