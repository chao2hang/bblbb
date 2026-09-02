/* ══ 项目管理 ══ */
(function () {
'use strict';
var B = window.B;
var D = B.D, state = B.state;
var $ = B.$, $$ = B.$$;
var svg = B.svg, esc = B.esc, grad = B.grad;
var PROJ_STATUS = B.PROJ_STATUS;
var findProject = window.findProject;

function filteredProjects() {
  var f = state.f.projects;
  return D.projects.filter(function (p) {
    if (f.status !== 'all' && p.status !== f.status) return false;
    if (f.q) {
      var q = f.q.toLowerCase();
      if (p.name.toLowerCase().indexOf(q) < 0 && p.owner.toLowerCase().indexOf(q) < 0) return false;
    }
    return true;
  });
}
function projectRows(list) {
  if (!list.length) {
    return '<div class="card">' + emptyHtml('没有匹配的项目', '换个关键词，或新建一个项目', '<button class="btn btn-ghost btn-sm" data-action="clear-filters">清除筛选</button>') + '</div>';
  }
  var rows = list.map(function (p) {
    var st = PROJ_STATUS[p.status];
    return '<tr>' +
    '<td><input type="checkbox" class="cbx" data-sel="projects" data-id="' + p.id + '" ' + (state.sel.projects[p.id] ? 'checked' : '') + ' aria-label="选择 ' + esc(p.name) + '"></td>' +
    '<td><div class="user-cell"><span class="cover" style="' + grad(p.grad) + '"><span>' + esc(p.name[0]) + '</span></span>' +
    '<div><div class="nm">' + esc(p.name) + '</div><div class="cell-sub mono">' + p.id + '</div></div></div></td>' +
    '<td>' + esc(p.owner) + '</td>' +
    '<td><span class="tag ' + st[1] + '">' + st[0] + '</span></td>' +
    '<td class="mono">' + p.views + '</td>' +
    '<td class="mono">' + p.likes + '</td>' +
    '<td class="cell-sub">' + p.updated + '</td>' +
    '<td><div class="row-acts">' +
    '<button class="btn-icon" title="查看" data-action="project-view" data-id="' + p.id + '">' + svg('eye') + '</button>' +
    '<button class="btn-icon" title="编辑" data-action="project-edit" data-id="' + p.id + '">' + svg('pen') + '</button>' +
    (p.status === 'published'
      ? '<button class="btn-icon warn" title="下线" data-action="project-toggle" data-id="' + p.id + '">' + svg('lock') + '</button>'
      : '<button class="btn-icon" title="上线" data-action="project-toggle" data-id="' + p.id + '">' + svg('unlock') + '</button>') +
    '<button class="btn-icon warn" title="删除" data-action="project-delete" data-id="' + p.id + '">' + svg('trash') + '</button>' +
    '</div></td></tr>';
  }).join('');
  return '<div class="card table-wrap"><table class="table"><thead><tr>' +
    '<th style="width:36px"><input type="checkbox" class="cbx" data-sel="projects" data-all aria-label="全选"></th>' +
    '<th>项目</th><th>所有者</th><th>状态</th><th>浏览</th><th>点赞</th><th>更新</th><th style="text-align:right">操作</th>' +
    '</tr></thead><tbody>' + rows + '</tbody></table>' +
    '<div class="list-foot">共 ' + list.length + ' 个项目' + (state.f.projects.q || state.f.projects.status !== 'all' ? '（已筛选）' : '') + '</div></div>';
}
function projectListWrap() {
  return batchbar('projects') + projectRows(filteredProjects());
}

function modalProject(p, isNew) {
  var owners = D.users.filter(function (u) { return u.status !== 'frozen'; });
  openModal(isNew ? '新建项目' : '编辑项目 · ' + esc(p.name),
    '<div class="field"><label>项目名称</label><input class="input" id="pj-name" value="' + esc(p ? p.name : '') + '" placeholder="给项目起个名字"></div>' +
    '<div class="field"><label>所有者</label><select class="select" id="pj-owner">' +
    owners.map(function (u) { return '<option' + (p && p.owner === u.name ? ' selected' : '') + '>' + esc(u.name) + '</option>'; }).join('') +
    '</select></div>' +
    '<div class="field"><label>状态</label><select class="select" id="pj-status">' +
    '<option value="draft"' + (!p || p.status === 'draft' ? ' selected' : '') + '>草稿</option>' +
    '<option value="published"' + (p && p.status === 'published' ? ' selected' : '') + '>已发布</option>' +
    '</select></div>' +
    '<div class="field" style="margin:0"><label>描述</label><textarea class="textarea" id="pj-desc" placeholder="一句话介绍这个项目">' + esc(p ? p.desc || '' : '') + '</textarea></div>',
    '<button class="btn btn-ghost" data-action="modal-close">取消</button>' +
    '<button class="btn btn-primary" data-action="proj-save" data-id="' + (p ? p.id : '') + '">' + svg('check') + ' ' + (isNew ? '创建项目' : '保存项目') + '</button>');
}
function modalProjectView(p) {
  var st = PROJ_STATUS[p.status];
  openModal('项目详情',
    '<div class="detail-cover" style="' + grad(p.grad) + '">' + esc(p.name[0]) + '</div>' +
    '<div style="font-size:16px;font-weight:700;margin-bottom:4px">' + esc(p.name) + '</div>' +
    '<div class="cell-sub mono" style="margin-bottom:14px">' + p.id + ' · ' + st[0] + '</div>' +
    '<div class="stat-strip">' +
    '<div class="st"><b>' + p.views + '</b><span>浏览</span></div>' +
    '<div class="st"><b>' + p.likes + '</b><span>点赞</span></div>' +
    '<div class="st"><b>' + Math.round(parseInt(p.views, 10) / 3 || 48) + '</b><span>分享</span></div></div>' +
    '<div class="info-row"><span class="k">所有者</span><span class="v">' + esc(p.owner) + '</span></div>' +
    '<div class="info-row"><span class="k">最近更新</span><span class="v">' + p.updated + '</span></div>' +
    '<div class="info-row"><span class="k">描述</span><span class="v" style="font-weight:400;text-align:right;max-width:280px;white-space:normal">' + esc(p.desc || '—') + '</span></div>',
    '<button class="btn btn-ghost" data-action="project-edit" data-id="' + p.id + '">' + svg('pen', 13) + ' 编辑</button>' +
    (p.status === 'published'
      ? '<button class="btn btn-ghost" data-action="project-toggle" data-id="' + p.id + '">' + svg('lock', 13) + ' 下线</button>'
      : '<button class="btn btn-green" data-action="project-toggle" data-id="' + p.id + '">' + svg('unlock', 13) + ' 上线</button>') +
    '<button class="btn btn-danger" data-action="project-delete" data-id="' + p.id + '">' + svg('trash', 13) + ' 删除</button>');
}

window.PAGE = {
  title: '项目管理',
  sub: function () {
    return '共 ' + D.projects.length + ' 个项目 · ' + B.countBy(D.projects, 'status', 'published') + ' 个已发布';
  },
  html: function () {
    return '<div class="toolbar">' +
      searchBox('projects', '搜索项目 / 所有者') +
      seg('projects', 'status', [['all', '全部'], ['published', '已发布'], ['draft', '草稿'], ['offline', '已下线']], state.f.projects.status) +
      '<div class="push"><button class="btn btn-primary" data-action="project-new">' + svg('plus') + ' 新建项目</button></div>' +
      '</div>' +
      '<div id="listwrap">' + projectListWrap() + '</div>';
  },
  list: projectListWrap,
  actions: {
    'project-new': function () { modalProject(null, true); },
    'project-edit': function (id) { var p = findProject(id); if (p) { closeModal(); modalProject(p, false); } },
    'proj-save': function (id) {
      var name = ($('#pj-name') || {}).value;
      if (!name) { toast('请填写项目名称', 'danger'); return; }
      var owner = ($('#pj-owner') || {}).value;
      var status = ($('#pj-status') || {}).value;
      var desc = ($('#pj-desc') || {}).value;
      if (id) {
        var p = findProject(id);
        p.name = name; p.owner = owner; p.status = status; p.desc = desc; p.updated = '刚刚';
        B.saveAfter('编辑项目', '「' + name + '」', '项目');
        toast('项目已更新');
      } else {
        var grads = [['#7C3AED', '#0EA5E9'], ['#00E5A0', '#7C3AED'], ['#F472B6', '#7C3AED'], ['#F59E0B', '#F43F5E']];
        D.projects.unshift({
          id: nextId('P', D.projects, 4), name: name, owner: owner, status: status,
          views: '0', likes: '0', updated: '刚刚', grad: grads[Math.floor(Math.random() * grads.length)], desc: desc
        });
        B.saveAfter('创建项目', '「' + name + '」', '项目');
        toast('项目已创建' + (status === 'published' ? '并发布' : '，当前为草稿'));
      }
      B.saveData();
      closeModal();
      rerenderFull();
    },
    'project-view': function (id) { var p = findProject(id); if (p) modalProjectView(p); },
    'project-toggle': function (id) {
      var p = findProject(id);
      if (!p) return;
      var on = p.status !== 'published';
      p.status = on ? 'published' : 'offline';
      p.updated = '刚刚';
      B.saveAfter(on ? '上线项目' : '下线项目', '「' + p.name + '」', '项目');
      B.saveData();
      closeModal();
      toast(on ? '已上线「' + p.name + '」' : '已下线「' + p.name + '」，前台不再展示', on ? 'success' : 'info');
      rerenderFull();
    },
    'project-delete': function (id) {
      var p = findProject(id);
      if (!p) return;
      confirmBox('删除项目 <b>「' + esc(p.name) + '」</b>（' + p.id + '）？<br>作品数据将归档 30 天后清除。', '删除项目', 'btn-danger', function () {
        D.projects = D.projects.filter(function (x) { return x.id !== id; });
        delete state.sel.projects[id];
        B.saveAfter('删除项目', '「' + p.name + '」', '项目');
        B.saveData();
        closeModal();
        toast('已删除「' + p.name + '」', 'danger');
        rerenderFull();
      });
    },
    'batch-project-delete': function () {
      var ids = Object.keys(state.sel.projects).filter(function (k) { return state.sel.projects[k]; });
      confirmBox('删除所选 <b>' + ids.length + '</b> 个项目？', '删除所选', 'btn-danger', function () {
        D.projects = D.projects.filter(function (p) { return ids.indexOf(p.id) < 0; });
        B.saveAfter('批量删除项目', ids.length + ' 个', '项目');
        B.saveData();
        state.sel.projects = {};
        toast('已删除 ' + ids.length + ' 个项目', 'danger');
        rerenderFull();
      });
    }
  }
};
})();
