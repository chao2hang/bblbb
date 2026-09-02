/* ============================================================================
   chaos v2 — runtime
   ----------------------------------------------------------------------------
   Responsibilities:
   1. icon injection (inline SVG, lucide-compatible, stroke 1.8)
   2. theme switching (light / dark / system, localStorage "chaos-theme")
   3. app shell injection for prototype pages
      - <header data-forum-nav>  → 顶部公开导航
      - <header data-admin-nav>  → 顶部管理导航（含返回前台）
      - <aside  data-admin-side> → 侧栏导航
   4. interactions
      - Toast.show(level, msg)
      - Confirm.show(opts) (returns Promise)
      - Modal.open(name) / Modal.close(name)
      - Popover.toggle(button)
      - data-toast="level|msg" 一次性 toast 触发
      - data-confirm="msg" 二次确认
      - data-modal="name" 打开模态
   5. mock data anchors — window.Forum.demo
   ============================================================================ */
(function () {
	'use strict';

	/* ------------------------------------------------------------------
	 * Icons — feather/lucide style, stroke 1.8
	 * ------------------------------------------------------------------ */
		var ICONS = {
		'activity': '<path d="M22 12h-4l-3 9L9 3l-3 9H2" />',
		'alert-circle': '<circle cx="12" cy="12" r="10" /> <path d="M12 8v4" /> <path d="M12 16h.01" />',
		'alert-triangle': '<path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3" /> <path d="M12 9v4" /> <path d="M12 17h.01" />',
		'archive': '<rect width="20" height="5" x="2" y="3" rx="1"/><path d="M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8"/><path d="M10 12h4"/>',
		'arrow-down': '<path d="M12 5v14"/><path d="m19 12-7 7-7-7"/>',
		'arrow-down-right': '<path d="m7 7 10 10"/><path d="M17 7v10H7"/>',
		'arrow-left': '<path d="m12 19-7-7 7-7"/><path d="M19 12H5"/>',
		'arrow-right': '<path d="M5 12h14"/><path d="m12 5 7 7-7 7"/>',
		'arrow-up': '<path d="m5 12 7-7 7 7"/><path d="M12 19V5"/>',
		'arrow-up-right': '<path d="M7 7h10v10"/><path d="M7 17 17 7"/>',
		'at': '<circle cx="12" cy="12" r="4"/><path d="M16 8v5a3 3 0 0 0 6 0v-1a10 10 0 1 0-4 8"/>',
		'at-sign': '<circle cx="12" cy="12" r="4" /> <path d="M16 8v5a3 3 0 0 0 6 0v-1a10 10 0 1 0-4 8" />',
		'award': '<path d="m15.477 12.89 1.515 8.526a.5.5 0 0 1-.81.47l-3.58-2.687a1 1 0 0 0-1.197 0l-3.586 2.686a.5.5 0 0 1-.81-.469l1.514-8.526" /> <circle cx="12" cy="8" r="6" />',
		'ban': '<circle cx="12" cy="12" r="10" /> <path d="M4.929 4.929 19.07 19.071" />',
		'bar-chart': '<line x1="12" x2="12" y1="20" y2="10"/><line x1="18" x2="18" y1="20" y2="4"/><line x1="6" x2="6" y1="20" y2="16"/>',
		'bell': '<path d="M10.268 21a2 2 0 0 0 3.464 0" /> <path d="M3.262 15.326A1 1 0 0 0 4 17h16a1 1 0 0 0 .74-1.673C19.41 13.956 18 12.499 18 8A6 6 0 0 0 6 8c0 4.499-1.411 5.956-2.738 7.326" />',
		'bold': '<path d="M14 12a4 4 0 0 0 0-8H6v8"/><path d="M15 20a4 4 0 0 0 0-8H6v8Z"/>',
		'book-open': '<path d="M12 5v16" /> <path d="M20.001 19A2 2 0 0022 17V5a2 2 0 00-1.999-2L16 3.002A5 5 0 0012 5a5 5 0 00-4-2H4a2 2 0 00-2 2v12a2 2 0 001.999 2H8a5 5 0 014 2 5 5 0 014-2z" />',
		'bot': '<path d="M12 8V4H8" /> <rect width="16" height="12" x="4" y="8" rx="2" /> <path d="M2 14h2M20 14h2M15 13v2M9 13v2" />',
		'boxes': '<path d="M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4a2 2 0 0 0 1-1.73Z"/><path d="m3.3 7 8.7 5 8.7-5"/><path d="M12 22V12"/>',
		'cable': '<path d="M17 21v-2a1 1 0 0 1-1-1v-1a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2v1a1 1 0 0 1-1 1"/><path d="M19 15V6.5a1 1 0 0 0-7 0v11a1 1 0 0 1-7 0V9"/><path d="M21 21v-2h-4"/><path d="M3 5h4V3"/><path d="M7 5a1 1 0 0 1 1 1v1a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a1 1 0 0 1 1-1V3"/>',
		'calendar': '<path d="M8 2v4M16 2v4M3 10h18M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z" />',
		'check': '<path d="M20 6 9 17l-5-5" />',
		'check-check': '<path d="M2 12 5 12" /><path d="M2 17l3 3 7-7" /><path d="M14 5l3 3 5-5" />',
		'check-circle': '<path d="M21.801 10A10 10 0 1 1 17 3.335" /> <path d="m9 11 3 3L22 4" />',
		'arrow-left': '<line x1="19" y1="12" x2="5" y2="12"/> <polyline points="12 19 5 12 12 5"/>',
		'edit': '<path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/> <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>',
		'phone': '<path d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72 12.84 12.84 0 0 0 .7 2.81 2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45 12.84 12.84 0 0 0 2.81.7A2 2 0 0 1 22 16.92z"/>',
		'bell-off': '<path d="M13.73 21a2 2 0 0 1-3.46 0"/> <path d="M18.63 13A17.89 17.89 0 0 1 18 8"/> <path d="M6.26 6.26A5.86 5.86 0 0 0 6 8c0 7-3 9-3 9h14"/> <path d="M18 8a6 6 0 0 0-9.33-5"/> <line x1="1" y1="1" x2="23" y2="23"/>',
		'smile': '<circle cx="12" cy="12" r="10"/> <path d="M8 14s1.5 2 4 2 4-2 4-2"/> <line x1="9" y1="9" x2="9.01" y2="9"/> <line x1="15" y1="9" x2="15.01" y2="9"/>',
		'chevron-down': '<path d="m6 9 6 6 6-6"/>',
		'chevron-left': '<path d="m15 18-6-6 6-6" />',
		'chevron-right': '<path d="m9 18 6-6-6-6" />',
		'chevron-up': '<path d="m18 15-6-6-6 6"/>',
		'circle': '<circle cx="12" cy="12" r="10"/>',
		'circle-alert': '<circle cx="12" cy="12" r="10"/><line x1="12" x2="12" y1="8" y2="12"/><line x1="12" x2="12.01" y1="16" y2="16"/>',
		'clock': '<circle cx="12" cy="12" r="10" /> <polyline points="12 6 12 12 16 14" />',
		'code': '<path d="m16 18 6-6-6-6" /> <path d="m8 6-6 6 6 6" />',
		'cog': '<path d="M11 10.27 7 3.34" /> <path d="m11 13.73-4 6.93" /> <path d="M12 22v-2" /> <path d="M12 2v2" /> <path d="M14 12h8" /> <path d="m17 20.66-1-1.73" /> <path d="m17 3.34-1 1.73" /> <path d="M2 12h2" /> <path d="m20.66 17-1.73-1" /> <path d="m20.66 7-1.73 1" /> <path d="m3.34 17 1.73-1" /> <path d="m3.34 7 1.73 1" /> <circle cx="12" cy="12" r="2" /> <circle cx="12" cy="12" r="8" />',
		'coins': '<path d="M13.744 17.736a6 6 0 1 1-7.48-7.48" /> <path d="M15 6h1v4" /> <path d="m6.134 14.768.866-.5 2 3.464" /> <circle cx="16" cy="8" r="6" />',
		'copy': '<rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>',
		'crown': '<path d="m2 4 3 12h14l3-12-6 7-4-9-4 9-6-7Zm3 16h14" />',
		'database': '<ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M3 5V19A9 3 0 0 0 21 19V5"/><path d="M3 12A9 3 0 0 0 21 12"/>',
		'download': '<path d="M12 15V3" /> <path d="m7 10 5 5 5-5" /> <path d="M5 21h14" />',
		'edit-3': '<path d="M13 21h8" /> <path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z" />',
		'external-link': '<path d="M15 3h6v6"/><path d="M10 14 21 3"/><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>',
		'eye': '<path d="M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 0 1 0 .696 10.75 10.75 0 0 1-19.876 0" /> <circle cx="12" cy="12" r="3" />',
		'eye-off': '<path d="M10.733 5.076a10.744 10.744 0 0 1 11.205 6.575 1 1 0 0 1 0 .696 10.747 10.747 0 0 1-1.444 2.49" /> <path d="M14.084 14.158a3 3 0 0 1-4.242-4.242" /> <path d="M17.479 17.499a10.75 10.75 0 0 1-15.417-5.151 1 1 0 0 1 0-.696 10.75 10.75 0 0 1 4.446-5.143" /> <path d="m2 2 20 20" />',
		'file-text': '<path d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z" /> <path d="M14 2v5a1 1 0 0 0 1 1h5" /> <path d="M10 9H8" /> <path d="M16 13H8" /> <path d="M16 17H8" />',
		'flag': '<path d="M4 22V4a1 1 0 0 1 .4-.8A6 6 0 0 1 8 2c3 0 5 2 7.333 2q2 0 3.067-.8A1 1 0 0 1 20 4v10a1 1 0 0 1-.4.8A6 6 0 0 1 16 16c-3 0-5-2-8-2a6 6 0 0 0-4 1.528" />',
		'folder': '<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.93a2 2 0 0 1-1.66-.9l-.82-1.2A2 2 0 0 0 7.93 1H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>',
		'gauge': '<path d="m12 14 4-4" /> <path d="M3.34 19a10 10 0 1 1 17.32 0" />',
		'git-branch': '<path d="M15 6a9 9 0 0 0-9 9V3" /> <circle cx="18" cy="6" r="3" /> <circle cx="6" cy="18" r="3" />',
		'github': '<path d="M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.3 1.15-.3 2.35 0 3.5A5.4 5.4 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4"/><path d="M9 18c-4.51 2-5-2-7-2"/>',
		'globe': '<circle cx="12" cy="12" r="10" /> <path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20" /> <path d="M2 12h20" />',
		'hash': '<line x1="4" x2="20" y1="9" y2="9"/><line x1="4" x2="20" y1="15" y2="15"/><line x1="10" x2="8" y1="3" y2="21"/><line x1="16" x2="14" y1="3" y2="21"/>',
		'heart': '<path d="M2 9.5a5.5 5.5 0 0 1 9.591-3.676.56.56 0 0 0 .818 0A5.49 5.49 0 0 1 22 9.5c0 2.29-1.5 4-3 5.5l-5.492 5.313a2 2 0 0 1-3 .019L5 15c-1.5-1.5-3-3.2-3-5.5" />',
		'help-circle': '<circle cx="12" cy="12" r="10" /> <path d="M12 16v-4" /> <path d="M12 8h.01" />',
		'home': '<path d="m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><polyline points="9 22 9 12 15 12 15 22"/>',
		'image': '<rect width="18" height="18" x="3" y="3" rx="2" ry="2" /> <circle cx="9" cy="9" r="2" /> <path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21" />',
		'import': '<path d="M12 3v12"/><path d="m8 11 4 4 4-4"/><path d="M8 5H4a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-4"/>',
		'inbox': '<polyline points="22 12 16 12 14 15 10 15 8 12 2 12" /> <path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 14.76 4H9.24a2 2 0 0 0-1.79 1.11z" />',
		'info': '<circle cx="12" cy="12" r="10" /> <path d="M12 16v-4" /> <path d="M12 8h.01" />',
		'key': '<path d="m15.5 7.5 2.3 2.3a1 1 0 0 0 1.4 0l2.1-2.1a1 1 0 0 0 0-1.4L19 4" /> <path d="m21 2-9.6 9.6" /> <circle cx="7.5" cy="15.5" r="5.5" />',
		'laptop': '<path d="M18 5a2 2 0 0 1 2 2v8.526a2 2 0 0 0 .212.897l1.068 2.127a1 1 0 0 1-.9 1.45H3.62a1 1 0 0 1-.9-1.45l1.068-2.127A2 2 0 0 0 4 15.526V7a2 2 0 0 1 2-2z" /> <path d="M20.054 15.987H3.946" />',
		'layout-dashboard': '<rect width="7" height="9" x="3" y="3" rx="1" /> <rect width="7" height="5" x="14" y="3" rx="1" /> <rect width="7" height="9" x="14" y="12" rx="1" /> <rect width="7" height="5" x="3" y="16" rx="1" />',
		'link': '<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" /> <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />',
		'list': '<path d="M3 5h.01" /> <path d="M3 12h.01" /> <path d="M3 19h.01" /> <path d="M8 5h13" /> <path d="M8 12h13" /> <path d="M8 19h13" />',
		'list-ordered': '<path d="M11 5h10" /> <path d="M11 12h10" /> <path d="M11 19h10" /> <path d="M4 4h1v5" /> <path d="M4 9h2" /> <path d="M6.5 20H3.4c0-1 2.6-1.925 2.6-3.5a1.5 1.5 0 0 0-2.6-1.02" />',
		'loader-circle': '<path d="M21 12a9 9 0 1 1-6.219-8.56"/>',
		'lock': '<rect width="18" height="11" x="3" y="11" rx="2" ry="2" /> <path d="M7 11V7a5 5 0 0 1 10 0v4" />',
		'log-in': '<path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4" /> <polyline points="10 17 15 12 10 7" /> <line x1="15" x2="3" y1="12" y2="12" />',
		'log-out': '<path d="m16 17 5-5-5-5" /> <path d="M21 12H9" /> <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" />',
		'mail': '<rect width="20" height="16" x="2" y="4" rx="2"/><path d="m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7"/>',
		'map-pin': '<path d="M20 10c0 6-8 12-8 12s-8-6-8-12a8 8 0 0 1 16 0Z"/><circle cx="12" cy="10" r="3"/>',
		'menu': '<path d="M4 5h16" /> <path d="M4 12h16" /> <path d="M4 19h16" />',
		'message': '<path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>',
		'message-circle': '<path d="M2.992 16.342a2 2 0 0 1 .094 1.167l-1.065 3.29a1 1 0 0 0 1.236 1.168l3.413-.998a2 2 0 0 1 1.099.092 10 10 0 1 0-4.777-4.719" />',
		'message-square': '<path d="M22 17a2 2 0 0 1-2 2H6.828a2 2 0 0 0-1.414.586l-2.202 2.202A.71.71 0 0 1 2 21.286V5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2z" />',
		'mic-off': '<path d="M12 19v3" /> <path d="M15 9.34V5a3 3 0 0 0-5.68-1.33" /> <path d="M16.95 16.95A7 7 0 0 1 5 12v-2" /> <path d="M18.89 13.23A7 7 0 0 0 19 12v-2" /> <path d="m2 2 20 20" /> <path d="M9 9v3a3 3 0 0 0 5.12 2.12" />',
		'minus': '<path d="M5 12h14" />',
		'monitor': '<rect width="20" height="14" x="2" y="3" rx="2" /> <line x1="8" x2="16" y1="21" y2="21" /> <line x1="12" x2="12" y1="17" y2="21" />',
		'moon': '<path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9" />',
		'more-horizontal': '<circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/><circle cx="5" cy="12" r="1"/>',
		'more-vertical': '<circle cx="12" cy="12" r="1"/><circle cx="12" cy="5" r="1"/><circle cx="12" cy="19" r="1"/>',
		'network': '<rect x="16" y="16" width="6" height="6" rx="1"/><rect x="2" y="16" width="6" height="6" rx="1"/><rect x="9" y="2" width="6" height="6" rx="1"/><path d="M5 16v-3a1 1 0 0 1 1-1h12a1 1 0 0 1 1 1v3"/><path d="M12 12V8"/>',
		'package': '<path d="M11 21.73a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73z" /> <path d="M12 22V12" /> <polyline points="3.29 7 12 12 20.71 7" /> <path d="m7.5 4.27 9 5.15" />',
		'palette': '<circle cx="13.5" cy="6.5" r=".5" fill="currentColor" /> <circle cx="17.5" cy="10.5" r=".5" fill="currentColor" /> <circle cx="8.5" cy="7.5" r=".5" fill="currentColor" /> <circle cx="6.5" cy="12.5" r=".5" fill="currentColor" /> <path d="M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20c1.1 0 2-.9 2-2 0-.5-.2-.9-.5-1.3-.3-.3-.5-.8-.5-1.2a2 2 0 0 1 2-2h2a5 5 0 0 0 5-5C22 5.8 17.5 2 12 2" />',
		'paperclip': '<path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48" />',
		'pen-line': '<path d="M13 21h8" /> <path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z" />',
		'pencil': '<path d="M12 20h9" /> <path d="M16.376 3.622a1 1 0 0 1 3.002 3.002L7.368 18.635a2 2 0 0 1-.855.506l-2.872.838a.5.5 0 0 1-.62-.62l.838-2.872a2 2 0 0 1 .506-.854z" />',
		'pin': '<path d="M12 17v5" /> <path d="M9 3h6l1 7 3 3H5l3-3z" /> <path d="M8 13h8" />',
		'play': '<polygon points="6 3 20 12 6 21 6 3"/>',
		'plug': '<path d="M12 22v-5"/><path d="M9 7V2"/><path d="M15 7V2"/><path d="M6 13V8h12v5a4 4 0 0 1-4 4h-4a4 4 0 0 1-4-4Z"/>',
		'plus': '<path d="M5 12h14" /> <path d="M12 5v14" />',
		'puzzle': '<path d="M19.4 13.1V19a2 2 0 0 1-2 2h-4.7a2.7 2.7 0 1 0-5.4 0H4a2 2 0 0 1-2-2v-3.3a2.7 2.7 0 1 0 0-5.4V7a2 2 0 0 1 2-2h5.9a2.7 2.7 0 1 1 5.4 0h2.1a2 2 0 0 1 2 2v2.1a2.7 2.7 0 1 1 0 5.4z" />',
		'quote': '<path d="M16 3a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2 1 1 0 0 1 1 1v1a2 2 0 0 1-2 2 1 1 0 0 0-1 1v2a1 1 0 0 0 1 1 6 6 0 0 0 6-6V5a2 2 0 0 0-2-2z" /> <path d="M5 3a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2 1 1 0 0 1 1 1v1a2 2 0 0 1-2 2 1 1 0 0 0-1 1v2a1 1 0 0 0 1 1 6 6 0 0 0 6-6V5a2 2 0 0 0-2-2z" />',
		'radio-tower': '<path d="M4.9 16.1C1 12.2 1 5.8 4.9 1.9"/><path d="M7.8 4.7a6.14 6.14 0 0 0-.8 7.5"/><circle cx="12" cy="9" r="2"/><path d="M16.2 4.8c2 2 2.26 5.11.8 7.47"/><path d="M19.1 1.9a9.96 9.96 0 0 1 0 14.1"/><path d="M9.5 18h5"/><path d="m8 22 4-11 4 11"/>',
		'refresh-cw': '<path d="M3 12a9 9 0 0 1 15.1-6.6L21 8" /> <path d="M21 3v5h-5" /> <path d="M21 12a9 9 0 0 1-15.1 6.6L3 16" /> <path d="M3 21v-5h5" />',
		'reply': '<path d="M20 18v-2a4 4 0 0 0-4-4H4" /> <path d="m9 17-5-5 5-5" />',
		'rotate-ccw': '<path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/>',
		'route': '<circle cx="6" cy="19" r="3"/><path d="M9 19h8.5a3.5 3.5 0 0 0 0-7h-11a3.5 3.5 0 0 1 0-7H15"/><circle cx="18" cy="5" r="3"/>',
		'save': '<path d="M15.2 3a2 2 0 0 1 1.4.6l3.8 3.8a2 2 0 0 1 .6 1.4V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z" /> <path d="M17 21v-8H7v8M7 3v5h8" />',
		'scale': '<path d="M12 3v18" /> <path d="m19 8 3 8a5 5 0 0 1-6 0zV7" /> <path d="M3 7h1a17 17 0 0 0 8-2 17 17 0 0 0 8 2h1" /> <path d="m5 8 3 8a5 5 0 0 1-6 0zV7" /> <path d="M7 21h10" />',
		'scroll-text': '<path d="M15 12h-5" /><path d="M15 8h-5" /><path d="M19 17V5a2 2 0 0 0-2-2H4" /><path d="M8 21h12a2 2 0 0 0 2-2v-1a1 1 0 0 0-1-1H11a1 1 0 0 0-1 1v1a2 2 0 1 1-4 0V5a2 2 0 1 0-4 0v2a1 1 0 0 0 1 1h3" />',
		'search': '<path d="m21 21-4.34-4.34" /> <circle cx="11" cy="11" r="8" />',
		'send': '<path d="M14.536 21.686a.5.5 0 0 0 .937-.024l6.5-19a.496.496 0 0 0-.635-.635l-19 6.5a.5.5 0 0 0-.024.937l7.93 3.18a2 2 0 0 1 1.112 1.11z" /> <path d="m21.854 2.147-10.94 10.939" />',
		'server': '<rect width="20" height="8" x="2" y="2" rx="2" ry="2"/><rect width="20" height="8" x="2" y="14" rx="2" ry="2"/><line x1="6" x2="6.01" y1="6" y2="6"/><line x1="6" x2="6.01" y1="18" y2="18"/>',
		'settings': '<path d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915" /> <circle cx="12" cy="12" r="3" />',
		'share': '<path d="M4 12v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8"/><polyline points="16 6 12 2 8 6"/><line x1="12" x2="12" y1="2" y2="15"/>',
		'share-2': '<circle cx="18" cy="5" r="3" /> <circle cx="6" cy="12" r="3" /> <circle cx="18" cy="19" r="3" /> <line x1="8.59" x2="15.42" y1="13.51" y2="17.49" /> <line x1="15.41" x2="8.59" y1="6.51" y2="10.49" />',
		'shield': '<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z" />',
		'shield-alert': '<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/><path d="M12 8v4"/><path d="M12 16h.01"/>',
		'shield-check': '<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z" /> <path d="m9 12 2 2 4-4" />',
		'shopping-bag': '<path d="M6 2 3 6v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V6l-3-4z" /> <path d="M3 6h18M16 10a4 4 0 0 1-8 0" />',
		'smartphone': '<rect width="14" height="20" x="5" y="2" rx="2" ry="2" /> <path d="M12 18h.01" />',
		'sparkles': '<path d="m12 3-1.5 4.5L6 9l4.5 1.5L12 15l1.5-4.5L18 9l-4.5-1.5L12 3Z" /><path d="m19 15-.8 2.2L16 18l2.2.8L19 21l.8-2.2L22 18l-2.2-.8L19 15Z" />',
		'square': '<rect width="18" height="18" x="3" y="3" rx="2"/>',
		'star': '<polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />',
		'store': '<path d="m2 7 4.41-4.41A2 2 0 0 1 7.83 2h8.34a2 2 0 0 1 1.42.59L22 7" /><path d="M4 12v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8" /><path d="M15 22v-4a2 2 0 0 0-2-2h-2a2 2 0 0 0-2 2v4" /><path d="M2 7h20" /><path d="M22 7v3a2 2 0 0 1-2 2a2.7 2.7 0 0 1-1.59-.63.7.7 0 0 0-.82 0A2.7 2.7 0 0 1 16 12a2.7 2.7 0 0 1-1.59-.63.7.7 0 0 0-.82 0A2.7 2.7 0 0 1 12 12a2.7 2.7 0 0 1-1.59-.63.7.7 0 0 0-.82 0A2.7 2.7 0 0 1 8 12a2.7 2.7 0 0 1-1.59-.63.7.7 0 0 0-.82 0A2.7 2.7 0 0 1 4 12a2 2 0 0 1-2-2V7" />',
		'sun': '<circle cx="12" cy="12" r="4" /> <path d="M12 2v2M12 20v2M4.93 4.93l1.42 1.42M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41" />',
		'table': '<path d="M12 3v18" /> <rect width="18" height="18" x="3" y="3" rx="2" /> <path d="M3 9h18" /> <path d="M3 15h18" />',
		'tablet': '<rect width="16" height="20" x="4" y="2" rx="2" ry="2"/><line x1="12" x2="12.01" y1="18" y2="18"/>',
		'tag': '<path d="M12.586 2.586A2 2 0 0 0 11.172 2H4a2 2 0 0 0-2 2v7.172a2 2 0 0 0 .586 1.414l8.704 8.704a2.426 2.426 0 0 0 3.42 0l6.58-6.58a2.426 2.426 0 0 0 0-3.42z" /> <circle cx="7.5" cy="7.5" r=".5" fill="currentColor" />',
		'thumbs-up': '<path d="M15 5.88 14 10h5.83a2 2 0 0 1 1.92 2.56l-2.33 8A2 2 0 0 1 17.5 22H4a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2h2.76a2 2 0 0 0 1.79-1.11L12 2a3.13 3.13 0 0 1 3 3.88Z" /> <path d="M7 10v12" />',
		'tool': '<path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76Z"/>',
		'trash': '<path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/>',
		'trash-2': '<path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/><line x1="10" x2="10" y1="11" y2="17"/><line x1="14" x2="14" y1="11" y2="17"/>',
		'trending-up': '<polyline points="22 7 13.5 15.5 8.5 10.5 2 17" /> <polyline points="16 7 22 7 22 13" />',
		'triangle-alert': '<path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z"/><path d="M12 9v4"/><path d="M12 17h.01"/>',
		'trophy': '<path d="M10 14.66V17a1 1 0 0 1-1 1 2 2 0 0 0-2 2v2" /> <path d="M14 14.66V17a1 1 0 0 0 1 1 2 2 0 0 1 2 2v2" /> <path d="M17.916 10H19.5A2.5 2.5 0 0 0 22 7.5V5a1 1 0 0 0-1-1h-3" /> <path d="M4 22h16" /> <path d="M6 9a6 6 0 0 0 12 0V3a1 1 0 0 0-1-1H7a1 1 0 0 0-1 1z" /> <path d="M6.084 10H4.5A2.5 2.5 0 0 1 2 7.5V5a1 1 0 0 1 1-1h3" />',
		'twitter': '<path d="M22 4s-.7 2.1-2 3.4c1.6 10-9.4 17.3-18 11.6 2.2.1 4.4-.6 6-2C3 15.5 5 14.1 5 14.1c-1.4-.1-3-.5-4-1.5 0 0 1-1 1-1 0 0 0-2-1-2 0 0 0 0 0 0-1.8 0-3.3.4-4.4 1.5"/>',
		'undo-2': '<path d="M9 14 4 9l5-5"/><path d="M4 9h11a5 5 0 0 1 5 5v2"/>',
		'upload': '<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" x2="12" y1="3" y2="15"/>',
		'user': '<path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2" /> <circle cx="12" cy="7" r="4" />',
		'user-plus': '<path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" /> <circle cx="9" cy="7" r="4" /> <line x1="19" x2="19" y1="8" y2="14" /> <line x1="22" x2="16" y1="11" y2="11" />',
		'users': '<path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" /> <path d="M16 3.128a4 4 0 0 1 0 7.744" /> <path d="M22 21v-2a4 4 0 0 0-3-3.87" /> <circle cx="9" cy="7" r="4" />',
		'video': '<path d="m16 13 5.223 3.482A.5.5 0 0 0 22 16.066V7.934a.5.5 0 0 0-.777-.416L16 11" /> <rect x="2" y="6" width="14" height="12" rx="2" />',
		'wallet': '<path d="M19 7V4a1 1 0 0 0-1-1H5a2 2 0 0 0 0 4h15a1 1 0 0 1 1 1v4h-3a2 2 0 1 0 0 4h3a1 1 0 0 0 1-1v-2a1 1 0 0 0-1-1"/><path d="M3 5v14a2 2 0 0 0 2 2h15a1 1 0 0 0 1-1v-4"/>',
		'wand-2': '<path d="m3 21 9-9" /> <path d="M12 12 9 9" /> <path d="m15 6 1.5 1.5M18 3l.5 2.5L21 6l-2.5.5L18 9l-.5-2.5L15 6l2.5-.5z" /> <path d="m3 3 .5 2.5L6 6l-2.5.5L3 9l-.5-2.5L0 6l2.5-.5z" />',
		'webhook': '<path d="M18 8a6 6 0 0 0-12 0c0 3 2 4 3 6" /> <path d="M6 16a6 6 0 0 0 12 0c0-3-2-4-3-6" /> <circle cx="6" cy="8" r="2" /> <circle cx="18" cy="16" r="2" />',
		'workflow': '<rect width="8" height="8" x="3" y="3" rx="2"/><path d="M7 11v4a2 2 0 0 0 2 2h4"/><rect width="8" height="8" x="13" y="13" rx="2"/>',
		'x': '<path d="M18 6 6 18" /> <path d="m6 6 12 12" />',
		'x-circle': '<circle cx="12" cy="12" r="10" /> <path d="m15 9-6 6" /> <path d="m9 9 6 6" />',
	};

	/* ------------------------------------------------------------------
	 * Icon injection — every [data-icon="name"][data-size="N"] gets SVG
	 * ------------------------------------------------------------------ */
	function injectIcon(el) {
		var name = el.getAttribute('data-icon');
		if (!name || !ICONS[name]) return;
		var svgNS = 'http://www.w3.org/2000/svg';
		var svg = document.createElementNS(svgNS, 'svg');
		// copy attrs from placeholder
		var attrs = ['class', 'aria-hidden', 'data-icon', 'data-size'];
		attrs.forEach(function (a) {
			if (el.hasAttribute(a)) svg.setAttribute(a, el.getAttribute(a));
		});
		svg.setAttribute('viewBox', '0 0 24 24');
		svg.setAttribute('fill', 'none');
		svg.setAttribute('stroke', 'currentColor');
		svg.setAttribute('stroke-width', '1.8');
		svg.setAttribute('stroke-linecap', 'round');
		svg.setAttribute('stroke-linejoin', 'round');
		svg.innerHTML = ICONS[name];
		el.replaceWith(svg);
	}

	/* ------------------------------------------------------------------
	 * Theme switcher — three modes, localStorage "chaos-theme"
	 * ------------------------------------------------------------------ */
	var THEME_KEY = 'chaos-theme';

	function applyTheme(mode) {
		var html = document.documentElement;
		if (mode === 'light' || mode === 'dark') {
			html.setAttribute('data-theme', mode);
		} else {
			html.removeAttribute('data-theme');
		}
		// re-emit event for listeners
		html.dispatchEvent(new CustomEvent('chaos:theme', { detail: { mode: mode } }));
	}

	function readTheme() {
		try { return localStorage.getItem(THEME_KEY) || 'system'; }
		catch (e) { return 'system'; }
	}

	function writeTheme(mode) {
		try { localStorage.setItem(THEME_KEY, mode); } catch (e) {}
		applyTheme(mode);
	}

	function buildThemeSwitcher(host) {
		var current = readTheme();
		var wrap = document.createElement('div');
		wrap.className = 'theme-switcher';
		wrap.setAttribute('role', 'group');
		wrap.setAttribute('aria-label', '主题切换');
		wrap.innerHTML =
			'<button type="button" data-theme-mode="light" title="浅色"><span data-icon="sun" data-size="14"></span></button>' +
			'<button type="button" data-theme-mode="dark" title="深色"><span data-icon="moon" data-size="14"></span></button>' +
			'<button type="button" data-theme-mode="system" title="跟随系统"><span data-icon="monitor" data-size="14"></span></button>';
		host.appendChild(wrap);
		wireThemeSwitcher(wrap);
	}

	function wireThemeSwitcher(wrap) {
		var buttons = wrap.querySelectorAll('button[data-theme-mode]');
		buttons.forEach(function (btn) {
			btn.addEventListener('click', function () {
				writeTheme(btn.getAttribute('data-theme-mode'));
			});
		});
		function syncActive() {
			var mode = readTheme();
			buttons.forEach(function (b) {
				var active = b.getAttribute('data-theme-mode') === mode;
				b.classList.toggle('active', active);
				b.setAttribute('aria-pressed', active ? 'true' : 'false');
			});
		}
		syncActive();
		document.documentElement.addEventListener('chaos:theme', syncActive);
	}

	// 无导航栏的页面（登录/注册/找回密码等）在右下角悬浮主题切换，保证深浅色入口一致
	function mountFloatingThemeSwitcher() {
		var host = document.createElement('div');
		host.className = 'theme-switcher theme-switcher--float';
		host.setAttribute('role', 'group');
		host.setAttribute('aria-label', '主题切换');
		host.innerHTML =
			'<button type="button" data-theme-mode="light" title="浅色"><span data-icon="sun" data-size="14"></span></button>' +
			'<button type="button" data-theme-mode="dark" title="深色"><span data-icon="moon" data-size="14"></span></button>' +
			'<button type="button" data-theme-mode="system" title="跟随系统"><span data-icon="monitor" data-size="14"></span></button>';
		document.body.appendChild(host);
		injectIcons(host);
		wireThemeSwitcher(host);
	}

	function escapeHtml(s) {
		return String(s).replace(/[&<>"']/g, function (c) {
			return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c];
		});
	}

	function roleBadge(r) {
		var roleClass = /管|admin|owner/i.test(r) ? 'role-admin' : 'role-mod';
		return '<span class="role-badge ' + roleClass + '">' + escapeHtml(r) + '</span>';
	}

	/* ------------------------------------------------------------------
	 * User dropdown menu — shared by forum nav and admin nav
	 * ------------------------------------------------------------------ */
	function buildUserMenuPanel(opts) {
		opts = opts || {};
		var p = DEMO.profiles['Chaos'];
		var roleTags = p.roles.map(roleBadge).join('');
		var adminItem = opts.admin
			? ''
			: '<a href="admin.html" class="popover-item"><span data-icon="shield" data-size="14"></span><span>管理后台</span></a>';
		return '' +
			'<div class="popover-panel user-menu" role="menu">' +
				'<div class="user-menu__head">' +
					'<a class="user-menu__profile" href="users-Chaos.html" data-user="Chaos">' +
						'<span class="avatar tone-' + p.tone + '">C</span>' +
						'<div class="user-menu__who">' +
							'<div class="user-menu__name">Chaos</div>' +
							'<div class="user-menu__title">' + p.title + '</div>' +
							'<div class="user-menu__meta">' +
								'<span class="level-badge lv-' + p.level + '">LV.' + p.level + '</span>' +
								'<span class="muted">主题 ' + p.topics + ' · 回复 ' + p.replies + '</span>' +
							'</div>' +
						'</div>' +
					'</a>' +
					(roleTags ? '<div class="user-menu__roles">' + roleTags + '</div>' : '') +
				'</div>' +
				'<div class="popover-sep"></div>' +
				'<a href="users-Chaos.html" class="popover-item" role="menuitem"><span data-icon="users" data-size="14"></span><span>我的主页</span></a>' +
				'<a href="favorites.html" class="popover-item" role="menuitem"><span data-icon="star" data-size="14"></span><span>我的收藏</span></a>' +
				'<a href="messages.html" class="popover-item" role="menuitem"><span data-icon="message-circle" data-size="14"></span><span>私信</span></a>' +
				'<a href="notifications.html" class="popover-item" role="menuitem"><span data-icon="bell" data-size="14"></span><span>消息通知</span></a>' +
				adminItem +
				'<a href="settings.html" class="popover-item" role="menuitem"><span data-icon="settings" data-size="14"></span><span>账号设置</span></a>' +
				'<div class="popover-sep"></div>' +
				'<button type="button" class="popover-item danger" role="menuitem" data-toast="info|退出登录|原型演示，未接入真实登出">' +
					'<span data-icon="log-out" data-size="14"></span><span>退出登录</span>' +
				'</button>' +
			'</div>';
	}

	/* ------------------------------------------------------------------
	 * App shell — public forum nav injection
	 * ------------------------------------------------------------------ */
	function buildForumNav(host) {
		var here = (location.pathname.split('/').pop() || 'index.html').toLowerCase();
		host.innerHTML =
			'<a href="#main" class="skip-link">跳转到主内容</a>' +
			'<div class="forum-nav__inner">' +
				'<a href="index.html" class="forum-nav__brand">' +
					'<span class="forum-nav__brand-mark"><span data-icon="workflow" data-size="14"></span></span>' +
					'<span class="forum-nav__brand-name">BBLBB</span>' +
				'</a>' +
				'<nav class="forum-nav__menu" aria-label="主导航">' +
					navLink('index.html', '首页', 'layout-dashboard', here) +
					navLink('articles.html', '文章', 'file-text', here) +
					navLink('boards.html', '板块', 'workflow', here) +
					navLink('tags.html', '标签', 'hash', here) +
					navLink('users-Chaos-public.html', '用户主页', 'users', here) +
					navLink('achievements.html', '成就', 'award', here) +
					navLink('messages.html', '消息', 'message-circle', here) +
					'<a href="guide.html" class="forum-nav__link forum-nav__link--meta" title="原型设计说明"><span data-icon="info" data-size="14"></span><span>说明</span></a>' +
				'</nav>' +
				'<form class="forum-nav__search" role="search" onsubmit="event.preventDefault();Forum.toast(\'info\',\'搜索演示：未接入真实索引\');">' +
					'<span data-icon="search" data-size="13"></span>' +
					'<input class="input" type="search" placeholder="搜索主题、用户、标签…" aria-label="搜索" />' +
				'</form>' +
				'<div class="forum-nav__user">' +
					'<div class="forum-nav__theme" data-theme-switcher-host title="主题切换"></div>' +
					'<a href="publish.html" class="btn btn--primary btn--sm forum-nav__publish"><span data-icon="plus" data-size="13"></span><span>发布</span></a>' +
					'<a href="notifications.html" class="btn btn--ghost btn--icon btn--sm forum-nav__bell" aria-label="通知（3 条未读）"><span data-icon="bell" data-size="14"></span><span class="forum-nav__bell-badge">3</span></a>' +
					'<div class="popover-anchor">' +
						'<button type="button" class="user-badge user-badge--btn" data-popover data-user-menu="forum" aria-haspopup="menu" aria-expanded="false">' +
							'<span class="avatar sm tone-1">C</span>' +
							'<span class="user-badge__name">Chaos</span>' +
							'<span data-icon="chevron-down" data-size="12"></span>' +
						'</button>' +
						buildUserMenuPanel({ admin: false }) +
					'</div>' +
				'</div>' +
			'</div>';
		// wire theme switcher
		var swHost = host.querySelector('[data-theme-switcher-host]');
		if (swHost) buildThemeSwitcher(swHost);
		// mobile hamburger — duplicated menu at narrow widths
		var burger = document.createElement('button');
		burger.type = 'button';
		burger.className = 'btn btn--ghost btn--icon forum-nav__burger';
		burger.setAttribute('aria-label', '打开菜单');
		burger.innerHTML = '<span data-icon="menu" data-size="18"></span>';
		burger.addEventListener('click', function () {
			host.classList.toggle('forum-nav--mobile-open');
		});
		host.appendChild(burger);
	}

	function navLink(href, label, icon, here) {
		var active = here === href.toLowerCase() ? ' active' : '';
		return '<a class="forum-nav__link' + active + '" href="' + href + '">' +
			'<span data-icon="' + icon + '" data-size="14"></span>' +
			'<span>' + label + '</span>' +
			'</a>';
	}

	/* ------------------------------------------------------------------
	 * Admin app shell
	 * ------------------------------------------------------------------ */
	// 按管理职能分组：概览 → 内容 → 用户 → 运营 → 集成 → 系统
	var ADMIN_NAV_GROUPS = [
		{
			title: '概览',
			items: [
				{ href: 'admin.html', label: '仪表盘', icon: 'layout-dashboard' }
			]
		},
		{
			title: '内容管理',
			items: [
				{ href: 'admin-posts.html', label: '帖子与文章', icon: 'file-text' },
				{ href: 'admin-boards.html', label: '板块', icon: 'workflow' },
				{ href: 'admin-tags.html', label: '标签', icon: 'tag' },
				{ href: 'admin-attachments.html', label: '附件', icon: 'paperclip' }
			]
		},
		{
			title: '用户与权限',
			items: [
				{ href: 'admin-users.html', label: '用户', icon: 'users' },
				{ href: 'admin-roles.html', label: '角色与权限', icon: 'shield' },
				{ href: 'admin-reports.html', label: '举报与审核', icon: 'flag', badge: '12' }
			]
		},
		{
			title: '激励与交易',
			items: [
				{ href: 'admin-achievements.html', label: '成就', icon: 'award' },
				{ href: 'admin-points.html', label: '积分与货币', icon: 'coins' },
				{ href: 'admin-levels.html', label: '等级', icon: 'award' },
				{ href: 'admin-marketplace.html', label: '市场与交易', icon: 'shopping-bag' },
				{ href: 'admin-download-billing.html', label: '下载计费', icon: 'download' }
			]
		},
		{
			title: '扩展与集成',
			items: [
				{ href: 'admin-themes.html', label: '主题', icon: 'palette' },
				{ href: 'admin-plugins.html', label: '插件', icon: 'puzzle' },
				{ href: 'admin-oauth.html', label: 'OAuth 客户端', icon: 'shield' },
				{ href: 'admin-ai.html', label: '大模型', icon: 'sparkles' },
				{ href: 'admin-video.html', label: '视频插件', icon: 'play' }
			]
		},
		{
			title: '数据分析',
			items: [
				{ href: 'admin-bi.html', label: 'BI 看板', icon: 'bar-chart' }
			]
		},
		{
			title: '系统',
			items: [
				{ href: 'admin-storage.html', label: '文件存储', icon: 'server' },
				{ href: 'admin-notifications.html', label: '通知', icon: 'bell' },
				{ href: 'admin-audit.html', label: '审计日志', icon: 'file-text' },
				{ href: 'admin-settings.html', label: '站点设置', icon: 'settings' }
			]
		}
	];

	function buildAdminNav(host) {
		host.innerHTML =
			'<a href="#main" class="skip-link">跳转到主内容</a>' +
			'<div class="forum-nav__inner">' +
				'<a href="index.html" class="forum-nav__brand">' +
					'<span class="forum-nav__brand-mark"><span data-icon="workflow" data-size="14"></span></span>' +
					'<span class="forum-nav__brand-name">BBLBB Admin</span>' +
				'</a>' +
				'<nav class="forum-nav__menu">' +
					'<a class="forum-nav__link" href="index.html"><span data-icon="arrow-left" data-size="14"></span><span>返回前台</span></a>' +
				'</nav>' +
				'<span class="spacer"></span>' +
				'<div class="forum-nav__theme" data-theme-switcher-host title="主题切换"></div>' +
				'<div class="popover-anchor">' +
					'<button type="button" class="user-badge user-badge--btn" data-popover data-user-menu="admin" aria-haspopup="menu" aria-expanded="false">' +
						'<span class="avatar sm tone-1">C</span>' +
						'<span class="user-badge__name">Chaos · 管理员</span>' +
						'<span data-icon="chevron-down" data-size="12"></span>' +
					'</button>' +
					buildUserMenuPanel({ admin: true }) +
				'</div>' +
			'</div>';
		var swHost = host.querySelector('[data-theme-switcher-host]');
		if (swHost) buildThemeSwitcher(swHost);
	}

	/* ------------------------------------------------------------------
	 * Nav progress bar — attaches to every [data-forum-nav] / [data-admin-nav]
	 * Auto-runs on init: 0 → 30 → 70 → 100% then fades out.
	 * Exposes Forum.loading.set(n) for manual control.
	 * ------------------------------------------------------------------ */
	var navProgressTimers = [];
	function attachNavProgress() {
		if (attachNavProgress.__wired) return;
		attachNavProgress.__wired = true;
		document.querySelectorAll('[data-forum-nav], [data-admin-nav]').forEach(function (nav) {
			if (nav.querySelector('.forum-nav__progress')) return;
			var bar = document.createElement('div');
			bar.className = 'forum-nav__progress';
			bar.setAttribute('aria-hidden', 'true');
			nav.appendChild(bar);
		});
	}
	function setNavProgress(n) {
		var pct = Math.max(0, Math.min(100, n));
		document.querySelectorAll('.forum-nav__progress').forEach(function (bar) {
			bar.style.width = pct + '%';
		});
	}
	function startNavProgressAuto() {
		document.querySelectorAll('[data-forum-nav], [data-admin-nav]').forEach(function (nav) {
			nav.classList.add('is-loading');
		});
		var seq = [25, 55, 80, 100];
		seq.forEach(function (val, i) {
			navProgressTimers.push(setTimeout(function () { setNavProgress(val); }, 90 + i * 140));
		});
		navProgressTimers.push(setTimeout(function () {
			document.querySelectorAll('[data-forum-nav], [data-admin-nav]').forEach(function (nav) {
				nav.classList.remove('is-loading');
				nav.classList.add('is-loading-done');
			});
		}, 90 + seq.length * 140 + 200));
		navProgressTimers.push(setTimeout(function () {
			document.querySelectorAll('[data-forum-nav], [data-admin-nav]').forEach(function (nav) {
				nav.classList.remove('is-loading-done');
				setNavProgress(0);
			});
		}, 90 + seq.length * 140 + 1100));
	}
	function stopNavProgressAuto() {
		navProgressTimers.forEach(function (t) { clearTimeout(t); });
		navProgressTimers = [];
		document.querySelectorAll('[data-forum-nav], [data-admin-nav]').forEach(function (nav) {
			nav.classList.remove('is-loading', 'is-loading-done');
		});
		setNavProgress(0);
	}

	function buildAdminSide(host) {
		var here = (location.pathname.split('/').pop() || 'admin.html').toLowerCase();
		function item(it) {
			var active = here === it.href.toLowerCase() ? ' admin-side__item--active' : '';
			var badge = it.badge ? '<span class="admin-side__badge">' + it.badge + '</span>' : '';
			return '<a class="admin-side__item' + active + '" href="' + it.href + '">' +
				'<span data-icon="' + it.icon + '" data-size="14"></span>' +
				'<span>' + it.label + '</span>' +
				badge +
				'</a>';
		}
		var groups = ADMIN_NAV_GROUPS.map(function (g) {
			return '<div class="admin-side__group">' +
				'<div class="admin-side__title">' + g.title + '</div>' +
				g.items.map(item).join('') +
				'</div>';
		}).join('');
		host.innerHTML =
			'<div class="admin-side__inner">' +
				groups +
				'<div class="admin-side__foot">' +
					'<span class="muted">原型演示 · v2</span>' +
				'</div>' +
			'</div>';
	}

	/* ------------------------------------------------------------------
	 * Toast / Confirm / Modal / Popover
	 * ------------------------------------------------------------------ */
	function ensureToastStack() {
		var s = document.querySelector('.toast-stack');
		if (s) return s;
		s = document.createElement('div');
		s.className = 'toast-stack';
		s.setAttribute('role', 'status');
		s.setAttribute('aria-live', 'polite');
		s.setAttribute('aria-atomic', 'true');
		document.body.appendChild(s);
		return s;
	}

	function toast(level, msg, ms) {
		var stack = ensureToastStack();
		if (stack.children.length >= 3) {
			stack.removeChild(stack.firstChild);
		}
		ms = ms || 3000;
		var t = document.createElement('div');
		t.className = 'toast toast--' + (level || 'info');
		var icon = { success: 'check-circle', info: 'info', warning: 'triangle-alert', danger: 'circle-alert' }[level] || 'info';
		t.innerHTML =
			'<span class="toast__icon" data-icon="' + icon + '" data-size="14"></span>' +
			'<span class="toast__body">' + msg + '</span>' +
			'<button type="button" class="toast__close" data-toast-close><span data-icon="x" data-size="12"></span></button>';
		stack.appendChild(t);
		injectIcons(t);
		var timer = setTimeout(function () { removeToast(t); }, ms);
		t.querySelector('[data-toast-close]').addEventListener('click', function () {
			clearTimeout(timer);
			removeToast(t);
		});
		return t;
	}
	function removeToast(t) {
		t.style.opacity = '0';
		t.style.transition = 'opacity 200ms';
		setTimeout(function () { if (t.parentNode) t.parentNode.removeChild(t); }, 200);
	}

	function confirmDialog(opts) {
		opts = opts || {};
		return new Promise(function (resolve) {
			var bd = document.createElement('div');
			bd.className = 'modal-backdrop confirm-dialog';
			bd.innerHTML =
				'<div class="modal" role="dialog" aria-modal="true">' +
					'<div class="modal__head">' +
						'<h2 class="modal__title">' + (opts.title || '请确认') + '</h2>' +
					'</div>' +
					'<div class="modal__body">' + (opts.body || '继续执行该操作？') + '</div>' +
					'<div class="modal__foot">' +
						'<button type="button" class="btn btn--secondary" data-cancel>' + (opts.cancelText || '取消') + '</button>' +
						'<button type="button" class="btn ' + (opts.danger ? 'btn--danger' : 'btn--primary') + '" data-ok>' + (opts.okText || '确认') + '</button>' +
					'</div>' +
				'</div>';
			document.body.appendChild(bd);
			injectIcons(bd);
			function close(v) {
				if (bd.parentNode) bd.parentNode.removeChild(bd);
				resolve(v);
			}
			bd.querySelector('[data-cancel]').addEventListener('click', function () { close(false); });
			bd.querySelector('[data-ok]').addEventListener('click', function () { close(true); });
			bd.addEventListener('click', function (e) { if (e.target === bd) close(false); });
			document.addEventListener('keydown', function esc(e) {
				if (e.key === 'Escape') { document.removeEventListener('keydown', esc); close(false); }
			});
			setTimeout(function () { bd.classList.add('open'); }, 10);
			bd.querySelector(opts.danger ? '[data-cancel]' : '[data-ok]').focus();
		});
	}

	function trapFocus(container, returnTo) {
		var sel = 'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';
		var handler = function (e) {
			if (e.key !== 'Tab') return;
			var nodes = Array.prototype.slice.call(container.querySelectorAll(sel)).filter(function (n) {
				return n.offsetParent !== null;
			});
			if (!nodes.length) return;
			var first = nodes[0], last = nodes[nodes.length - 1];
			if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
			else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
		};
		container._trap = handler;
		container.addEventListener('keydown', handler);
		if (returnTo) container._returnTo = returnTo;
	}
	function releaseFocus(container) {
		if (container._trap) {
			container.removeEventListener('keydown', container._trap);
			container._trap = null;
		}
		if (container._returnTo && document.contains(container._returnTo)) {
			container._returnTo.focus();
		}
		container._returnTo = null;
	}
	function escHandler(e, close) {
		if (e.key === 'Escape') { e.preventDefault(); close(); }
	}

	function openModal(name) {
		var el = document.querySelector('[data-modal-template="' + name + '"]');
		if (!el) return;
		var bd = document.createElement('div');
		bd.className = 'modal-backdrop';
		bd.innerHTML = '<div class="modal modal--wide" role="dialog" aria-modal="true" aria-labelledby="modal-title-' + name + '">' +
			'<div class="modal__head">' +
				'<h2 class="modal__title" id="modal-title-' + name + '">' + escapeHtml(el.getAttribute('data-modal-title') || '') + '</h2>' +
				'<button type="button" class="btn btn--ghost btn--icon btn--sm modal__close" data-modal-close aria-label="关闭"><span data-icon="x" data-size="14"></span></button>' +
			'</div>' +
			'<div class="modal__body">' + el.innerHTML + '</div>' +
			'<div class="modal__foot"><button type="button" class="btn btn--secondary" data-modal-close>关闭</button></div>' +
			'</div>';
		document.body.appendChild(bd);
		injectIcons(bd);
		bd.classList.add('open');
		trapFocus(bd, document.activeElement);
		bd.querySelector('.modal__title').setAttribute('tabindex', '-1');
		bd.querySelector('.modal__title').focus();
		bd.querySelectorAll('[data-modal-close]').forEach(function (b) {
			b.addEventListener('click', function () { closeModal(bd); });
		});
		bd.addEventListener('click', function (e) { if (e.target === bd) closeModal(bd); });
		bd.addEventListener('keydown', function (e) { escHandler(e, function () { closeModal(bd); }); });
	}
	function closeModal(bd) {
		releaseFocus(bd);
		bd.parentNode.removeChild(bd);
	}

	function openDrawer(id, trigger) {
		var d = document.getElementById(id) || document.querySelector('[data-drawer="' + id + '"]');
		if (!d) return;
		// close any open popovers so their capture-phase ESC handler doesn't intercept
		document.querySelectorAll('.popover-panel.open').forEach(function (p) {
			p.classList.remove('open');
			var t = p.parentNode.querySelector('[data-popover]');
			if (t) t.setAttribute('aria-expanded', 'false');
		});
		d.classList.add('open');
		trapFocus(d, trigger || document.activeElement);
		var title = d.querySelector('.drawer__title');
		if (title) {
			if (!title.id) title.id = 'drawer-title-' + id;
			d.setAttribute('aria-labelledby', title.id);
		}
		d.setAttribute('aria-modal', 'true');
		d.setAttribute('role', 'dialog');
		if (title) { title.setAttribute('tabindex', '-1'); title.focus(); }
		d.addEventListener('keydown', function (e) { escHandler(e, function () { closeDrawer(d); }); });
	}
	function closeDrawer(d) {
		d.removeEventListener('keydown', function () {});
		releaseFocus(d);
		d.classList.remove('open');
	}

	function openConfirmReason(btn) {
		var body = btn.getAttribute('data-confirm-reason') || '';
		var title = btn.getAttribute('data-confirm-title') || '请确认';
		var danger = btn.hasAttribute('data-confirm-danger');
		var titleId = 'confirm-title-' + Date.now();
		var bd = document.createElement('div');
		bd.className = 'modal-backdrop';
		bd.innerHTML =
			'<div class="modal ' + (danger ? 'confirm-dialog confirm-dialog--danger' : 'confirm-dialog') + '" role="dialog" aria-modal="true" aria-labelledby="' + titleId + '">' +
				'<div class="modal__head">' +
					'<h2 class="modal__title" id="' + titleId + '">' + escapeHtml(title) + '</h2>' +
					'<button type="button" class="btn btn--ghost btn--icon btn--sm modal__close" data-modal-close aria-label="关闭"><span data-icon="x" data-size="14"></span></button>' +
				'</div>' +
				'<div class="modal__body">' +
					'<p class="confirm-msg">' + escapeHtml(body) + '</p>' +
					'<div class="field mt-4">' +
						'<label class="field__label">操作原因 <span style="color:var(--danger)">*</span></label>' +
						'<textarea class="textarea" rows="3" placeholder="请填写原因，将写入审计日志" data-confirm-reason-input></textarea>' +
					'</div>' +
					'<p class="field__hint mt-2"><span data-icon="lock" data-size="11"></span> 最近身份验证：2 分钟前 · admin@bblbb</p>' +
				'</div>' +
				'<div class="modal__foot">' +
					'<button type="button" class="btn btn--secondary" data-modal-close>取消</button>' +
					'<button type="button" class="btn ' + (danger ? 'btn--danger' : 'btn--primary') + '" data-modal-confirm-ok>确认</button>' +
				'</div>' +
			'</div>';
		document.body.appendChild(bd);
		injectIcons(bd);
		bd.classList.add('open');
		trapFocus(bd, btn);
		bd.querySelector('.modal__title').setAttribute('tabindex', '-1');
		bd.querySelector('.modal__title').focus();
		bd.querySelectorAll('[data-modal-close]').forEach(function (b) {
			b.addEventListener('click', function () { closeModal(bd); });
		});
		bd.addEventListener('click', function (e) { if (e.target === bd) closeModal(bd); });
		bd.addEventListener('keydown', function (e) { escHandler(e, function () { closeModal(bd); }); });
		var okBtn = bd.querySelector('[data-modal-confirm-ok]');
		// 不可逆操作 3 秒倒计时
		if (danger) {
			var countdown = 3;
			okBtn.disabled = true;
			okBtn.textContent = '确认（' + countdown + 's）';
			var cdIv = setInterval(function () {
				countdown--;
				if (countdown <= 0) {
					okBtn.disabled = false;
					okBtn.textContent = '确认';
					clearInterval(cdIv);
				} else {
					okBtn.textContent = '确认（' + countdown + 's）';
				}
			}, 1000);
		}
		okBtn.addEventListener('click', function () {
			if (okBtn.disabled) return;
			var ta = bd.querySelector('[data-confirm-reason-input]');
			if (ta && !ta.value.trim()) {
				ta.focus();
				ta.setAttribute('aria-invalid', 'true');
				toast('warn', '请填写原因', 2400);
				return;
			}
			closeModal(bd);
			var okToast = btn.getAttribute('data-toast') || (danger ? 'success|已执行|操作已完成，审计已记录' : 'success|已确认');
			var parts = okToast.split('|');
			toast(parts[0], parts[1], parts[2] ? parseInt(parts[2], 10) : 3000);
		});
	}

	function togglePopover(btn) {
		var panel = btn.parentNode.querySelector('.popover-panel');
		if (!panel) return;
		var wasOpen = panel.classList.contains('open');
		document.querySelectorAll('.popover-panel.open').forEach(function (p) {
			p.classList.remove('open');
			var t = p.parentNode.querySelector('[data-popover]');
			if (t) t.setAttribute('aria-expanded', 'false');
		});
		if (!wasOpen) {
			panel.classList.add('open');
			btn.setAttribute('aria-expanded', 'true');
			trapFocus(panel, btn);
		} else {
			btn.setAttribute('aria-expanded', 'false');
			releaseFocus(panel);
		}
	}
	document.addEventListener('keydown', function (e) {
		if (e.key !== 'Escape') return;
		var open = document.querySelector('.popover-panel.open');
		if (!open) return;
		e.stopPropagation();
		open.classList.remove('open');
		var t = open.parentNode.querySelector('[data-popover]');
		if (t) {
			t.setAttribute('aria-expanded', 'false');
			t.focus();
		}
		releaseFocus(open);
	}, true);

	/* ------------------------------------------------------------------
	 * User hover card — triggered on any [data-user="Name"] ancestor of avatar
	 * ------------------------------------------------------------------ */
	var userCardTimer = null;
	var userCardEl = null;

	function showUserCard(name, anchor) {
		hideUserCard();
		var p = DEMO.profiles[name];
		if (!p) return;
		var roleTags = p.roles.map(roleBadge).join('');
		var el = document.createElement('div');
		el.className = 'user-popover';
		el.innerHTML =
			'<div class="user-popover__head">' +
				'<span class="avatar tone-' + p.tone + '">' + name.charAt(0) + '</span>' +
				'<div class="user-popover__who">' +
					'<a class="user-popover__name" href="users-' + name + '.html">' + name + '</a>' +
					'<span class="user-popover__title">' + p.title + '</span>' +
				'</div>' +
			'</div>' +
			'<div class="user-popover__body">' +
				'<p class="user-popover__bio">' + p.bio + '</p>' +
				'<div class="user-popover__meta">' +
					'<span class="level-badge lv-' + p.level + '">LV.' + p.level + '</span>' +
					'<span class="muted">主题 ' + p.topics + ' · 回复 ' + p.replies + '</span>' +
				'</div>' +
				(roleTags ? '<div class="user-popover__roles">' + roleTags + '</div>' : '') +
			'</div>' +
			'<div class="user-popover__foot">' +
				'<span class="muted">加入于 ' + p.joined + '</span>' +
				'<span class="spacer"></span>' +
				'<a href="users-' + name + '.html" class="btn btn--ghost btn--sm">查看主页</a>' +
			'</div>';
		document.body.appendChild(el);
		injectIcons(el);
		// position
		var rect = anchor.getBoundingClientRect();
		var cardRect = el.getBoundingClientRect();
		var left = rect.left + (rect.width / 2) - (cardRect.width / 2);
		var top = rect.bottom + 8;
		if (left < 8) left = 8;
		if (left + cardRect.width > window.innerWidth - 8) left = window.innerWidth - cardRect.width - 8;
		if (top + cardRect.height > window.innerHeight - 8) top = rect.top - cardRect.height - 8;
		el.style.left = left + window.scrollX + 'px';
		el.style.top = top + window.scrollY + 'px';
		el.classList.add('open');
		userCardEl = el;
	}

	function hideUserCard() {
		if (userCardTimer) { clearTimeout(userCardTimer); userCardTimer = null; }
		if (userCardEl && userCardEl.parentNode) {
			userCardEl.classList.remove('open');
			var el = userCardEl;
			setTimeout(function () { if (el.parentNode) el.parentNode.removeChild(el); }, 120);
			userCardEl = null;
		}
	}

	function wireUserCards() {
		if (wireUserCards.__wired) return;
		wireUserCards.__wired = true;
		var delayOpen = function (anchor, name) {
			clearTimeout(userCardTimer);
			userCardTimer = setTimeout(function () { showUserCard(name, anchor); }, 220);
		};
		document.addEventListener('mouseover', function (e) {
			var a = e.target.closest('[data-user]');
			if (!a) return;
			delayOpen(a, a.getAttribute('data-user'));
		});
		document.addEventListener('mouseout', function (e) {
			var a = e.target.closest('[data-user]');
			var related = e.relatedTarget;
			if (a && (!related || (!a.contains(related) && (!userCardEl || !userCardEl.contains(related))))) {
				hideUserCard();
			}
		});
		// keep card open when hovering the card itself
		document.addEventListener('mouseover', function (e) {
			if (userCardEl && userCardEl.contains(e.target)) {
				if (userCardTimer) clearTimeout(userCardTimer);
			}
		});
		document.addEventListener('mouseout', function (e) {
			if (userCardEl && userCardEl.contains(e.target) && !e.relatedTarget) {
				hideUserCard();
			}
		});
		// focus / blur for keyboard accessibility
		document.addEventListener('focusin', function (e) {
			var a = e.target.closest('[data-user]');
			if (a) delayOpen(a, a.getAttribute('data-user'));
		});
		document.addEventListener('focusout', function (e) {
			var a = e.target.closest('[data-user]');
			if (a) hideUserCard();
		});
		// scroll / resize close
		window.addEventListener('scroll', hideUserCard, true);
		window.addEventListener('resize', hideUserCard);
	}

	/* ------------------------------------------------------------------
	 * Tab switching — [data-tab="key"] buttons toggle [data-tab-panel="key"]
	 * Works with .tabs/.tabs__btn and any side nav.
	 * ------------------------------------------------------------------ */
	function activateTab(key, scope) {
		var root = scope || document;
		root.querySelectorAll('[data-tab]').forEach(function (btn) {
			var match = btn.getAttribute('data-tab') === key;
			btn.classList.toggle('active', match);
			btn.setAttribute('aria-selected', match ? 'true' : 'false');
		});
		root.querySelectorAll('[data-tab-panel]').forEach(function (panel) {
			var match = panel.getAttribute('data-tab-panel') === key;
			panel.classList.toggle('active', match);
			panel.setAttribute('aria-hidden', match ? 'false' : 'true');
		});
	}

	function wireTabs() {
		if (wireTabs.__wired) return;
		wireTabs.__wired = true;
		document.addEventListener('click', function (e) {
			var btn = e.target.closest('[data-tab]');
			if (!btn) return;
			var key = btn.getAttribute('data-tab');
			if (!key) return;
			var scope = btn.closest('[data-tabs]') || document;
			activateTab(key, scope);
			if (scope !== document && scope.id) {
				history.replaceState(null, '', '#' + scope.id + ':' + key);
			}
		});
		// restore from hash on init
		if (location.hash) {
			var m = location.hash.match(/#([^:]+):(.+)/);
			if (m) {
				var scope = document.getElementById(m[1]);
				if (scope) activateTab(m[2], scope);
			}
		}
	}

	/* ------------------------------------------------------------------
	 * Wire delegated listeners
	 * ------------------------------------------------------------------ */
	function wireDelegates() {
		if (wireDelegates.__wired) return;
		wireDelegates.__wired = true;
		document.addEventListener('click', function (e) {
			var t = e.target.closest('[data-toast]');
			if (t) {
				e.preventDefault();
				var parts = (t.getAttribute('data-toast') || '').split('|');
				toast(parts[0] || 'info', parts[1] || '', parts[2] ? parseInt(parts[2], 10) : 3000);
				return;
			}
			var c = e.target.closest('[data-confirm]');
			if (c) {
				e.preventDefault();
				confirmDialog({
					title: c.getAttribute('data-confirm-title') || '请确认',
					body: c.getAttribute('data-confirm'),
					danger: c.hasAttribute('data-confirm-danger'),
					okText: c.getAttribute('data-confirm-ok') || '确认'
				}).then(function (ok) {
					if (ok) toast('success', '已确认');
				});
				return;
			}
			var m = e.target.closest('[data-modal]');
			if (m) {
				e.preventDefault();
				openModal(m.getAttribute('data-modal'));
				return;
			}
			var p = e.target.closest('[data-popover]');
			if (p) {
				e.preventDefault();
				e.stopPropagation();
				togglePopover(p);
				return;
			}
			var dOpen = e.target.closest('[data-drawer-open]');
			if (dOpen) {
				e.preventDefault();
				var id = dOpen.getAttribute('data-drawer-open');
				openDrawer(id, dOpen);
				return;
			}
			var dClose = e.target.closest('[data-drawer-close]');
			if (dClose) {
				e.preventDefault();
				var drawer = dClose.closest('[data-drawer]');
				if (drawer) closeDrawer(drawer);
				return;
			}
			var cr = e.target.closest('[data-confirm-reason]');
			if (cr) {
				e.preventDefault();
				openConfirmReason(cr);
				return;
			}
			// close popovers on outside click
			if (!e.target.closest('.popover-panel') && !e.target.closest('[data-popover]')) {
				document.querySelectorAll('.popover-panel.open').forEach(function (pp) {
					pp.classList.remove('open');
					var t = pp.parentNode.querySelector('[data-popover]');
					if (t) t.setAttribute('aria-expanded', 'false');
				});
			}
		});
		// search reset
		document.addEventListener('click', function (e) {
			var r = e.target.closest('[data-search-reset]');
			if (r) {
				var form = r.closest('.filter-bar, form');
				if (form) form.querySelectorAll('input, select').forEach(function (i) {
					if (i.type === 'search' || i.tagName === 'INPUT') i.value = '';
					else if (i.tagName === 'SELECT') i.selectedIndex = 0;
				});
				toast('info', '已重置筛选');
			}
		});
	}

	/* ------------------------------------------------------------------
	 * Mock data anchors — used by visual demos
	 * ------------------------------------------------------------------ */
	var DEMO = {
		user: 'Chaos',
		level: 6,
		exp: { current: 2680, max: 3000 },
		coins: 328,
		contrib: 146,
		roles: ['社区成员', 'Rust 板块版主'],
		topics: {
			'101': { title: 'Rust 小机器上的 SQLite 并发实践', author: 'Chaos', board: 'rust', tag: 'SQLite' },
			'201': { title: '使用 SvelteKit 构建博客与轻量论坛是否合理？', author: 'Chaos', board: 'web-dev', tag: 'SvelteKit' },
			'202': { title: 'axum 0.7 中间件链的顺序陷阱', author: 'Yuwen', board: 'rust', tag: 'axum' }
		},
		report: 'R-1024',
		profiles: {
			'Chaos':  { tone: 1, level: 6, title: '站长', roles: ['Rust 板块版主'], topics: 86, replies: 1240, joined: '2024-03', bio: '写 Rust 和自托管，维护这个论坛。' },
			'Yuwen':  { tone: 5, level: 5, title: '核心贡献者', roles: ['web-dev 版主'], topics: 54, replies: 892, joined: '2024-06', bio: '前端与 SSR，偶尔写点 OIDC。' },
			'Mark':   { tone: 3, level: 4, title: '活跃成员', roles: [], topics: 31, replies: 470, joined: '2024-11', bio: '关注 Markdown 渲染与 CSP 安全。' },
			'Alice':  { tone: 7, level: 3, title: '成员', roles: [], topics: 12, replies: 203, joined: '2025-02', bio: '后端与数据库调优。' },
			'Nina':   { tone: 2, level: 3, title: '成员', roles: [], topics: 9, replies: 156, joined: '2025-04', bio: '在做一个小型静态站生成器。' },
			'Reo':    { tone: 4, level: 2, title: '新人', roles: [], topics: 4, replies: 62, joined: '2025-09', bio: '刚开始学 Rust。' }
		},
		boards: [
			{ slug: 'tech-essay', name: '技术随笔' },
			{ slug: 'rust', name: 'Rust' },
			{ slug: 'web-dev', name: 'Web 开发' },
			{ slug: 'opensource', name: '开源' },
			{ slug: 'chat', name: '闲聊灌水' },
			{ slug: 'meta', name: '站务' }
		]
	};

	/* ------------------------------------------------------------------
	 * Inject all icons in DOM
	 * ------------------------------------------------------------------ */
	function injectIcons(root) {
		(root || document).querySelectorAll('[data-icon]').forEach(function (el) {
			if (el.tagName.toLowerCase() === 'svg') return;
			injectIcon(el);
		});
	}

	/* ------------------------------------------------------------------
	 * Public API
	 * ------------------------------------------------------------------ */
var ACTIVITY_KEY = 'chaos:activity-log';
	function logActivity(entry) {
		var log = [];
		try { log = JSON.parse(localStorage.getItem(ACTIVITY_KEY) || '[]'); } catch (e) { log = []; }
		log.unshift({
			ts: Date.now(),
			page: location.pathname.split('/').pop() || 'index.html',
			icon: entry.icon || 'activity',
			label: entry.label || '',
			detail: entry.detail || '',
			kind: entry.kind || 'info'
		});
		// keep latest 30
		if (log.length > 30) log = log.slice(0, 30);
		try { localStorage.setItem(ACTIVITY_KEY, JSON.stringify(log)); } catch (e) {}
	}
	function readActivity() {
		try { return JSON.parse(localStorage.getItem(ACTIVITY_KEY) || '[]'); } catch (e) { return []; }
	}
	function clearActivity() {
		try { localStorage.removeItem(ACTIVITY_KEY); } catch (e) {}
	}

	var Forum = {
		init: function (opts) {
			opts = opts || {};
			injectIcons();
			document.querySelectorAll('[data-forum-nav]').forEach(function (h) {
				if (h.querySelector('.forum-nav__inner')) return;
				buildForumNav(h);
				injectIcons(h);
			});
			document.querySelectorAll('[data-admin-nav]').forEach(function (h) {
				if (h.querySelector('.forum-nav__inner')) return;
				buildAdminNav(h);
				injectIcons(h);
			});
			document.querySelectorAll('[data-admin-side]').forEach(function (h) {
				if (h.querySelector('.admin-side__inner')) return;
				buildAdminSide(h);
				injectIcons(h);
			});
			applyTheme(readTheme());
			if (!document.querySelector('.theme-switcher')) mountFloatingThemeSwitcher();
			wireDelegates();
			wireTabs();
			wireUserCards();
			attachNavProgress();
			if (document.readyState === 'complete' || document.readyState === 'interactive') {
				startNavProgressAuto();
			} else {
				window.addEventListener('DOMContentLoaded', startNavProgressAuto);
			}
			if (opts.page) document.body.setAttribute('data-page', opts.page);
		},
		toast: toast,
		confirm: confirmDialog,
		modal: { open: openModal },
		drawer: { open: openDrawer, close: closeDrawer },
		popover: { toggle: togglePopover },
		loading: {
			set: setNavProgress,
			start: startNavProgressAuto,
			stop: stopNavProgressAuto
		},
		theme: {
			get: readTheme,
			set: writeTheme
		},
		demo: DEMO,
		activity: { log: logActivity, read: readActivity, clear: clearActivity },
		flows: { _registry: {}, register: function (name, fn) { this._registry[name] = fn; }, run: function (name) { var fn = this._registry[name]; if (fn) try { fn(); } catch (e) { console.error('[flow:' + name + ']', e); } } },
		ICONS: ICONS
	};

	window.Forum = Forum;
	// auto-boot
	if (document.readyState === 'loading') {
		document.addEventListener('DOMContentLoaded', function () { Forum.init(); });
	} else {
		Forum.init();
	}
})();