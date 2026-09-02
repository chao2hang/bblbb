/* ══ 操作日志 ══ */
(function () {
'use strict';
var B = window.B;
var D = B.D, state = B.state;
var $ = B.$, $$ = B.$$;
var svg = B.svg, esc = B.esc;
var LOG_CAT_COLOR = B.LOG_CAT_COLOR;

function filteredLogs() {
  var f = state.f.logs;
  return D.logs.filter(function (l) {
    if (f.cat !== 'all' && l.cat !== f.cat) return false;
    if (f.q) {
      var q = f.q.toLowerCase();
      if (l.action.toLowerCase().indexOf(q) < 0 && l.target.toLowerCase().indexOf(q) < 0 && l.actor.toLowerCase().indexOf(q) < 0) return false;
    }
    return true;
  });
}
function logsBody() {
  var list = filteredLogs();
  if (!list.length) {
    return '<div class="card">' + (D.logs.length
      ? emptyHtml('没有匹配的日志', '换个关键词，或清除筛选条件试试', '<button class="btn btn-ghost btn-sm" data-action="clear-filters">清除筛选</button>')
      : emptyHtml('日志已清空', '新的操作会自动记录在这里', '')) + '</div>';
  }
  var rows = list.map(function (l) {
    return '<tr>' +
    '<td class="mono">' + l.t + '</td>' +
    '<td><span class="tag ' + (LOG_CAT_COLOR[l.cat] || 'tag-dim') + '">' + l.cat + '</span></td>' +
    '<td>' + esc(l.actor) + '</td>' +
    '<td class="cell-main">' + esc(l.action) + '</td>' +
    '<td class="cell-sub" style="white-space:normal;max-width:340px">' + esc(l.target) + '</td></tr>';
  }).join('');
  return '<div class="card table-wrap"><table class="table"><thead><tr><th>时间</th><th>类别</th><th>操作者</th><th>操作</th><th>对象</th></tr></thead><tbody>' + rows + '</tbody></table>' +
    '<div class="list-foot">共 ' + list.length + ' 条记录</div></div>';
}

window.PAGE = {
  title: '操作日志',
  sub: function () {
    return '最近 ' + D.logs.length + ' 条操作记录';
  },
  html: function () {
    return '<div class="toolbar">' +
      searchBox('logs', '搜索操作 / 对象 / 操作者') +
      seg('logs', 'cat', [['all', '全部'], ['审核', '审核'], ['用户', '用户'], ['项目', '项目'], ['公告', '公告'], ['账单', '账单'], ['系统', '系统']], state.f.logs.cat) +
      '<div class="push">' +
      '<button class="btn btn-ghost" data-action="export-logs">' + svg('download') + ' 导出 CSV</button>' +
      '<button class="btn btn-danger" data-action="logs-clear">' + svg('trash', 13) + ' 清空日志</button>' +
      '</div></div>' +
      '<div id="listwrap">' + logsBody() + '</div>';
  },
  list: logsBody,
  actions: {
    'export-logs': function () {
      var list = filteredLogs();
      var rows = [['时间', '类别', '操作者', '操作', '对象']];
      list.forEach(function (l) { rows.push([l.t, l.cat, l.actor, l.action, l.target]); });
      downloadCSV('bblbb-logs.csv', rows);
      toast('已导出 ' + list.length + ' 条日志（CSV）');
    },
    'logs-clear': function () {
      confirmBox('清空全部操作日志？<br>清空后无法恢复，仅影响后台展示。', '清空日志', 'btn-danger', function () {
        D.logs = [];
        B.saveData();
        toast('日志已清空', 'info');
        rerenderFull();
      });
    }
  }
};
})();
