/* ══ 审核队列 ══ */
(function () {
'use strict';
var B = window.B;
var D = B.D, state = B.state;
var $ = B.$, $$ = B.$$;
var svg = B.svg, esc = B.esc, grad = B.grad;
var REV_STATUS = B.REV_STATUS;

function filteredReviews() {
  return D.reviews.filter(function (r) { return r.status === state.f.reviews.status; });
}
function reviewCards(list) {
  if (!list.length) {
    var pending = state.f.reviews.status === 'pending';
    return '<div class="card">' + emptyHtml(
      pending ? '队列已清空' : '这里还没有记录',
      pending ? '所有作品都已处理完毕' : '稍后再来看看',
      '<button class="btn btn-ghost btn-sm" data-action="clear-filters">查看全部</button>') + '</div>';
  }
  var cards = list.map(function (r) {
    var isPend = r.status === 'pending';
    return '<div class="card review-card">' +
    '<div class="rc-cover" style="' + grad(r.grad) + '"><span>' + esc(r.title[0]) + '</span>' +
    '<span class="tag ' + (r.featured ? 'tag-purple' : REV_STATUS[r.status][1]) + '" style="position:absolute;top:10px;left:10px">' + (r.featured ? '已推荐' : REV_STATUS[r.status][0]) + '</span>' +
    (isPend ? '<input type="checkbox" class="cbx" data-sel="reviews" data-id="' + r.id + '" ' + (state.sel.reviews[r.id] ? 'checked' : '') + ' style="position:absolute;top:10px;right:10px;width:18px;height:18px" aria-label="选择 ' + esc(r.title) + '">' : '') +
    '</div>' +
    '<div class="rc-body"><div class="rc-title">' + esc(r.title) + '</div><div class="rc-meta">' + esc(r.author) + ' · ' + r.type + ' · ' + r.time + (r.status === 'rejected' && r.reason ? ' · ' + esc(r.reason) : '') + '</div></div>' +
    '<div class="rc-foot">' +
    (isPend
      ? '<button class="btn btn-green btn-sm" data-action="review-approve" data-id="' + r.id + '">' + svg('check', 12) + ' 通过</button>' +
      '<button class="btn btn-danger btn-sm" data-action="review-reject" data-id="' + r.id + '">驳回</button>' +
      '<button class="btn btn-ghost btn-sm" data-action="review-recommend" data-id="' + r.id + '" title="推荐到发现页">' + svg('star', 13) + '</button>'
      : '<button class="btn btn-ghost btn-sm" data-action="review-view" data-id="' + r.id + '">' + svg('eye', 13) + ' 查看</button>') +
    '</div></div>';
  }).join('');
  return '<div class="review-grid">' + cards + '</div>';
}
function reviewListWrap() {
  var n = Object.keys(state.sel.reviews).filter(function (k) { return state.sel.reviews[k]; }).length;
  var bar = '';
  if (state.f.reviews.status === 'pending' && n > 0) {
    bar = '<div class="batchbar"><span class="cnt">已选 <b>' + n + '</b> 条作品</span><span class="btns">' +
      '<button class="btn btn-ghost btn-sm" data-action="sel-clear" data-kind="reviews">取消选择</button>' +
      '<button class="btn btn-green btn-sm" data-action="batch-review-approve">' + svg('check', 12) + ' 批量通过</button>' +
      '</span></div>';
  }
  return bar + reviewCards(filteredReviews());
}

window.PAGE = {
  title: '审核队列',
  sub: function () {
    return B.pendingCount() + ' 条待处理 · 平均处理用时 14 分钟';
  },
  html: function () {
    return '<div class="toolbar">' +
      seg('reviews', 'status',
        [['pending', '待处理 · ' + B.countBy(D.reviews, 'status', 'pending')],
         ['approved', '已通过 · ' + B.countBy(D.reviews, 'status', 'approved')],
         ['rejected', '已驳回 · ' + B.countBy(D.reviews, 'status', 'rejected')]],
        state.f.reviews.status) +
      '<div class="push"><span class="cell-sub">驳回需填写原因，会随通知发给作者</span></div>' +
      '</div>' +
      '<div id="listwrap">' + reviewListWrap() + '</div>';
  },
  list: reviewListWrap
};
})();
