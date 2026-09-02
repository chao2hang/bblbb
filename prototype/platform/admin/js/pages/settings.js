/* ══ 系统设置 ══ */
(function () {
'use strict';
var B = window.B;
var D = B.D, state = B.state;
var $ = B.$, $$ = B.$$;
var svg = B.svg, esc = B.esc;

/* 已保存快照:用于「放弃更改」回滚 */
var SNAPSHOT = JSON.stringify(D.settings);
/* 读取当前页签中存在的输入框,合并进 D.settings(切页签时自动暂存,不丢值) */
function collectSettings() {
  var name = $('#s-name'); if (name) D.settings.name = name.value.trim();
  var slogan = $('#s-slogan'); if (slogan) D.settings.slogan = slogan.value.trim();
  var email = $('#s-email'); if (email) D.settings.email = email.value.trim();
  var maint = $('#s-maint'); if (maint) D.settings.maint = maint.checked;
  var auto = $('#s-auto'); if (auto) D.settings.auto = auto.checked;
  var cond = $('#s-cond'); if (cond) D.settings.cond = cond.value;
  var cap = $('#s-cap'); if (cap) D.settings.cap = parseInt(cap.value, 10) || 0;
  var ntMail = $('#s-nt-mail'); if (ntMail) D.settings.ntMail = ntMail.checked;
  var ntIn = $('#s-nt-in'); if (ntIn) D.settings.ntIn = ntIn.checked;
  var ntSms = $('#s-nt-sms'); if (ntSms) D.settings.ntSms = ntSms.checked;
  var tpl = $('#s-tpl'); if (tpl) D.settings.tpl = tpl.value;
}

function modalWordlib() {
  openModal('敏感词库',
    '<div class="field" style="margin:0"><label>词条（每行一个，当前 ' + D.words.split('\n').length + ' 个）</label>' +
    '<textarea class="textarea" id="wl-text" style="min-height:150px;font-family:var(--mono);font-size:12.5px">' + esc(D.words) + '</textarea>' +
    '<div class="hint">发布或提交作品时命中任一词条，内容将直接进入人工审核队列。</div></div>',
    '<button class="btn btn-ghost" data-action="modal-close">取消</button>' +
    '<button class="btn btn-primary" data-action="wl-save">' + svg('check') + ' 保存词库</button>');
}

window.PAGE = {
  title: '系统设置',
  beforeTab: collectSettings,
  sub: function () {
    return '配置平台的运行规则';
  },
  html: function () {
    var t = state.settingsTab;
    var panes = '';
    if (t === 'basic') {
      panes = '<div class="card set-card">' +
      '<div class="set-row"><div class="sl"><div class="t">站点名称</div><div class="d">显示在页面标题与通知签名中</div></div><div class="sr"><input class="input" id="s-name" value="' + esc(D.settings.name) + '"></div></div>' +
      '<div class="set-row"><div class="sl"><div class="t">站点标语</div><div class="d">首页 Hero 与分享卡片展示</div></div><div class="sr"><input class="input" id="s-slogan" value="' + esc(D.settings.slogan) + '"></div></div>' +
      '<div class="set-row"><div class="sl"><div class="t">支持邮箱</div><div class="d">用户反馈与申诉的接收地址</div></div><div class="sr"><input class="input" id="s-email" value="' + esc(D.settings.email) + '"></div></div>' +
      '<div class="set-row"><div class="sl"><div class="t">维护模式</div><div class="d">开启后访问者将看到维护提示，管理后台不受影响</div></div><div class="sr"><label class="switch"><input type="checkbox" id="s-maint"' + (D.settings.maint ? ' checked' : '') + '><span class="track"></span><span class="thumb"></span></label></div></div>' +
      '</div>';
    } else if (t === 'rules') {
      var conds = ['作品完整度 ≥ 90%', '作者信用分 ≥ 95', '完整度 ≥ 90% 且 信用分 ≥ 80', '全部人工审核'];
      panes = '<div class="card set-card">' +
      '<div class="set-row"><div class="sl"><div class="t">自动通过审核</div><div class="d">符合条件的作品跳过人工审核直接发布</div></div><div class="sr"><label class="switch"><input type="checkbox" id="s-auto"' + (D.settings.auto ? ' checked' : '') + '><span class="track"></span><span class="thumb"></span></label></div></div>' +
      '<div class="set-row"><div class="sl"><div class="t">自动通过条件</div><div class="d">满足条件即自动通过</div></div><div class="sr"><select class="select" id="s-cond">' +
      conds.map(function (c) { return '<option' + (D.settings.cond === c ? ' selected' : '') + '>' + c + '</option>'; }).join('') +
      '</select></div></div>' +
      '<div class="set-row"><div class="sl"><div class="t">每日自动审核上限</div><div class="d">超出部分进入人工队列</div></div><div class="sr"><input class="input" type="number" id="s-cap" value="' + D.settings.cap + '" min="0" style="width:120px"></div></div>' +
      '<div class="set-row"><div class="sl"><div class="t">敏感词库</div><div class="d">命中即进入人工队列，当前 ' + D.words.split('\n').length + ' 个词条</div></div><div class="sr"><button class="btn btn-ghost" data-action="wordlib">' + svg('file', 13) + ' 管理词库</button></div></div>' +
      '</div>';
    } else {
      panes = '<div class="card set-card">' +
      '<div class="set-row"><div class="sl"><div class="t">邮件通知</div><div class="d">审核结果、账单与公告推送</div></div><div class="sr"><label class="switch"><input type="checkbox" id="s-nt-mail"' + (D.settings.ntMail ? ' checked' : '') + '><span class="track"></span><span class="thumb"></span></label></div></div>' +
      '<div class="set-row"><div class="sl"><div class="t">站内信</div><div class="d">平台内消息中心推送</div></div><div class="sr"><label class="switch"><input type="checkbox" id="s-nt-in"' + (D.settings.ntIn ? ' checked' : '') + '><span class="track"></span><span class="thumb"></span></label></div></div>' +
      '<div class="set-row"><div class="sl"><div class="t">短信通知</div><div class="d">仅用于安全相关事件（登录异常、退款）</div></div><div class="sr"><label class="switch"><input type="checkbox" id="s-nt-sms"' + (D.settings.ntSms ? ' checked' : '') + '><span class="track"></span><span class="thumb"></span></label></div></div>' +
      '<div class="set-row stacked"><div class="sl"><div class="t">审核通过通知模板</div><div class="d">支持变量：{name} 作者 · {title} 作品名</div></div><textarea class="textarea" id="s-tpl">' + esc(D.settings.tpl) + '</textarea></div>' +
      '</div>';
    }
    return '<div class="toolbar">' +
      (function () {
        var opts = [['basic', '基础'], ['rules', '审核规则'], ['notify', '通知']];
        return '<div class="seg">' + opts.map(function (o) {
          return '<button data-seg="settings" data-key="tab" data-val="' + o[0] + '" class="' + (t === o[0] ? 'on' : '') + '">' + o[1] + '</button>';
        }).join('') + '</div>';
      })() +
      '<div class="push"><span class="cell-sub">更改保存后对全站即时生效</span></div>' +
      '</div>' +
      panes +
      '<div class="card set-card danger-zone">' +
      '<div class="card-head" style="padding:14px 0 0;border:none"><h3 style="font-size:13px;color:var(--red)">危险操作</h3></div>' +
      '<div class="set-row"><div class="sl"><div class="t">重置演示数据</div><div class="d">将所有列表恢复到初始演示状态（含跨页数据），不可恢复</div></div><div class="sr"><button class="btn btn-danger" data-action="reset-data">' + svg('refresh', 13) + ' 重置演示数据</button></div></div>' +
      '<div class="set-row"><div class="sl"><div class="t">退出登录</div><div class="d">退出当前管理员会话</div></div><div class="sr"><button class="btn btn-ghost" data-action="logout">' + svg('alert', 13) + ' 退出登录</button></div></div>' +
      '</div>' +
      '<div class="toolbar" style="margin:18px 0 0">' +
      '<button class="btn btn-primary" data-action="settings-save">' + svg('check') + ' 保存更改</button>' +
      '<button class="btn btn-ghost" data-action="settings-cancel">放弃更改</button>' +
      '</div>';
  },
  actions: {
    'settings-save': function () {
      collectSettings();
      if (!D.settings.name) { toast('站点名称不能为空', 'danger'); return; }
      B.saveAfter('保存系统设置', '基础 / 审核 / 通知', '系统');
      B.saveData();
      SNAPSHOT = JSON.stringify(D.settings);
      toast('设置已保存，已对全站生效');
    },
    'settings-cancel': function () {
      D.settings = JSON.parse(SNAPSHOT);
      B.saveData();
      rerenderFull();
      toast('已放弃未保存的更改', 'info');
    },
    'wordlib': function () { modalWordlib(); },
    'wl-save': function () {
      var ta = $('#wl-text');
      if (!ta) return;
      D.words = ta.value.split('\n').map(function (s) { return s.trim(); }).filter(Boolean).join('\n');
      B.saveAfter('更新敏感词库', D.words.split('\n').length + ' 个词条', '系统');
      B.saveData();
      closeModal();
      toast('词库已保存，当前 ' + D.words.split('\n').length + ' 个词条');
      rerenderFull();
    },
    'reset-data': function () {
      confirmBox('重置所有演示数据？<br>所有页面的列表、日志都会恢复到初始状态。', '重置演示数据', 'btn-danger', function () {
        B.resetData();
        closeModal();
        toast('演示数据已重置，正在重新加载…', 'info');
        setTimeout(function () { location.reload(); }, 500);
      });
    }
  }
};
})();
