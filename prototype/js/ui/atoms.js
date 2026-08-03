// ============================================================
// BBLBB UI — Atoms: 无状态的原子组件（纯渲染函数，数据 → HTML）
// 不持有状态、不引用 Store；所有动态值必须过 escapeHtml()
// ============================================================
window.Atoms = (function () {
  'use strict';

  // --- HTML escaping（两条硬规则之一：所有变量插值必须转义） ---
  const ESCAPE_MAP = {
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'
  };
  function escapeHtml(text) {
    if (text === null || text === undefined) return '';
    return String(text).replace(/[&<>"']/g, (c) => ESCAPE_MAP[c]);
  }

  // --- Icon（本地内联 SVG，无 CDN） ---
  function icon(name, size = 16, extra = '') {
    const body = (window.Icons && window.Icons[name]) || '';
    return `<svg class="icon icon-${escapeHtml(name)}" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false" ${extra || ''}>${body}</svg>`;
  }

  // --- Avatar（首字符 + 用户名哈希色板，渐变底，无图片请求） ---
  const AVATAR_PALETTE = [
    ['#0969DA', '#54AEFF'], ['#8250DF', '#B083F0'], ['#1A7F37', '#4AC26B'],
    ['#BF8700', '#E3B341'], ['#CF222E', '#FF8182'], ['#0E8A16', '#57AB5A'],
    ['#DA3633', '#F778BA'], ['#8250DF', '#8C959F']
  ];
  function avatar(name, size = 'md') {
    const initial = (name && name.charAt(0)) || '?';
    const idx = Math.abs(String(name || '?').split('').reduce((a, c) => a + c.charCodeAt(0), 0)) % AVATAR_PALETTE.length;
    const [c1, c2] = AVATAR_PALETTE[idx];
    const px = { xs: 20, sm: 24, md: 32, lg: 40, xl: 64, '2xl': 96 }[size] || 32;
    return `<span class="avatar avatar-${escapeHtml(size)}" title="${escapeHtml(name)}" role="img" aria-label="${escapeHtml(name)}"
      style="width:${px}px;height:${px}px;font-size:${Math.round(px * 0.42)}px;background:linear-gradient(135deg,${c1},${c2});color:#fff;">${escapeHtml(initial)}</span>`;
  }

  // --- Button（六态：default/hover/active/focus/disabled/loading） ---
  function button({ text = '', variant = 'primary', size = 'md', icon: iconName = '', iconOnly = false, onClick = '', href = '', disabled = false, id = '', extraClass = '' }) {
    const classes = ['btn', 'btn-' + variant, 'btn-' + size, iconOnly ? 'btn-icon' : '', extraClass].filter(Boolean).join(' ');
    const iconHtml = iconName ? icon(iconName, size === 'sm' ? 14 : 16) : '';
    const inner = `${iconHtml}${iconHtml && text ? '<span>' + escapeHtml(text) + '</span>' : escapeHtml(text)}`;
    const common = `class="${classes}"${disabled ? ' disabled' : ''}${id ? ' id="' + escapeHtml(id) + '"' : ''}`;
    if (href) {
      return `<a href="${escapeHtml(href)}" ${common}${onClick ? ` onclick="${escapeHtml(onClick)}"` : ''}>${inner}</a>`;
    }
    return `<button type="button" ${common}${onClick ? ` onclick="${escapeHtml(onClick)}"` : ''}>${inner}</button>`;
  }

  // --- Tag（方角小标签） ---
  function tag(name, count = null, href = null) {
    const label = `${escapeHtml(name)}${count !== null ? `<span class="tag-count">${escapeHtml(count)}</span>` : ''}`;
    if (href) return `<a href="${escapeHtml(href)}" class="tag">${label}</a>`;
    return `<span class="tag">${label}</span>`;
  }

  // --- Badge（通用小徽标，type 控制语义色） ---
  function badge(text, type = 'level') {
    return `<span class="badge badge-${escapeHtml(type)}">${escapeHtml(text)}</span>`;
  }

  function levelBadge(level) {
    return `<span class="badge badge-level">LV.${escapeHtml(level)}</span>`;
  }

  function roleBadge(roles) {
    if (!roles) return '';
    if (roles.includes('admin')) return `<span class="badge badge-role-admin">管理员</span>`;
    if (roles.includes('moderator')) return `<span class="badge badge-role-mod">版主</span>`;
    return '';
  }

  const STATUS_LABELS = {
    published: '已发布', pending: '待审核', rejected: '已驳回', draft: '草稿',
    locked: '已锁定', processing: '处理中', resolved: '已处理', open: '待处理'
  };
  const STATUS_TYPES = {
    published: 'success', pending: 'warning', rejected: 'danger', draft: 'neutral',
    locked: 'neutral', processing: 'warning', resolved: 'success', open: 'danger'
  };
  function statusBadge(status) {
    const t = STATUS_TYPES[status] || 'neutral';
    return `<span class="badge badge-${t}">${escapeHtml(STATUS_LABELS[status] || status)}</span>`;
  }

  const PRIORITY_LABELS = { high: '高', medium: '中', low: '低' };
  function priorityBadge(priority) {
    return `<span class="badge badge-priority-${escapeHtml(priority)}">${escapeHtml(PRIORITY_LABELS[priority] || priority)}</span>`;
  }

  // --- Category Badge（彩色方块 + 板块名） ---
  function categoryBadge(slug, variant) {
    const board = window.MockData && window.MockData.getBoard(slug);
    const color = (board && board.color) || '#0969DA';
    const cls = 'category-badge' + (variant ? ' is-' + variant : '');
    return `<a href="#/boards/${escapeHtml(slug)}" class="${cls}" style="--cat-color:${escapeHtml(color)};">
      <span class="category-badge-square"></span><span>${escapeHtml(board ? board.name : slug)}</span></a>`;
  }

  // --- 数字紧凑格式（1234 → 1.2k） ---
  function formatCount(n) {
    if (typeof n !== 'number') return escapeHtml(n);
    if (n >= 100000) return (n / 1000).toFixed(0) + 'k';
    if (n >= 1000) {
      const v = n / 1000;
      return (v >= 10 ? Math.round(v) : v.toFixed(1).replace(/\.0$/, '')) + 'k';
    }
    return String(n);
  }

  // --- 等级进度条 ---
  function levelProgress(exp, expNext, level) {
    const pct = expNext > 0 ? Math.min(100, Math.round((exp / expNext) * 100)) : 0;
    return `
      <div class="level-progress" role="progressbar" aria-valuenow="${pct}" aria-valuemin="0" aria-valuemax="100">
        <div class="level-progress-head">
          <span class="level-progress-label">LV.${escapeHtml(level)} · ${escapeHtml(formatCount(exp))} / ${escapeHtml(formatCount(expNext))} 经验</span>
        </div>
        <div class="level-progress-track"><div class="level-progress-fill" style="width:${pct}%"></div></div>
      </div>`;
  }

  // --- Switch（开关） ---
  function switchEl(on, onClick = '') {
    return `<button type="button" role="switch" aria-checked="${on ? 'true' : 'false'}" class="switch ${on ? 'is-on' : ''}"${onClick ? ` data-action="${escapeHtml(onClick)}"` : ''}><span class="switch-knob"></span></button>`;
  }

  // --- Skeleton（骨架占位） ---
  function skeleton(width = '100%', height = '16px') {
    return `<span class="skeleton" style="width:${escapeHtml(width)};height:${escapeHtml(height)};"></span>`;
  }

  // --- Empty State ---
  function emptyState({ icon: iconName = 'inbox', title = '暂无内容', desc = '' }) {
    return `
      <div class="empty-state">
        <div class="empty-state-icon">${icon(iconName, 40)}</div>
        <div class="empty-state-title">${escapeHtml(title)}</div>
        ${desc ? `<div class="empty-state-desc">${escapeHtml(desc)}</div>` : ''}
      </div>`;
  }

  return {
    escapeHtml, icon, avatar, button, tag, badge, levelBadge, roleBadge,
    statusBadge, priorityBadge, categoryBadge, formatCount,
    levelProgress, switchEl, skeleton, emptyState
  };
})();
