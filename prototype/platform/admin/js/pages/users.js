/* ══ 创作者管理 ══ */
(function () {
'use strict';
var B = window.B;
var D = B.D, state = B.state;
var $ = B.$, $$ = B.$$;
var svg = B.svg, esc = B.esc;
var USER_STATUS = B.USER_STATUS;
var findUser = window.findUser;

function filteredUsers() {
  var f = state.f.users;
  return D.users.filter(function (u) {
    if (f.status !== 'all' && u.status !== f.status) return false;
    if (f.plan !== 'all' && u.plan !== f.plan) return false;
    if (f.q) {
      var q = f.q.toLowerCase();
      if (u.name.toLowerCase().indexOf(q) < 0 && u.handle.toLowerCase().indexOf(q) < 0 && u.email.toLowerCase().indexOf(q) < 0) return false;
    }
    return true;
  });
}
function userRows(list) {
  if (!list.length) {
    return '<div class="card">' + emptyHtml('没有匹配的创作者', '换个关键词，或清除筛选条件试试', '<button class="btn btn-ghost btn-sm" data-action="clear-filters">清除筛选</button>') + '</div>';
  }
  var rows = list.map(function (u) {
    var st = USER_STATUS[u.status];
    var frozen = u.status === 'frozen';
    return '<tr>' +
    '<td><input type="checkbox" class="cbx" data-sel="users" data-id="' + u.id + '" ' + (state.sel.users[u.id] ? 'checked' : '') + ' aria-label="选择 ' + esc(u.name) + '"></td>' +
    '<td><div class="user-cell"><span class="avatar" style="background:hsl(' + u.hue + ' 55% 42%)">' + esc(u.name[0]) + '</span>' +
    '<div><div class="nm">' + esc(u.name) + '</div><div class="cell-sub">@' + esc(u.handle) + ' · ' + esc(u.email) + '</div></div></div></td>' +
    '<td class="pts">' + u.points.toLocaleString() + '</td>' +
    '<td>' + (u.plan === 'Free' ? '<span class="tag tag-dim no-dot">Free</span>' : '<span class="tag tag-purple">' + u.plan + '</span>') + '</td>' +
    '<td><span class="tag ' + st[1] + '">' + st[0] + '</span></td>' +
    '<td class="cell-sub">' + u.active + '</td>' +
    '<td class="mono">' + u.joined + '</td>' +
    '<td><div class="row-acts">' +
    '<button class="btn-icon" title="查看资料" data-action="user-view" data-id="' + u.id + '">' + svg('eye') + '</button>' +
    '<button class="btn-icon" title="调整积分" data-action="user-points" data-id="' + u.id + '">' + svg('coins') + '</button>' +
    (frozen
      ? '<button class="btn-icon" title="解冻账号" data-action="user-freeze" data-id="' + u.id + '">' + svg('unlock') + '</button>'
      : '<button class="btn-icon warn" title="冻结账号" data-action="user-freeze" data-id="' + u.id + '">' + svg('lock') + '</button>') +
    '<button class="btn-icon warn" title="删除" data-action="user-delete" data-id="' + u.id + '">' + svg('trash') + '</button>' +
    '</div></td></tr>';
  }).join('');
  return '<div class="card table-wrap"><table class="table"><thead><tr>' +
    '<th style="width:36px"><input type="checkbox" class="cbx" data-sel="users" data-all aria-label="全选"></th>' +
    '<th>创作者</th><th>积分</th><th>方案</th><th>状态</th><th>最近活跃</th><th>加入</th><th style="text-align:right">操作</th>' +
    '</tr></thead><tbody>' + rows + '</tbody></table>' +
    '<div class="list-foot">共 ' + list.length + ' 位创作者' + (state.f.users.q || state.f.users.status !== 'all' || state.f.users.plan !== 'all' ? '（已筛选）' : '') + '</div></div>';
}
function userListWrap() {
  return batchbar('users') + userRows(filteredUsers());
}

function modalUserView(u) {
  openModal('创作者资料',
    '<div style="display:flex;align-items:center;gap:14px;margin-bottom:18px">' +
    '<span class="avatar" style="width:52px;height:52px;font-size:20px;background:hsl(' + u.hue + ' 55% 42%)">' + esc(u.name[0]) + '</span>' +
    '<div><div style="font-size:16px;font-weight:700">' + esc(u.name) + ' <span class="cell-sub">@' + esc(u.handle) + '</span></div>' +
    '<div style="margin-top:4px">' + B.tag(USER_STATUS, u.status) + ' <span class="tag ' + (u.plan === 'Free' ? 'tag-dim no-dot' : 'tag-purple') + '">' + u.plan + '</span></div></div></div>' +
    '<div class="stat-strip">' +
    '<div class="st"><b>' + u.points.toLocaleString() + '</b><span>积分</span></div>' +
    '<div class="st"><b>' + u.projects + '</b><span>项目</span></div>' +
    '<div class="st"><b style="font-size:14px;padding-top:4px">' + u.active + '</b><span>最近活跃</span></div></div>' +
    '<div class="info-row"><span class="k">创作者编号</span><span class="v mono">' + u.id + '</span></div>' +
    '<div class="info-row"><span class="k">邮箱</span><span class="v">' + esc(u.email) + '</span></div>' +
    '<div class="info-row"><span class="k">加入时间</span><span class="v mono">' + u.joined + '</span></div>',
    '<button class="btn btn-ghost" data-action="user-points" data-id="' + u.id + '">' + svg('coins', 13) + ' 调整积分</button>' +
    (u.status === 'frozen'
      ? '<button class="btn btn-green" data-action="user-freeze" data-id="' + u.id + '">' + svg('unlock', 13) + ' 解冻账号</button>'
      : '<button class="btn btn-danger" data-action="user-freeze" data-id="' + u.id + '">' + svg('lock', 13) + ' 冻结账号</button>') +
    '<button class="btn btn-primary" data-action="modal-close">完成</button>');
}
function modalUserPoints(u) {
  openModal('调整积分 · ' + esc(u.name),
    '<div class="field"><label>当前积分</label><div class="stepper"><span class="pts-big">' + u.points.toLocaleString() + '</span></div></div>' +
    '<div class="field"><label>调整金额</label><div class="stepper">' +
    '<button class="btn btn-ghost btn-sm" data-action="pts-step" data-d="-1000">−1000</button>' +
    '<button class="btn btn-ghost btn-sm" data-action="pts-step" data-d="-500">−500</button>' +
    '<button class="btn btn-ghost btn-sm" data-action="pts-step" data-d="500">+500</button>' +
    '<button class="btn btn-ghost btn-sm" data-action="pts-step" data-d="1000">+1000</button>' +
    '<input class="input" id="pts-val" type="number" value="0" step="100" style="width:130px" aria-label="积分调整值"></div>' +
    '<div class="hint">正数为增加，负数为扣减。调整后会写入操作日志，并站内信通知本人。</div></div>',
    '<button class="btn btn-ghost" data-action="modal-close">取消</button>' +
    '<button class="btn btn-primary" data-action="pts-save" data-id="' + u.id + '">' + svg('check') + ' 确认调整</button>');
}

window.PAGE = {
  title: '创作者管理',
  sub: function () {
    return '共 ' + D.users.length + ' 位创作者 · ' + B.countBy(D.users, 'status', 'verified') + ' 位已认证';
  },
  html: function () {
    return '<div class="toolbar">' +
      searchBox('users', '搜索名称 / 账号 / 邮箱') +
      seg('users', 'status', [['all', '全部'], ['verified', '已认证'], ['pending', '待验证'], ['frozen', '已冻结']], state.f.users.status) +
      '<select class="select" data-plan="users" style="width:auto">' +
      '<option value="all"' + (state.f.users.plan === 'all' ? ' selected' : '') + '>全部方案</option>' +
      '<option value="Free"' + (state.f.users.plan === 'Free' ? ' selected' : '') + '>Free</option>' +
      '<option value="Pro"' + (state.f.users.plan === 'Pro' ? ' selected' : '') + '>Pro</option>' +
      '<option value="Studio"' + (state.f.users.plan === 'Studio' ? ' selected' : '') + '>Studio</option>' +
      '</select>' +
      '<div class="push"><button class="btn btn-primary" data-action="user-invite">' + svg('plus') + ' 邀请创作者</button></div>' +
      '</div>' +
      '<div id="listwrap">' + userListWrap() + '</div>';
  },
  list: userListWrap,
  actions: {
    'user-view': function (id) { var u = findUser(id); if (u) modalUserView(u); },
    'user-points': function (id) { var u = findUser(id); if (u) { closeModal(); modalUserPoints(u); } },
    'pts-step': function (id, el) {
      var inp = $('#pts-val');
      if (!inp) return;
      inp.value = parseInt(inp.value || '0', 10) + parseInt(el.dataset.d, 10);
    },
    'pts-save': function (id) {
      var u = findUser(id), inp = $('#pts-val');
      var delta = inp ? parseInt(inp.value, 10) : 0;
      if (!delta) { toast('调整金额为 0，无需调整', 'info'); return; }
      u.points = Math.max(0, u.points + delta);
      B.saveAfter('调整积分', u.name + ' ' + (delta > 0 ? '+' : '') + delta, '用户');
      B.saveData();
      closeModal();
      toast('已将 ' + u.name + ' 的积分' + (delta > 0 ? '增加 ' : '扣减 ') + Math.abs(delta).toLocaleString());
      rerenderFull();
    },
    'user-freeze': function (id) {
      var u = findUser(id);
      if (!u) return;
      if (u.status === 'frozen') {
        u.status = 'verified';
        B.saveAfter('解冻账号', u.name + ' (' + u.id + ')', '用户');
        B.saveData();
        closeModal();
        toast('已解冻 ' + u.name + ' 的账号，作品已恢复展示');
        rerenderFull();
      } else {
        confirmBox('冻结 <b>' + esc(u.name) + '</b>（' + u.id + '）？<br>冻结后对方将无法登录，' + u.projects + ' 个项目将暂时下线。', '冻结账号', 'btn-danger', function () {
          u.status = 'frozen';
          B.saveAfter('冻结账号', u.name + ' (' + u.id + ')', '用户');
          B.saveData();
          toast('已冻结 ' + u.name + ' 的账号，其作品已下线', 'danger');
          rerenderFull();
        });
      }
    },
    'user-delete': function (id) {
      var u = findUser(id);
      if (!u) return;
      confirmBox('删除创作者 <b>' + esc(u.name) + '</b>（' + u.id + '）？<br>其 ' + u.projects + ' 个项目将一并归档，此操作不可恢复。', '删除创作者', 'btn-danger', function () {
        D.users = D.users.filter(function (x) { return x.id !== id; });
        delete state.sel.users[id];
        B.saveAfter('删除创作者', u.name + ' (' + u.id + ')', '用户');
        B.saveData();
        closeModal();
        toast('已删除 ' + u.name + '，项目已归档', 'danger');
        rerenderFull();
      });
    },
    'batch-user-freeze': function () {
      var ids = Object.keys(state.sel.users).filter(function (k) { return state.sel.users[k]; });
      confirmBox('冻结所选 <b>' + ids.length + '</b> 位创作者？冻结后他们将无法登录。', '冻结所选', 'btn-danger', function () {
        ids.forEach(function (id) { var u = findUser(id); if (u && u.status !== 'frozen') u.status = 'frozen'; });
        B.saveAfter('批量冻结', ids.length + ' 位创作者', '用户');
        B.saveData();
        state.sel.users = {};
        toast('已冻结 ' + ids.length + ' 位创作者', 'danger');
        rerenderFull();
      });
    },
    'batch-user-delete': function () {
      var ids = Object.keys(state.sel.users).filter(function (k) { return state.sel.users[k]; });
      confirmBox('删除所选 <b>' + ids.length + '</b> 位创作者？此操作不可恢复。', '删除所选', 'btn-danger', function () {
        D.users = D.users.filter(function (u) { return ids.indexOf(u.id) < 0; });
        B.saveAfter('批量删除创作者', ids.length + ' 位', '用户');
        B.saveData();
        state.sel.users = {};
        toast('已删除 ' + ids.length + ' 位创作者', 'danger');
        rerenderFull();
      });
    },
    'batch-user-export': function () {
      var ids = Object.keys(state.sel.users).filter(function (k) { return state.sel.users[k]; });
      var rows = [['名称', '账号', '邮箱', '积分', '方案', '状态', '加入时间', '最近活跃']];
      ids.forEach(function (id) {
        var u = findUser(id);
        if (u) rows.push([u.name, u.handle, u.email, u.points, u.plan, USER_STATUS[u.status][0], u.joined, u.active]);
      });
      downloadCSV('bblbb-users.csv', rows);
      toast('已导出 ' + ids.length + ' 位创作者（CSV）');
    }
  }
};
})();
