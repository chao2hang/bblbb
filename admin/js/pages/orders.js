/* ══ 订单与账单 ══ */
(function () {
'use strict';
var B = window.B;
var D = B.D, state = B.state;
var $ = B.$, $$ = B.$$;
var svg = B.svg, esc = B.esc;
var ORD_STATUS = B.ORD_STATUS;
var findOrder = window.findOrder;

function filteredOrders() {
  var f = state.f.orders;
  return D.orders.filter(function (o) {
    if (f.status !== 'all' && o.status !== f.status) return false;
    if (f.q) {
      var q = f.q.toLowerCase();
      if (o.id.toLowerCase().indexOf(q) < 0 && o.user.toLowerCase().indexOf(q) < 0 && o.item.toLowerCase().indexOf(q) < 0) return false;
    }
    return true;
  });
}
function ordersBody() {
  var list = filteredOrders();
  if (!list.length) {
    return '<div class="card">' + emptyHtml('没有匹配的订单', '换个关键词，或清除筛选条件试试', '<button class="btn btn-ghost btn-sm" data-action="clear-filters">清除筛选</button>') + '</div>';
  }
  var rows = list.map(function (o) {
    var acts = '<button class="btn-icon" title="查看详情" data-action="order-view" data-id="' + o.id + '">' + svg('eye') + '</button>';
    if (o.status === 'paid') acts += '<button class="btn-icon warn" title="发起退款" data-action="order-refund" data-id="' + o.id + '">' + svg('refresh') + '</button>';
    if (o.status === 'processing') acts += '<button class="btn btn-green btn-sm" data-action="order-finish" data-id="' + o.id + '">标记完成</button>';
    if (o.status === 'refunding') acts += '<button class="btn btn-green btn-sm" data-action="order-refund-done" data-id="' + o.id + '">确认退款完成</button>';
    return '<tr>' +
    '<td class="mono">' + o.id + '</td>' +
    '<td class="cell-main">' + esc(o.user) + '</td>' +
    '<td>' + esc(o.item) + '</td>' +
    '<td class="mono" style="color:var(--t1)">' + o.amount + '</td>' +
    '<td>' + B.tag(ORD_STATUS, o.status) + '</td>' +
    '<td class="cell-sub">' + o.pay + '</td>' +
    '<td class="mono">' + o.time + '</td>' +
    '<td><div class="row-acts">' + acts + '</div></td></tr>';
  }).join('');
  return '<div class="card table-wrap"><table class="table"><thead><tr><th>订单号</th><th>用户</th><th>商品</th><th>金额</th><th>状态</th><th>支付方式</th><th>时间</th><th style="text-align:right">操作</th></tr></thead><tbody>' + rows + '</tbody></table>' +
    '<div class="list-foot">共 ' + list.length + ' 笔订单' + (state.f.orders.q || state.f.orders.status !== 'all' ? '（已筛选）' : '') + '</div></div>';
}
function modalOrder(o) {
  var foot = '<button class="btn btn-ghost" data-action="modal-close">关闭</button>';
  if (o.status === 'paid') foot = '<button class="btn btn-danger" data-action="order-refund" data-id="' + o.id + '">' + svg('refresh', 13) + ' 发起退款</button>' + foot;
  if (o.status === 'processing') foot = '<button class="btn btn-green" data-action="order-finish" data-id="' + o.id + '">' + svg('check', 13) + ' 标记完成</button>' + foot;
  if (o.status === 'refunding') foot = '<button class="btn btn-green" data-action="order-refund-done" data-id="' + o.id + '">' + svg('check', 13) + ' 确认退款完成</button>' + foot;
  openModal('订单详情',
    '<div class="info-row"><span class="k">订单号</span><span class="v mono">' + o.id + '</span></div>' +
    '<div class="info-row"><span class="k">用户</span><span class="v">' + esc(o.user) + '</span></div>' +
    '<div class="info-row"><span class="k">商品</span><span class="v">' + esc(o.item) + '</span></div>' +
    '<div class="info-row"><span class="k">金额</span><span class="v mono" style="font-size:15px">' + o.amount + '</span></div>' +
    '<div class="info-row"><span class="k">状态</span><span class="v">' + B.tag(ORD_STATUS, o.status) + '</span></div>' +
    '<div class="info-row"><span class="k">支付方式</span><span class="v">' + o.pay + '</span></div>' +
    '<div class="info-row"><span class="k">下单时间</span><span class="v mono">' + o.time + '</span></div>',
    foot);
}

window.PAGE = {
  title: '订单与账单',
  sub: function () {
    return '本月 GMV ¥14,358 · 待处理订单 ' + B.countBy(D.orders, 'status', 'processing');
  },
  html: function () {
    return '<div class="toolbar">' +
      searchBox('orders', '搜索订单号 / 用户 / 商品') +
      seg('orders', 'status', [['all', '全部'], ['paid', '已支付'], ['processing', '处理中'], ['refunding', '退款中'], ['refunded', '已退款']], state.f.orders.status) +
      '<div class="push"><button class="btn btn-ghost" data-action="export-orders">' + svg('download') + ' 导出 CSV</button></div>' +
      '</div>' +
      '<div id="listwrap">' + ordersBody() + '</div>';
  },
  list: ordersBody,
  actions: {
    'order-view': function (id) { var o = findOrder(id); if (o) modalOrder(o); },
    'order-refund': function (id) {
      var o = findOrder(id);
      if (!o || o.status !== 'paid') return;
      confirmBox('对订单 <b>' + o.id + '</b>（' + o.amount + '）发起退款？<br>款项将原路退回至' + o.pay + '，且不可撤销。', '发起退款', 'btn-danger', function () {
        o.status = 'refunding';
        B.saveAfter('发起退款', o.id + ' · ' + o.amount, '账单');
        B.saveData();
        closeModal();
        toast('退款申请已提交，预计 1–3 个工作日到账');
        rerenderFull();
      });
    },
    'order-finish': function (id) {
      var o = findOrder(id);
      if (!o || o.status !== 'processing') return;
      o.status = 'paid';
      B.saveAfter('标记订单完成', o.id, '账单');
      B.saveData();
      closeModal();
      toast('订单 ' + o.id + ' 已标记完成');
      rerenderFull();
    },
    'order-refund-done': function (id) {
      var o = findOrder(id);
      if (!o || o.status !== 'refunding') return;
      o.status = 'refunded';
      B.saveAfter('退款完成', o.id + ' · ' + o.amount, '账单');
      B.saveData();
      closeModal();
      toast('订单 ' + o.id + ' 退款已原路退回');
      rerenderFull();
    },
    'export-orders': function () {
      var list = filteredOrders();
      var rows = [['订单号', '用户', '商品', '金额', '状态', '支付方式', '时间']];
      list.forEach(function (o) { rows.push([o.id, o.user, o.item, o.amount, ORD_STATUS[o.status][0], o.pay, o.time]); });
      downloadCSV('bblbb-orders.csv', rows);
      toast('已导出 ' + list.length + ' 笔订单（CSV）');
    }
  }
};
})();
