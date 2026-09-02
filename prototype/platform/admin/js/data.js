/* ══════════════════════════════════════════════
   bblbb 管理后台 · 数据层（所有页面共享）
   数据保存在 localStorage，跨页面保持连续
   ══════════════════════════════════════════════ */
(function () {
'use strict';

/* ── 图标 ── */
var ICONS = {
  check: '<polyline points="20 6 9 17 4 12"/>',
  x: '<line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>',
  plus: '<line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>',
  trash: '<polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>',
  pen: '<path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z"/>',
  eye: '<path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/>',
  lock: '<rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>',
  unlock: '<rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 9.9-1"/>',
  coins: '<circle cx="8" cy="8" r="6"/><path d="M18.09 10.37A6 6 0 1 1 10.34 18"/><path d="M7 6h1v4"/>',
  download: '<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/>',
  alert: '<path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/>',
  arrow: '<line x1="5" y1="12" x2="19" y2="12"/><polyline points="12 5 19 12 12 19"/>',
  search: '<circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>',
  zap: '<polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/>',
  clock: '<circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/>',
  star: '<polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/>',
  send: '<line x1="22" y1="2" x2="11" y2="13"/><polygon points="22 2 15 22 11 13 2 9 22 2"/>',
  box: '<path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/>',
  refresh: '<polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>',
  megaphone: '<path d="M3 11l18-7v18l-18-7v-4z"/><path d="M7 14v5a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2v-3"/>',
  file: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/>'
};
function svg(name, size) {
  size = size || 14;
  return '<svg width="' + size + '" height="' + size + '" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">' + ICONS[name] + '</svg>';
}

/* ── 小工具 ── */
function $(s, r) { return (r || document).querySelector(s); }
function $$(s, r) { return Array.prototype.slice.call((r || document).querySelectorAll(s)); }
function esc(s) { return String(s == null ? '' : s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;'); }
function pad2(n) { return (n < 10 ? '0' : '') + n; }
function fmtToday() {
  var d = new Date();
  var wk = ['周日', '周一', '周二', '周三', '周四', '周五', '周六'][d.getDay()];
  return d.getFullYear() + '-' + pad2(d.getMonth() + 1) + '-' + pad2(d.getDate()) + ' ' + wk;
}
function grad(g) { return 'background:linear-gradient(135deg,' + g[0] + ',' + g[1] + ')'; }
function countBy(list, key, val) { return list.filter(function (i) { return i[key] === val; }).length; }
function pendingCount() { return D.reviews.filter(function (r) { return r.status === 'pending'; }).length; }

/* ── 状态字典 ── */
var USER_STATUS = { verified: ['已认证', 'tag-green'], frozen: ['已冻结', 'tag-red'], pending: ['待验证', 'tag-amber'] };
var PROJ_STATUS = { published: ['已发布', 'tag-green'], draft: ['草稿', 'tag-dim'], offline: ['已下线', 'tag-amber'] };
var REV_STATUS = { pending: ['待审核', 'tag-amber'], approved: ['已通过', 'tag-green'], rejected: ['已驳回', 'tag-red'] };
var ANN_STATUS = { published: ['已发布', 'tag-green'], scheduled: ['定时发布', 'tag-purple'], draft: ['草稿', 'tag-dim'] };
var ORD_STATUS = { paid: ['已支付', 'tag-green'], processing: ['处理中', 'tag-amber'], refunding: ['退款中', 'tag-amber'], refunded: ['已退款', 'tag-red'] };
var LOG_CAT_COLOR = { '审核': 'tag-purple', '用户': 'tag-green', '系统': 'tag-dim', '账单': 'tag-amber', '公告': 'tag-amber', '项目': 'tag-purple', '操作': 'tag-dim' };
function tag(map, k) { return '<span class="tag ' + map[k][1] + '">' + map[k][0] + '</span>'; }

/* ── 演示数据 ── */
function seedData() {
  return {
    users: [
      { id: 'U-2301', name: '林晚', handle: 'linwan', email: 'lin.wan@example.com', hue: 262, points: 12800, status: 'verified', plan: 'Pro', joined: '2025-11-02', active: '3 分钟前', projects: 12 },
      { id: 'U-2188', name: '陈屿', handle: 'chenyu', email: 'chen.yu@example.com', hue: 158, points: 8600, status: 'verified', plan: 'Studio', joined: '2025-09-14', active: '12 分钟前', projects: 23 },
      { id: 'U-2450', name: '江晚明', handle: 'jiangwm', email: 'jiang.wm@example.com', hue: 200, points: 15600, status: 'verified', plan: 'Studio', joined: '2025-08-30', active: '12 分钟前', projects: 31 },
      { id: 'U-2277', name: '周慕', handle: 'zhoumu', email: 'zhou.mu@example.com', hue: 32, points: 5200, status: 'verified', plan: 'Pro', joined: '2025-12-20', active: '2 小时前', projects: 9 },
      { id: 'U-2315', name: '苏黎', handle: 'suli', email: 'su.li@example.com', hue: 330, points: 2400, status: 'verified', plan: 'Free', joined: '2026-01-08', active: '1 小时前', projects: 5 },
      { id: 'U-2512', name: '顾一顾', handle: 'yigu', email: 'yigu.gu@example.com', hue: 210, points: 4100, status: 'verified', plan: 'Pro', joined: '2026-02-17', active: '5 天前', projects: 7 },
      { id: 'U-2601', name: '白鹿', handle: 'bailu', email: 'bai.lu@example.com', hue: 356, points: 980, status: 'frozen', plan: 'Free', joined: '2026-03-11', active: '30 天前', projects: 3 },
      { id: 'U-2688', name: '何见', handle: 'hejian', email: 'he.jian@example.com', hue: 48, points: 300, status: 'pending', plan: 'Free', joined: '2026-07-25', active: '今天', projects: 1 }
    ],
    projects: [
      { id: 'P-0921', name: '像素农场小游戏', owner: '江晚明', status: 'published', views: '45.2k', likes: '2.3k', updated: '12 分钟前', grad: ['#7C3AED', '#0EA5E9'], desc: '低多边形风格的像素农场，支持多人协作种植。' },
      { id: 'P-0918', name: '星图导航页', owner: '林晚', status: 'published', views: '12.4k', likes: '892', updated: '2 小时前', grad: ['#5B21B6', '#7C3AED'], desc: '以星图为灵感的交互式导航页。' },
      { id: 'P-0902', name: '城市呼吸·动态海报', owner: '陈屿', status: 'published', views: '8.1k', likes: '566', updated: '5 小时前', grad: ['#00E5A0', '#7C3AED'], desc: '城市主题动态海报系列，Lottie 驱动。' },
      { id: 'P-0897', name: '深夜电台主视觉', owner: '江晚明', status: 'published', views: '22.7k', likes: '1.2k', updated: '昨天', grad: ['#0F766E', '#00E5A0'], desc: '深夜电台栏目主视觉与配套物料。' },
      { id: 'P-0935', name: '手账模板包 V3', owner: '苏黎', status: 'draft', views: '340', likes: '21', updated: '1 小时前', grad: ['#F472B6', '#7C3AED'], desc: '第 3 版手账模板，新增周计划页。' },
      { id: 'P-0940', name: '节气日历插画集', owner: '林晚', status: 'draft', views: '128', likes: '12', updated: '今天', grad: ['#F59E0B', '#F43F5E'], desc: '二十四节气手绘插画日历，进行中。' },
      { id: 'P-0881', name: '品牌焕新提案', owner: '周慕', status: 'offline', views: '1.9k', likes: '88', updated: '3 天前', grad: ['#334155', '#7C3AED'], desc: '客户品牌焕新提案，等待客户反馈。' },
      { id: 'P-0864', name: '咖啡角线下导视', owner: '顾一顾', status: 'offline', views: '760', likes: '45', updated: '上周', grad: ['#78350F', '#FBBF24'], desc: '线下咖啡角导视系统，已交付归档。' }
    ],
    reviews: [
      { id: 'R-301', title: '星轨·交互式落地页', author: '林晚', type: '页面', time: '8 分钟前', status: 'pending', grad: ['#1E1B4B', '#7C3AED'] },
      { id: 'R-302', title: '夏日限定主视觉', author: '苏黎', type: '海报', time: '26 分钟前', status: 'pending', grad: ['#9D174D', '#FB7185'] },
      { id: 'R-303', title: '品牌短片分镜', author: '陈屿', type: '动画', time: '1 小时前', status: 'pending', grad: ['#064E3B', '#00E5A0'] },
      { id: 'R-304', title: '独立音乐人主页', author: '周慕', type: '页面', time: '3 小时前', status: 'pending', grad: ['#1E3A8A', '#60A5FA'] },
      { id: 'R-305', title: '城市地图贴纸包', author: '顾一顾', type: '素材', time: '5 小时前', status: 'pending', grad: ['#78350F', '#FBBF24'] },
      { id: 'R-306', title: '动态字体演示', author: '江晚明', type: '动画', time: '昨天', status: 'pending', grad: ['#4A044E', '#F472B6'] },
      { id: 'R-298', title: '像素农场小游戏', author: '江晚明', type: '页面', time: '2 天前', status: 'approved', grad: ['#7C3AED', '#0EA5E9'] },
      { id: 'R-296', title: '节气日历插画集', author: '林晚', type: '素材', time: '2 天前', status: 'approved', grad: ['#F59E0B', '#F43F5E'] },
      { id: 'R-291', title: '城市呼吸·动态海报', author: '陈屿', type: '海报', time: '4 天前', status: 'approved', grad: ['#00E5A0', '#7C3AED'] },
      { id: 'R-289', title: '未授权字体商用', author: '何见', type: '素材', time: '6 天前', status: 'rejected', reason: '素材含未授权商用字体', grad: ['#334155', '#64748B'] },
      { id: 'R-284', title: '低分辨率活动海报', author: '苏黎', type: '海报', time: '1 周前', status: 'rejected', reason: '清晰度不足，建议 2x 导出', grad: ['#334155', '#64748B'] }
    ],
    anns: [
      { id: 'A-109', title: '系统维护通知：7 月 28 日 02:00–04:00', type: '系统', status: 'published', time: '2026-07-24 18:00', reads: '5,210' },
      { id: 'A-108', title: '「夏日创作季」活动开始啦', type: '活动', status: 'published', time: '2026-07-20 10:00', reads: '8,432' },
      { id: 'A-110', title: '新功能：协作空间支持语音批注', type: '更新', status: 'scheduled', time: '2026-08-02 09:00', reads: '—' },
      { id: 'A-111', title: '关于积分兑换规则调整', type: '系统', status: 'draft', time: '未发布', reads: '—' }
    ],
    orders: [
      { id: 'O-1102', user: '陈屿', item: 'Studio 年卡', amount: '¥2,999', status: 'paid', time: '2026-07-25 08:56', pay: '支付宝' },
      { id: 'O-1101', user: '林晚', item: '积分 10,000', amount: '¥299', status: 'paid', time: '2026-07-30 11:08', pay: '微信支付' },
      { id: 'O-1100', user: '周慕', item: 'Pro 月卡', amount: '¥59', status: 'paid', time: '2026-07-29 20:45', pay: '支付宝' },
      { id: 'O-1099', user: '苏黎', item: '积分 2,000', amount: '¥59', status: 'processing', time: '2026-07-29 16:32', pay: '微信支付' },
      { id: 'O-1098', user: '江晚明', item: 'Studio 月卡', amount: '¥299', status: 'paid', time: '2026-07-28 09:12', pay: '支付宝' },
      { id: 'O-1097', user: '顾一顾', item: 'Pro 月卡', amount: '¥59', status: 'refunding', time: '2026-07-27 19:40', pay: '微信支付' },
      { id: 'O-1096', user: '白鹿', item: '积分 1,000', amount: '¥29', status: 'refunded', time: '2026-07-26 13:05', pay: '支付宝' },
      { id: 'O-1095', user: '陈屿', item: 'Pro 年卡', amount: '¥1,999', status: 'paid', time: '2026-07-24 14:21', pay: '支付宝' }
    ],
    logs: [
      { t: '07-30 15:02', cat: '审核', actor: '陈默', action: '审核通过', target: '作品「星轨·交互式落地页」' },
      { t: '07-30 14:47', cat: '用户', actor: '陈默', action: '冻结账号', target: '白鹿 (U-2601)' },
      { t: '07-30 04:00', cat: '系统', actor: '系统', action: '每日备份', target: '完成 · 2.3 GB' },
      { t: '07-28 10:11', cat: '账单', actor: '李禾', action: '导出账单', target: '2026-07 月度账单' },
      { t: '07-28 09:47', cat: '审核', actor: '李禾', action: '审核驳回', target: '「未授权字体商用」' },
      { t: '07-27 17:20', cat: '用户', actor: '陈默', action: '调整积分', target: '苏黎 +500' },
      { t: '07-26 22:15', cat: '系统', actor: '系统', action: '版本更新', target: 'v2.8.1 → v2.9.0' },
      { t: '07-26 11:02', cat: '公告', actor: '李禾', action: '撤回公告', target: '「关于积分兑换规则调整」' },
      { t: '07-25 16:30', cat: '项目', actor: '陈默', action: '创建项目', target: '「品牌焕新提案」' },
      { t: '07-25 08:56', cat: '账单', actor: '系统', action: '订单支付', target: 'O-1102 · ¥2,999' }
    ],
    words: '低清\n水印\n未授权\n竞品名\n微信号\n联系方式',
    settings: {
      name: 'bblbb', slogan: '创造无界', email: 'support@bblbb.com', maint: false,
      auto: true, cond: '完整度 ≥ 90% 且 信用分 ≥ 80', cap: 200,
      ntMail: true, ntIn: true, ntSms: false,
      tpl: '你好 {name}，你的作品「{title}」已通过审核并成功发布，去分享吧！'
    }
  };
}

/* ── 存储：跨页面共享数据 ── */
var STORE_KEY = 'bblbb-admin-data';
function storeGet(key) { try { return localStorage.getItem(STORE_KEY + ':' + key); } catch (e) { return null; } }
function storeSet(key, val) { try { localStorage.setItem(STORE_KEY + ':' + key, val); } catch (e) {} }
function storeData() {
  try {
    var raw = localStorage.getItem(STORE_KEY);
    if (raw) {
      var d = JSON.parse(raw);
      if (d && d.users && d.reviews) {
        if (!d.settings) d.settings = seedData().settings;
        return d;
      }
    }
  } catch (e) {}
  return null;
}
function saveData() { try { localStorage.setItem(STORE_KEY, JSON.stringify(D)); } catch (e) {} }
function resetData() { try { localStorage.removeItem(STORE_KEY); } catch (e) {} }

/* ── 全局状态 ── */
var D = storeData() || seedData();
var state = {
  f: {
    users: { q: '', status: 'all', plan: 'all' },
    projects: { q: '', status: 'all' },
    reviews: { status: 'pending' },
    orders: { q: '', status: 'all' },
    logs: { q: '', cat: 'all' },
    anns: { status: 'all' }
  },
  sel: { users: {}, projects: {}, reviews: {} },
  settingsTab: 'basic',
  _onYes: null,
  _annId: null
};
function saveAfter(action, target, cat) {
  D.logs.unshift({ t: '刚刚', cat: cat || '操作', actor: '陈默', action: action, target: target });
  if (D.logs.length > 40) D.logs.pop();
  saveData();
}

/* 暴露给 common.js 与各页面 */
window.B = {
  D: D, state: state, svg: svg, esc: esc, fmtToday: fmtToday, pad2: pad2,
  grad: grad, countBy: countBy, pendingCount: pendingCount,
  tag: tag, seedData: seedData, saveData: saveData, resetData: resetData, saveAfter: saveAfter,
  storeGet: storeGet, storeSet: storeSet,
  USER_STATUS: USER_STATUS, PROJ_STATUS: PROJ_STATUS, REV_STATUS: REV_STATUS,
  ANN_STATUS: ANN_STATUS, ORD_STATUS: ORD_STATUS, LOG_CAT_COLOR: LOG_CAT_COLOR,
  $: $, $$: $$
};
})();
