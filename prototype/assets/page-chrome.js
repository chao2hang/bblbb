/* BBLBB 原型 · 共享页面骨架（单一来源）
 * index.html（SPA 入口）与 pages/*.html（独立页面）都从这里注入：
 * SVG 图标 sprite、桌面顶栏、底部导航、Toast 容器。
 * 独立页面模式下（<body data-standalone>），站内 #链接 会跳回 SPA 入口。 */
(function () {
  var SPRITE = "<svg xmlns=\"http://www.w3.org/2000/svg\" style=\"display:none\" aria-hidden=\"true\"><symbol id=\"i-search\" viewBox=\"0 0 24 24\"><circle cx=\"11\" cy=\"11\" r=\"8\"/><path d=\"m21 21-4.3-4.3\"/></symbol><symbol id=\"i-house\" viewBox=\"0 0 24 24\"><path d=\"M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8\"/><path d=\"M3 10a2 2 0 0 1 .709-1.528l7-6a2 2 0 0 1 2.582 0l7 6A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z\"/></symbol><symbol id=\"i-mail\" viewBox=\"0 0 24 24\"><rect width=\"20\" height=\"16\" x=\"2\" y=\"4\" rx=\"2\"/><path d=\"m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7\"/></symbol><symbol id=\"i-compass\" viewBox=\"0 0 24 24\"><circle cx=\"12\" cy=\"12\" r=\"10\"/><polygon points=\"16.24 7.76 14.12 14.12 7.76 16.24 9.88 9.88 16.24 7.76\"/></symbol><symbol id=\"i-user\" viewBox=\"0 0 24 24\"><path d=\"M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2\"/><circle cx=\"12\" cy=\"7\" r=\"4\"/></symbol><symbol id=\"i-users\" viewBox=\"0 0 24 24\"><path d=\"M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2\"/><circle cx=\"9\" cy=\"7\" r=\"4\"/><path d=\"M22 21v-2a4 4 0 0 0-3-3.87\"/><path d=\"M16 3.13a4 4 0 0 1 0 7.75\"/></symbol><symbol id=\"i-bell\" viewBox=\"0 0 24 24\"><path d=\"M10.268 21a2 2 0 0 0 3.464 0\"/><path d=\"M3.262 15.326A1 1 0 0 0 4 17h16a1 1 0 0 0 .74-1.673C19.41 13.956 18 12.499 18 8A6 6 0 0 0 6 8c0 4.499-1.411 5.956-2.738 7.326\"/></symbol><symbol id=\"i-sun\" viewBox=\"0 0 24 24\"><circle cx=\"12\" cy=\"12\" r=\"4\"/><path d=\"M12 2v2\"/><path d=\"M12 20v2\"/><path d=\"m4.93 4.93 1.41 1.41\"/><path d=\"m17.66 17.66 1.41 1.41\"/><path d=\"M2 12h2\"/><path d=\"M20 12h2\"/><path d=\"m6.34 17.66-1.41 1.41\"/><path d=\"m19.07 4.93-1.41 1.41\"/></symbol><symbol id=\"i-moon\" viewBox=\"0 0 24 24\"><path d=\"M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z\"/></symbol><symbol id=\"i-heart\" viewBox=\"0 0 24 24\"><path d=\"M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.51 4.04 3 5.5l7 7Z\"/></symbol><symbol id=\"i-heart-fill\" viewBox=\"0 0 24 24\"><path d=\"M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.51 4.04 3 5.5l7 7Z\" fill=\"currentColor\" stroke=\"none\"/></symbol><symbol id=\"i-comment\" viewBox=\"0 0 24 24\"><path d=\"M7.9 20A9 9 0 1 0 4 16.1L2 22Z\"/></symbol><symbol id=\"i-share\" viewBox=\"0 0 24 24\"><circle cx=\"18\" cy=\"5\" r=\"3\"/><circle cx=\"6\" cy=\"12\" r=\"3\"/><circle cx=\"18\" cy=\"19\" r=\"3\"/><line x1=\"8.59\" x2=\"15.42\" y1=\"13.51\" y2=\"17.49\"/><line x1=\"15.41\" x2=\"8.59\" y1=\"6.51\" y2=\"10.49\"/></symbol><symbol id=\"i-refresh\" viewBox=\"0 0 24 24\"><path d=\"M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8\"/><path d=\"M21 3v5h-5\"/><path d=\"M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16\"/><path d=\"M3 21v-5h5\"/></symbol><symbol id=\"i-menu\" viewBox=\"0 0 24 24\"><line x1=\"4\" x2=\"20\" y1=\"12\" y2=\"12\"/><line x1=\"4\" x2=\"20\" y1=\"6\" y2=\"6\"/><line x1=\"4\" x2=\"20\" y1=\"18\" y2=\"18\"/></symbol><symbol id=\"i-chevron-down\" viewBox=\"0 0 24 24\"><path d=\"m6 9 6 6 6-6\"/></symbol><symbol id=\"i-plus\" viewBox=\"0 0 24 24\"><path d=\"M5 12h14\"/><path d=\"M12 5v14\"/></symbol><symbol id=\"i-x\" viewBox=\"0 0 24 24\"><path d=\"M18 6 6 18\"/><path d=\"m6 6 12 12\"/></symbol><symbol id=\"i-send\" viewBox=\"0 0 24 24\"><path d=\"M14.536 21.686a.5.5 0 0 0 .937-.024l6.5-19a.496.496 0 0 0-.635-.635l-19 6.5a.5.5 0 0 0-.024.937l7.93 3.18a2 2 0 0 1 1.112 1.11z\"/><path d=\"m21.854 2.147-10.94 10.939\"/></symbol><symbol id=\"i-flame\" viewBox=\"0 0 24 24\"><path d=\"M8.5 14.5A2.5 2.5 0 0 0 11 12c0-1.38-.5-2-1-3-1.072-2.143-.224-4.054 2-6 .5 2.5 2 4.9 4 6.5 2 1.6 3 3.5 3 5.5a7 7 0 1 1-14 0c0-1.153.433-2.294 1-3a2.5 2.5 0 0 0 2.5 2.5z\"/></symbol><symbol id=\"i-tag\" viewBox=\"0 0 24 24\"><path d=\"M12.586 2.586A2 2 0 0 0 11.172 2H4a2 2 0 0 0-2 2v7.172a2 2 0 0 0 .586 1.414l8.704 8.704a2.426 2.426 0 0 0 3.42 0l6.58-6.58a2.426 2.426 0 0 0 0-3.42z\"/><circle cx=\"7.5\" cy=\"7.5\" r=\".5\" fill=\"currentColor\"/></symbol><symbol id=\"i-star\" viewBox=\"0 0 24 24\"><path d=\"M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.123 2.123 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.123 2.123 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.122 2.122 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.122 2.122 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.122 2.122 0 0 0 1.597-1.16z\"/></symbol><symbol id=\"i-bookmark\" viewBox=\"0 0 24 24\"><path d=\"m19 21-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z\"/></symbol><symbol id=\"i-settings\" viewBox=\"0 0 24 24\"><path d=\"M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z\"/><circle cx=\"12\" cy=\"12\" r=\"3\"/></symbol><symbol id=\"i-log-out\" viewBox=\"0 0 24 24\"><path d=\"M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4\"/><path d=\"m16 17 5-5-5-5\"/><path d=\"M21 12H9\"/></symbol><symbol id=\"i-edit\" viewBox=\"0 0 24 24\"><path d=\"M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z\"/></symbol><symbol id=\"i-check\" viewBox=\"0 0 24 24\"><path d=\"M20 6 9 17l-5-5\"/></symbol><symbol id=\"i-eye\" viewBox=\"0 0 24 24\"><path d=\"M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 0 1 0 .696 10.75 10.75 0 0 1-19.876 0\"/><circle cx=\"12\" cy=\"12\" r=\"3\"/></symbol><symbol id=\"i-trophy\" viewBox=\"0 0 24 24\"><path d=\"M6 9H4.5a2.5 2.5 0 0 1 0-5H6\"/><path d=\"M18 9h1.5a2.5 2.5 0 0 0 0-5H18\"/><path d=\"M4 22h16\"/><path d=\"M10 14.66V17c0 .55-.47.98-.97 1.21C7.85 18.75 7 20.24 7 22\"/><path d=\"M14 14.66V17c0 .55.47.98.97 1.21C16.15 18.75 17 20.24 17 22\"/><path d=\"M18 2H6v7a6 6 0 0 0 12 0V2Z\"/></symbol><symbol id=\"i-coins\" viewBox=\"0 0 24 24\"><circle cx=\"8\" cy=\"8\" r=\"6\"/><path d=\"M18.09 10.37A6 6 0 1 1 10.34 18\"/><path d=\"M7 6h1v4\"/><path d=\"m16.71 13.88.7.71-2.82 2.82\"/></symbol><symbol id=\"i-shield\" viewBox=\"0 0 24 24\"><path d=\"M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z\"/></symbol><symbol id=\"i-palette\" viewBox=\"0 0 24 24\"><circle cx=\"13.5\" cy=\"6.5\" r=\".5\" fill=\"currentColor\"/><circle cx=\"17.5\" cy=\"10.5\" r=\".5\" fill=\"currentColor\"/><circle cx=\"8.5\" cy=\"7.5\" r=\".5\" fill=\"currentColor\"/><circle cx=\"6.5\" cy=\"12.5\" r=\".5\" fill=\"currentColor\"/><path d=\"M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.926 0 1.648-.746 1.648-1.688 0-.437-.18-.835-.437-1.125-.29-.289-.438-.652-.438-1.125a1.64 1.64 0 0 1 1.668-1.668h1.996c3.051 0 5.555-2.503 5.555-5.554C21.965 6.012 17.461 2 12 2z\"/></symbol><symbol id=\"i-lock\" viewBox=\"0 0 24 24\"><rect width=\"18\" height=\"11\" x=\"3\" y=\"11\" rx=\"2\" ry=\"2\"/><path d=\"M7 11V7a5 5 0 0 1 10 0v4\"/></symbol><symbol id=\"i-flag\" viewBox=\"0 0 24 24\"><path d=\"M4 15s1-1 4-1 5 2 8 2 4-1 4-1V3s-1 1-4 1-5-2-8-2-4 1-4 1z\"/><line x1=\"4\" x2=\"4\" y1=\"22\" y2=\"15\"/></symbol><symbol id=\"i-copy\" viewBox=\"0 0 24 24\"><rect width=\"14\" height=\"14\" x=\"8\" y=\"8\" rx=\"2\" ry=\"2\"/><path d=\"M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2\"/></symbol><symbol id=\"i-book-open\" viewBox=\"0 0 24 24\"><path d=\"M12 7v14\"/><path d=\"M3 18a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h5a4 4 0 0 1 4 4 4 4 0 0 1 4-4h5a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1h-6a3 3 0 0 0-3 3 3 3 0 0 0-3-3z\"/></symbol><symbol id=\"i-medal\" viewBox=\"0 0 24 24\"><path d=\"M7.21 15 2.66 7.14a2 2 0 0 1 .13-2.2L4.4 2.8A2 2 0 0 1 6 2h12a2 2 0 0 1 1.6.8l1.6 2.14a2 2 0 0 1 .14 2.2L16.79 15\"/><path d=\"M11 12 5.12 2.2\"/><path d=\"m13 12 5.88-9.8\"/><path d=\"M8 7h8\"/><circle cx=\"12\" cy=\"17\" r=\"5\"/><path d=\"M12 18v-2h-.5\"/></symbol><symbol id=\"i-message-square\" viewBox=\"0 0 24 24\"><path d=\"M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z\"/></symbol><symbol id=\"i-shopping-cart\" viewBox=\"0 0 24 24\"><circle cx=\"8\" cy=\"21\" r=\"1\"/><circle cx=\"19\" cy=\"21\" r=\"1\"/><path d=\"M2.05 2.05h2l2.66 12.42a2 2 0 0 0 2 1.58h9.78a2 2 0 0 0 1.95-1.57l1.65-7.43H5.12\"/></symbol><symbol id=\"i-workflow\" viewBox=\"0 0 24 24\"><rect width=\"8\" height=\"8\" x=\"3\" y=\"3\" rx=\"2\"/><path d=\"M7 11v4a2 2 0 0 0 2 2h4\"/><rect width=\"8\" height=\"8\" x=\"13\" y=\"13\" rx=\"2\"/></symbol><symbol id=\"i-file-text\" viewBox=\"0 0 24 24\"><path d=\"M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7z\"/><path d=\"M14 2v4a2 2 0 0 0 2 2h4\"/><path d=\"M10 9H8\"/><path d=\"M16 13H8\"/><path d=\"M16 17H8\"/></symbol><symbol id=\"i-activity\" viewBox=\"0 0 24 24\"><path d=\"M22 12h-4l-3 9L9 3l-3 9H2\"/></symbol><symbol id=\"i-link\" viewBox=\"0 0 24 24\"><path d=\"M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71\"/><path d=\"M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71\"/></symbol><symbol id=\"i-list\" viewBox=\"0 0 24 24\"><path d=\"M3 12h.01\"/><path d=\"M3 18h.01\"/><path d=\"M3 6h.01\"/><path d=\"M8 12h13\"/><path d=\"M8 18h13\"/><path d=\"M8 6h13\"/></symbol><symbol id=\"i-monitor\" viewBox=\"0 0 24 24\"><rect width=\"20\" height=\"14\" x=\"2\" y=\"3\" rx=\"2\"/><line x1=\"8\" x2=\"16\" y1=\"21\" y2=\"21\"/><line x1=\"12\" x2=\"12\" y1=\"17\" y2=\"21\"/></symbol><symbol id=\"i-smartphone\" viewBox=\"0 0 24 24\"><rect width=\"14\" height=\"20\" x=\"5\" y=\"2\" rx=\"2\" ry=\"2\"/><path d=\"M12 18h.01\"/></symbol><symbol id=\"i-paperclip\" viewBox=\"0 0 24 24\"><path d=\"m16 6-8.414 8.586a2 2 0 0 0 2.829 2.829l8.414-8.586a4 4 0 1 0-5.657-5.657l-8.399 8.585a6 6 0 1 0 8.485 8.485L21 13\"/></symbol><symbol id=\"i-layout-dashboard\" viewBox=\"0 0 24 24\"><rect width=\"7\" height=\"9\" x=\"3\" y=\"3\" rx=\"1\"/><rect width=\"7\" height=\"5\" x=\"14\" y=\"3\" rx=\"1\"/><rect width=\"7\" height=\"9\" x=\"14\" y=\"12\" rx=\"1\"/><rect width=\"7\" height=\"5\" x=\"3\" y=\"16\" rx=\"1\"/></symbol><symbol id=\"i-award\" viewBox=\"0 0 24 24\"><circle cx=\"12\" cy=\"8\" r=\"6\"/><path d=\"M15.477 12.89 17 22l-5-3-5 3 1.523-9.11\"/></symbol><symbol id=\"i-puzzle\" viewBox=\"0 0 24 24\"><path d=\"M19.439 7.85c-.049.322.059.648.289.878l1.568 1.568c.47.47.706 1.087.706 1.704s-.235 1.233-.706 1.704l-1.611 1.611a.98.98 0 0 1-.837.276c-.47-.07-.802-.48-.968-.927a2.501 2.501 0 1 0-3.214 3.214c.446.166.856.497.927.968a.979.979 0 0 1-.276.837l-1.61 1.61a2.404 2.404 0 0 1-1.705.707 2.402 2.402 0 0 1-1.704-.706l-1.568-1.568a1.026 1.026 0 0 0-.877-.29c-.493.074-.84.504-1.02.968a2.5 2.5 0 1 1-3.237-3.237c.464-.18.894-.527.967-1.02a1.026 1.026 0 0 0-.289-.877l-1.568-1.568A2.402 2.402 0 0 1 1.998 12c0-.617.236-1.234.706-1.704L4.23 8.77c.24-.24.581-.353.917-.303.515.077.877.528 1.073 1.01a2.5 2.5 0 1 0 3.259-3.259c-.482-.196-.933-.558-1.01-1.073-.05-.336.062-.676.303-.917l1.525-1.525A2.402 2.402 0 0 1 12 1.998c.617 0 1.234.236 1.704.706l1.568 1.568c.23.23.556.338.877.29.493-.074.84-.504 1.02-.968a2.5 2.5 0 1 1 3.237 3.237c-.464.18-.894.527-.967 1.02Z\"/></symbol><symbol id=\"i-server\" viewBox=\"0 0 24 24\"><rect width=\"20\" height=\"8\" x=\"2\" y=\"2\" rx=\"2\" ry=\"2\"/><rect width=\"20\" height=\"8\" x=\"2\" y=\"14\" rx=\"2\" ry=\"2\"/><line x1=\"6\" x2=\"6.01\" y1=\"6\" y2=\"6\"/><line x1=\"6\" x2=\"6.01\" y1=\"18\" y2=\"18\"/></symbol><symbol id=\"i-download\" viewBox=\"0 0 24 24\"><path d=\"M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4\"/><polyline points=\"7 10 12 15 17 10\"/><line x1=\"12\" x2=\"12\" y1=\"15\" y2=\"3\"/></symbol><symbol id=\"i-sparkles\" viewBox=\"0 0 24 24\"><path d=\"m12 3-1.912 5.813a2 2 0 0 1-1.275 1.275L3 12l5.813 1.912a2 2 0 0 1 1.275 1.275L12 21l1.912-5.813a2 2 0 0 1 1.275-1.275L21 12l-5.813-1.912a2 2 0 0 1-1.275-1.275L12 3Z\"/><path d=\"M5 3v4\"/><path d=\"M19 17v4\"/><path d=\"M3 5h4\"/><path d=\"M17 19h4\"/></symbol><symbol id=\"i-video\" viewBox=\"0 0 24 24\"><path d=\"m16 13 5.223 3.482a.5.5 0 0 0 .777-.416V7.87a.5.5 0 0 0-.752-.432L16 10.5\"/><rect x=\"2\" y=\"6\" width=\"14\" height=\"12\" rx=\"2\"/></symbol><symbol id=\"i-bar-chart\" viewBox=\"0 0 24 24\"><line x1=\"12\" x2=\"12\" y1=\"20\" y2=\"10\"/><line x1=\"18\" x2=\"18\" y1=\"20\" y2=\"4\"/><line x1=\"6\" x2=\"6\" y1=\"20\" y2=\"16\"/></symbol><symbol id=\"i-key\" viewBox=\"0 0 24 24\"><path d=\"m21 2-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0 3 3L22 7l-3-3m-3.5 3.5L19 4\"/></symbol></svg>";
  var HEADER = "  <header class=\"desktop-header\">\n    <a class=\"wordmark\" href=\"#home\"><b>BBLBB</b></a>\n    <form class=\"search\" id=\"header-search-form\" role=\"search\"><svg class=\"ic ic-16\"><use href=\"#i-search\"/></svg><input id=\"header-search-input\" placeholder=\"搜索\" aria-label=\"搜索\" autocomplete=\"off\"></form>\n<nav class=\"desktop-nav\">\n      <a data-route href=\"#home\" class=\"active\"><svg class=\"ic\"><use href=\"#i-house\"/></svg>首页</a>\n      <a data-route href=\"#discover\"><svg class=\"ic\"><use href=\"#i-compass\"/></svg>发现</a>\n      <a data-route href=\"#messages\" style=\"position:relative\"><svg class=\"ic\"><use href=\"#i-mail\"/></svg>消息<em class=\"nav-dot\"></em></a>\n<div class=\"drop-wrap\">\n        <button class=\"nav-more-btn\" onclick=\"toggleDrop(event,'dd-more')\" aria-haspopup=\"menu\" aria-expanded=\"false\">更多<svg class=\"ic ic-14\"><use href=\"#i-chevron-down\"/></svg></button>\n<div class=\"dropdown dd-right\" id=\"dd-more\" role=\"menu\" aria-label=\"更多导航\">\n          <button class=\"dd-item\" onclick=\"closeDrops();go('#search')\"><svg class=\"ic\"><use href=\"#i-search\"/></svg>搜索</button><button class=\"dd-item\" onclick=\"closeDrops();go('#loading')\"><svg class=\"ic\"><use href=\"#i-refresh\"/></svg>加载页预览</button>\n          <button class=\"dd-item\" data-auth-required onclick=\"closeDrops();go('#shop')\"><svg class=\"ic\"><use href=\"#i-coins\"/></svg>商城与积分</button>\n          <button class=\"dd-item\" onclick=\"closeDrops();go('#achievements')\"><svg class=\"ic\"><use href=\"#i-trophy\"/></svg>成就墙</button>\n          <button class=\"dd-item\" onclick=\"closeDrops();go('#articles')\"><svg class=\"ic\"><use href=\"#i-book-open\"/></svg>文章</button>\n          <button class=\"dd-item\" data-auth-required onclick=\"closeDrops();go('#notifications')\"><svg class=\"ic\"><use href=\"#i-bell\"/></svg>通知中心</button>\n          <button class=\"dd-item\" data-auth-required onclick=\"closeDrops();go('#billing')\"><svg class=\"ic\"><use href=\"#i-download\"/></svg>下载账单</button>\n          <hr><button class=\"dd-item\" data-auth-required onclick=\"closeDrops();go('#admin')\"><svg class=\"ic\"><use href=\"#i-shield\"/></svg>管理后台</button>\n          <button class=\"dd-item\" onclick=\"closeDrops();go('#design')\"><svg class=\"ic\"><use href=\"#i-palette\"/></svg>原型标准文档</button>\n</div>\n</div>\n      <button class=\"theme-toggle\" onclick=\"toggleTheme()\" title=\"日间 / 夜间切换\" aria-label=\"切换主题\"><svg class=\"ic icon-sun\"><use href=\"#i-sun\"/></svg><svg class=\"ic icon-moon\"><use href=\"#i-moon\"/></svg></button>\n<div class=\"drop-wrap\">\n        <button class=\"icon-btn\" onclick=\"toggleDrop(event,'dd-bell')\" title=\"通知\" aria-label=\"通知\" aria-haspopup=\"menu\" aria-expanded=\"false\"><svg class=\"ic\"><use href=\"#i-bell\"/></svg><em class=\"badge\" id=\"bell-badge\">2</em></button>\n<div class=\"dropdown dd-right\" id=\"dd-bell\" role=\"menu\">\n<div class=\"dd-head\">通知<b id=\"dd-bell-count\">· 2 条未读</b><button type=\"button\" onclick=\"markAllRead()\">全部已读</button></div>\n          <button class=\"dd-item\" role=\"menuitem\" data-guest-hide onclick=\"closeDrops();go('#notifications')\"><svg class=\"ic heart-c\"><use href=\"#i-heart\"/></svg><span>Lin 赞了你的回复<small>12 分钟前</small></span></button>\n          <button class=\"dd-item\" role=\"menuitem\" data-guest-hide onclick=\"closeDrops();go('#notifications')\"><svg class=\"ic\"><use href=\"#i-comment\"/></svg><span>Mark 评论了你的帖子<small>1 小时前</small></span></button>\n          <button class=\"dd-item\" role=\"menuitem\" onclick=\"closeDrops();go('#notifications')\"><svg class=\"ic warn-c\"><use href=\"#i-tag\"/></svg><span>系统：社区规范已更新<small>昨天</small></span></button>\n</div>\n</div>\n<div class=\"drop-wrap\">\n        <button class=\"user-chip\" id=\"user-chip\" onclick=\"toggleDrop(event,'dd-user')\" aria-haspopup=\"menu\" aria-expanded=\"false\"><span class=\"avatar-sm\" id=\"header-avatar\">A</span><span class=\"uname\" id=\"header-username\">admin</span><svg class=\"ic ic-14\"><use href=\"#i-chevron-down\"/></svg></button>\n<div class=\"dropdown dd-right\" id=\"dd-user\" role=\"menu\" aria-label=\"用户菜单\">\n          <button class=\"dd-item\" role=\"menuitem\" data-auth-required onclick=\"closeDrops();go('#me')\"><svg class=\"ic\"><use href=\"#i-user\"/></svg>个人主页</button>\n          <button class=\"dd-item\" role=\"menuitem\" data-auth-required onclick=\"closeDrops();go('#me');switchTab('fav')\"><svg class=\"ic\"><use href=\"#i-bookmark\"/></svg>我的收藏</button>\n          <button class=\"dd-item\" role=\"menuitem\" data-auth-required onclick=\"closeDrops();openPublish()\"><svg class=\"ic\"><use href=\"#i-edit\"/></svg>发布内容</button>\n          <button class=\"dd-item\" role=\"menuitem\" data-auth-required onclick=\"closeDrops();go('#me');switchTab('set')\"><svg class=\"ic\"><use href=\"#i-settings\"/></svg>设置</button>\n          <hr><button class=\"dd-item\" role=\"menuitem\" data-auth-action=\"login\" onclick=\"closeDrops();go('#login')\"><svg class=\"ic\"><use href=\"#i-user\"/></svg>登录</button><button class=\"dd-item danger\" role=\"menuitem\" data-auth-action=\"logout\" onclick=\"logout()\"><svg class=\"ic\"><use href=\"#i-log-out\"/></svg>退出登录</button>\n</div>\n</div>\n</nav>\n</header>";
  var BOTTOM_NAV = "<nav class=\"bottom-nav\">\n    <a data-route href=\"#home\" class=\"active\"><svg class=\"ic\"><use href=\"#i-house\"/></svg><span>首页</span></a>\n    <a data-route href=\"#discover\"><svg class=\"ic\"><use href=\"#i-compass\"/></svg><span>发现</span></a>\n    <button onclick=\"openPublish()\" aria-label=\"发布\"><svg class=\"ic ic-fab\"><use href=\"#i-plus\"/></svg></button>\n    <a data-route href=\"#messages\"><svg class=\"ic\"><use href=\"#i-mail\"/></svg><span>消息</span></a>\n    <a data-route href=\"#me\" aria-label=\"登录\"><svg class=\"ic\"><use href=\"#i-user\"/></svg><span>我的</span></a>\n</nav>";
  var TOAST = "<div id=\"toast\" role=\"status\" aria-live=\"polite\"></div>";
  var PROGRESS = '<div class="route-progress" id="route-progress" aria-hidden="true"><i></i></div>';

  /* —— 页面转场加载条（黑白灰，亮/暗主题自适应）——
   * start()：路由开始，细线从 0 拉升到 72%；
   * done()：路由渲染完成，补到 100% 后淡出（最短显示 RP_MIN_RUN，保证快速路由可感知）；
   * cancel()：路由被守卫拦截等情形，直接淡出；
   * 运行中重复 start() 不重置；prefers-reduced-motion 时不做横向位移，仅淡入淡出。 */
  var RP_MIN_RUN = 450, RP_RAMP_MS = 500, RP_DONE_MS = 240, RP_FADE_MS = 340;
  var rpState = 'idle', rpTimers = [], rpStartedAt = 0;
  function rpClearTimers() { rpTimers.forEach(function (t) { clearTimeout(t); }); rpTimers = []; }
  function rpReduced() { try { return !!(window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches); } catch (e) { return false; } }
  function rpEl() {
    var el = document.getElementById('route-progress');
    if (el) return el;
    el = document.createElement('div');
    el.className = 'route-progress'; el.id = 'route-progress'; el.setAttribute('aria-hidden', 'true');
    el.innerHTML = '<i></i>';
    var header = document.querySelector('.app-shell .desktop-header') || document.querySelector('.desktop-header');
    if (header && header.parentNode) header.parentNode.insertBefore(el, header.nextSibling);
    else (document.body || document.documentElement).insertBefore(el, null);
    return el;
  }
  function rpFadeOut(el, after) {
    el.classList.remove('is-active');
    rpTimers.push(setTimeout(function () {
      var fill = el.firstChild;
      if (fill) { fill.style.transition = 'none'; fill.style.width = '0%'; }
      rpState = 'idle';
      after();
    }, RP_FADE_MS));
  }
  function rpStart() {
    if (rpState === 'running') return;
    var el = rpEl(); if (!el) return;
    var fill = el.firstChild;
    rpClearTimers();
    rpState = 'running'; rpStartedAt = Date.now();
    el.classList.add('is-active');
    if (!fill) return;
    if (rpReduced()) { fill.style.transition = 'none'; fill.style.width = '100%'; return; }
    fill.style.transition = 'none'; fill.style.width = '0%';
    void fill.offsetWidth;
    fill.style.transition = 'width ' + RP_RAMP_MS + 'ms cubic-bezier(0.25,0.8,0.35,1)';
    fill.style.width = '72%';
  }
  function rpDone() {
    if (rpState === 'idle') return;
    var el = rpEl(); if (!el) { rpState = 'idle'; return; }
    var fill = el.firstChild;
    rpClearTimers();
    rpState = 'finishing';
    if (fill) {
      if (!rpReduced()) fill.style.transition = 'width ' + RP_DONE_MS + 'ms ease-out';
      fill.style.width = '100%';
    }
    var wait = Math.max(0, RP_MIN_RUN - (Date.now() - rpStartedAt)) + RP_DONE_MS;
    rpTimers.push(setTimeout(function () { rpFadeOut(el, function () {}); }, wait));
  }
  function rpCancel() {
    if (rpState === 'idle') return;
    var el = rpEl(); if (!el) { rpState = 'idle'; return; }
    rpClearTimers();
    rpState = 'finishing';
    rpFadeOut(el, function () {});
  }
  window.__routeProgress = { start: rpStart, done: rpDone, cancel: rpCancel };
  /* 设计文档页的演示入口（触发一次完整的 start→done 时序） */
  window.designProgressDemo = function () { rpStart(); rpTimers.push(setTimeout(rpDone, 1100)); };
  /* 设计文档页交互：章节锚点、复制清单、标签示例（此前缺失，点击会抛 ReferenceError） */
  window.designJump = window.designJump || function (id) {
    var target = document.getElementById(id);
    if (!target) return;
    target.scrollIntoView({ behavior: 'smooth', block: 'start' });
    Array.prototype.forEach.call(document.querySelectorAll('.design-doc-nav [data-design-target]'), function (btn) {
      btn.classList.toggle('is-active', btn.getAttribute('data-design-target') === id);
    });
  };
  window.copyDesignChecklist = window.copyDesignChecklist || function (button) {
    var items = Array.prototype.map.call(document.querySelectorAll('#design-checklist .design-check'), function (label) { return '- ' + label.textContent.replace(/\s+/g, ' ').trim(); });
    if (!items.length) return;
    var text = items.join('\n');
    var restore = function () { if (button && button.dataset.copyText) button.innerHTML = button.dataset.copyText; };
    if (!button) return;
    var markDone = function () { button.dataset.copyText = button.innerHTML; button.textContent = '已复制 ✓'; setTimeout(restore, 1500); };
    if (navigator.clipboard && navigator.clipboard.writeText) { navigator.clipboard.writeText(text).then(markDone, markDone); }
    else { try { var ta = document.createElement('textarea'); ta.value = text; document.body.appendChild(ta); ta.select(); document.execCommand('copy'); ta.remove(); markDone(); } catch (e) { restore(); } }
  };
  window.pickTagDemo = window.pickTagDemo || function (button) { button.classList.toggle('is-picked'); };

  function mount() {
    var body = document.body;
    var skip = body.querySelector('.skip-link');
    if (skip) skip.insertAdjacentHTML('afterend', SPRITE);
    else body.insertAdjacentHTML('afterbegin', SPRITE);
    var shell = body.querySelector('.app-shell');
    if (!shell) {
      var main = body.querySelector('main.layout');
      if (main) {
        shell = document.createElement('div');
        shell.className = 'app-shell';
        body.insertBefore(shell, main);
        shell.appendChild(main);
      }
    }
    if (shell) {
      shell.insertAdjacentHTML('afterbegin', HEADER);
      var headerEl = shell.firstElementChild;
      var barEl = document.getElementById('route-progress'); /* 启动期 rpEl() 可能已创建（page-loader 先于 mount 触发 start） */
      if (headerEl && headerEl.classList.contains('desktop-header')) {
        if (barEl) headerEl.insertAdjacentElement('afterend', barEl);
        else headerEl.insertAdjacentHTML('afterend', PROGRESS);
      } else if (!barEl) shell.insertAdjacentHTML('afterbegin', PROGRESS);
      shell.insertAdjacentHTML('beforeend', BOTTOM_NAV);
      var contentNav = shell.querySelector('.desktop-nav button[onclick*="#articles"]');
      if (contentNav) Array.prototype.forEach.call(contentNav.childNodes, function (node) { if (node.nodeType === 3 && node.textContent.trim()) node.textContent = '内容'; });
    }
    if (!body.querySelector('#toast')) body.insertAdjacentHTML('beforeend', TOAST);
    var publishPage = body.querySelector('#page-publish');
    if (publishPage) {
      var publishType = publishPage.querySelector('.composer-type-switch'); if (publishType) publishType.remove();
      var publishTitle = publishPage.querySelector('h1'); if (publishTitle) publishTitle.textContent = '发布内容';
      var publishCrumb = publishPage.querySelector('.topic-context__current'); if (publishCrumb) publishCrumb.textContent = '发布内容';
      var publishEyebrow = publishPage.querySelector('.composer-eyebrow'); if (publishEyebrow) publishEyebrow.textContent = 'CREATE / CONTENT';
      var summaryLabel = publishPage.querySelector('label[for="publish-summary"]'); if (summaryLabel) summaryLabel.textContent = '摘要';
    }
    var contentPage = body.querySelector('#page-articles');
    /* 原型仅保留标题与操作，移除解释性导语、提示和实现说明。 */
    Array.prototype.forEach.call(body.querySelectorAll('.app-route-head p, .app-route-intro, .app-notice, .app-field-help, .app-field-hint, .login-demo-hint, .app-admin-side__foot, .app-promo'), function (node) { node.remove(); });
    if (contentPage) {
      var contentTitle = contentPage.querySelector('h1'); if (contentTitle) contentTitle.textContent = '内容';
      var contentLede = contentPage.querySelector('.app-route-head p'); if (contentLede) contentLede.remove();
      Array.prototype.forEach.call(contentPage.querySelectorAll('[data-go-publish]'), function (button) { button.textContent = '发布内容'; });
      Array.prototype.forEach.call(contentPage.querySelectorAll('.sbadge'), function (badge) { if (badge.textContent.trim() === '文章') badge.textContent = '内容'; });
    }
    var topicPage = body.querySelector('#page-topic');
    if (topicPage) {
      var topicCrumb = topicPage.querySelector('.topic-context__current'); if (topicCrumb) topicCrumb.textContent = '内容详情';
      var topicBadge = topicPage.querySelector('.topic-meta .sbadge'); if (topicBadge) topicBadge.textContent = '内容';
    }
    var boardPublish = body.querySelector('#page-board [data-go-publish]'); if (boardPublish) boardPublish.textContent = '发布内容';
    var tagLede = body.querySelector('#page-tag .app-route-head p'); if (tagLede) tagLede.remove();
    var userTabs = body.querySelector('#page-me [data-user-tab="posts"]'); if (userTabs) userTabs.textContent = '内容';
    var userTopics = body.querySelector('#page-me [data-user-tab="topics"]'); if (userTopics) userTopics.remove();
    /* 顶栏/导航的内联 onclick 依赖引擎全局函数；独立页面先给空实现（SPA 中引擎随后覆盖） */
    ['toggleDrop', 'closeDrops', 'go', 'markAllRead', 'switchTab', 'openPublish', 'logout', 'toggleTheme', 'startLoading'].forEach(function (name) {
      if (typeof window[name] !== 'function') window[name] = function () {};
    });
    if (body.getAttribute('data-standalone')) {
      document.addEventListener('click', function (event) {
        var target = event.target;
        var anchor = target && target.closest ? target.closest('a[href^="#"]') : null;
        if (!anchor) return;
        var href = anchor.getAttribute('href') || '';
        if (href.length > 1) { event.preventDefault(); if (window.__routeProgress) window.__routeProgress.start(); location.href = '../index.html' + href; }
      });
    }
  }

  window.BBLBBChrome = { sprite: SPRITE, header: HEADER, bottomNav: BOTTOM_NAV, toast: TOAST, mount: mount };
  /* 脚本位于 body 顶部，执行时 app-shell / main 可能尚未解析：等 DOM 就绪再挂载 */
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', mount);
  else mount();
})();
