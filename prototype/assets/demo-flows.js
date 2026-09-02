/* ============================================================================
 * Demo deep-simulation flows (v2 — uses Forum.activity + Forum.flows SDK)
 * Each flow registers itself; the dispatcher at the bottom routes by pathname.
 * ============================================================================
 */
(function () {
	'use strict';
	if (!window.Forum) return;

	function escHtml(s) {
		return String(s).replace(/[&<>"']/g, function (c) {
			return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c];
		});
	}
	function nl2br(s) { return escHtml(s).replace(/\n/g, '<br>'); }
	function pad(n) { return n < 10 ? '0' + n : '' + n; }
	function nowStr() {
		var d = new Date();
		return d.getFullYear() + '-' + pad(d.getMonth() + 1) + '-' + pad(d.getDate()) + ' ' + pad(d.getHours()) + ':' + pad(d.getMinutes());
	}
	function lsGet(k) { try { return localStorage.getItem(k); } catch (e) { return null; } }
	function lsSet(k, v) { try { localStorage.setItem(k, v); } catch (e) {} }
	function qs(sel, root) { return (root || document).querySelector(sel); }
	function qsa(sel, root) { return Array.prototype.slice.call((root || document).querySelectorAll(sel)); }
	function log(label, detail, kind, icon) {
		Forum.activity.log({ label: label, detail: detail || '', kind: kind || 'info', icon: icon || 'activity' });
	}
	function toast(level, msg, ms) { Forum.toast(level, msg, ms || 2400); }

	/* -------------------------------------------------------------------
	 * 共享主题工具：14 封闭 token → CSS 变量 / 枚举辅助属性（与正式
	 * THEME_TOKEN_KEYS / theme-bridge.css / themeDataAttrs 对齐）。
	 * 供 themes flow（管理页）与 settings flow（主题偏好）复用。
	 * ------------------------------------------------------------------- */
	var BB_VAR = {
		'color.background': '--bb-color-background',
		'color.surface': '--bb-color-surface',
		'color.text': '--bb-color-text',
		'color.muted': '--bb-color-muted',
		'color.accent': '--bb-color-accent',
		'color.border': '--bb-color-border',
		'font.body': '--bb-font-body',
		'font.mono': '--bb-font-mono',
		'radius.control': '--bb-radius-control',
		'radius.card': '--bb-radius-card',
		'space.density': 'data-density',
		'shadow.card': 'data-shadow',
		'motion.duration': '--bb-motion-duration',
		'motion.reduced': 'data-motion-reduced'
	};
	var THEME_TOKENS = {
		'default': {
			'color.background': '#ffffff', 'color.surface': '#ffffff', 'color.text': '#111111',
			'color.muted': '#5f5f5f', 'color.accent': '#0088cc', 'color.border': '#dddddd',
			'font.body': 'Inter, ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", "Noto Sans SC", "PingFang SC", "Microsoft YaHei", sans-serif',
			'font.mono': '"SFMono-Regular", ui-monospace, Consolas, "Liberation Mono", "Cascadia Mono", monospace',
			'radius.control': '4px', 'radius.card': '6px',
			'space.density': 'comfortable', 'shadow.card': 'sm', 'motion.duration': '180ms', 'motion.reduced': false
		},
		'warm': {
			'color.background': '#fff8ee', 'color.surface': '#fff8ee', 'color.text': '#3a1f0a',
			'color.muted': '#8a6d4f', 'color.accent': '#b0531a', 'color.border': '#e8d9c2',
			'font.body': 'Inter, ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", "Noto Sans SC", "PingFang SC", "Microsoft YaHei", sans-serif',
			'font.mono': '"SFMono-Regular", ui-monospace, Consolas, "Liberation Mono", "Cascadia Mono", monospace',
			'radius.control': '4px', 'radius.card': '6px',
			'space.density': 'compact', 'shadow.card': 'sm', 'motion.duration': '160ms', 'motion.reduced': false
		},
		'solarized': {
			'color.background': '#fdf6e3', 'color.surface': '#fdf6e3', 'color.text': '#586e75',
			'color.muted': '#839496', 'color.accent': '#268bd2', 'color.border': '#eee8d5',
			'font.body': 'Inter, ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", "Noto Sans SC", "PingFang SC", "Microsoft YaHei", sans-serif',
			'font.mono': '"SFMono-Regular", ui-monospace, Consolas, "Liberation Mono", "Cascadia Mono", monospace',
			'radius.control': '4px', 'radius.card': '6px',
			'space.density': 'relaxed', 'shadow.card': 'md', 'motion.duration': '220ms', 'motion.reduced': true
		}
	};
	/* 应用主题 token：写入 --bb-* 变量 + 投影枚举辅助属性。tokens 为空则复位。 */
	function applyThemeTokens(tokens) {
		var html = document.documentElement;
		if (!tokens) {
			Object.keys(BB_VAR).forEach(function (k) {
				if (BB_VAR[k].indexOf('--') === 0) html.style.removeProperty(BB_VAR[k]);
			});
			html.removeAttribute('data-density');
			html.removeAttribute('data-shadow');
			html.removeAttribute('data-motion-reduced');
			return;
		}
		Object.keys(BB_VAR).forEach(function (k) {
			var target = BB_VAR[k];
			var v = tokens[k];
			if (v === undefined || v === null) return;
			if (target.indexOf('--') === 0) {
				html.style.setProperty(target, String(v));
			} else {
				if (k === 'space.density') html.setAttribute('data-density', v === 'compact' || v === 'relaxed' ? v : 'comfortable');
				else if (k === 'shadow.card') html.setAttribute('data-shadow', v === 'none' || v === 'md' || v === 'lg' ? v : 'sm');
				else if (k === 'motion.reduced') html.setAttribute('data-motion-reduced', v === true || v === 'true' ? 'true' : 'false');
			}
		});
	}

	/* --- Flow 2: favorite --- */
	Forum.flows.register('favorite', function () {
		qsa('[data-fav-topic]').forEach(function (btn) {
			var topicId = btn.getAttribute('data-fav-topic');
			var KEY = 'chaos:fav:' + topicId;
			function isFav() { return lsGet(KEY) === '1'; }
			function render() {
				var label = btn.querySelector('.data-fav-label');
				if (isFav()) {
					btn.classList.add('btn--active');
					btn.setAttribute('aria-pressed', 'true');
					btn.style.background = 'var(--warn-soft)';
					btn.style.borderColor = 'var(--warn)';
					btn.style.color = 'var(--warn)';
					if (label) label.textContent = '已收藏';
				} else {
					btn.classList.remove('btn--active');
					btn.removeAttribute('aria-pressed');
					btn.style.background = '';
					btn.style.borderColor = '';
					btn.style.color = '';
					if (label) label.textContent = '收藏';
				}
			}
			render();
			btn.addEventListener('click', function () {
				var nowFav = !isFav();
				lsSet(KEY, nowFav ? '1' : '0');
				render();
				toast(nowFav ? 'success' : 'info', nowFav ? '已加入收藏' : '已取消收藏', 2400);
				log((nowFav ? '收藏' : '取消收藏') + ' topic-' + topicId, nowFav ? '用户在 topic-' + topicId + ' 收藏了主题' : '取消了 topic-' + topicId + ' 的收藏', nowFav ? 'success' : 'info', 'star');
			});
		});
	});

	/* --- Flow 2b: favorites page banner --- */
	Forum.flows.register('favorites', function () {
		var banner = qs('#just-favorited-banner');
		if (!banner) return;
		var fav101 = lsGet('chaos:fav:101');
		if (fav101 === '1') {
			banner.innerHTML =
				'<div class="notice notice--success mt-3"><span class="notice__icon" data-icon="star" data-size="14"></span>' +
				'<div class="notice__body"><p class="notice__title">刚刚在 topic-101 收藏 · 「Rust 小机器上的 SQLite 并发实践」</p>' +
				'<p style="font-size:0.78rem">演示流程 2 验证 · 收藏状态在浏览器本地持久化（localStorage）。</p></div></div>';
		}
	});

	/* --- Flow 3: reply unlock --- */
	Forum.flows.register('replyUnlock', function () {
		var KEY = 'chaos:topic-201:unlocked-by-reply';
		function showUnlocked() {
			var card = qs('.restricted-card');
			var mount = qs('[data-unlocked-mount]');
			var tpl = qs('[data-unlocked-content]');
			if (card) card.hidden = true;
			if (mount && tpl) {
				mount.innerHTML = '';
				mount.appendChild(tpl.content.cloneNode(true));
			}
			var count = qs('.page-head__title .muted');
			if (count && count.textContent.indexOf('14') >= 0) count.textContent = '· 15 条';
		}
		if (lsGet(KEY) === '1') showUnlocked();
		qsa('.restricted-card button').forEach(function (btn) {
			if (btn.textContent.indexOf('去回复') >= 0) {
				btn.addEventListener('click', function () {
					var ta = qs('#reply');
					if (ta) { ta.scrollIntoView({ behavior: 'smooth', block: 'center' }); setTimeout(function () { ta.focus(); }, 300); }
				});
			}
		});
		var submit = qs('#reply-submit');
		if (submit) {
			submit.addEventListener('click', function () {
				// 429 rate limit: 60 秒内第 2 次提交触发限流
				var LAST_KEY = 'chaos:topic-201:last-reply';
				var last = parseInt(lsGet(LAST_KEY) || '0', 10);
				var now = Date.now();
				if (last && now - last < 60000) {
					toast('danger', '429 · 回复限速', '1 分钟内仅可回复 1 次，请稍后重试', 3000);
					return;
				}
				var tries = 0;
				var iv = setInterval(function () {
					var ok = document.querySelector('.modal .btn--primary:not(.btn--secondary)');
					tries++;
					if (ok && !ok.__replyWired) {
						ok.__replyWired = true;
						ok.addEventListener('click', function () {
							lsSet(LAST_KEY, String(Date.now()));
							var ta = qs('#reply');
							var body = (ta && ta.value || '').trim() || '感谢分享，已参考实现。';
							appendReply(body);
							if (ta) ta.value = '';
							lsSet(KEY, '1');
							setTimeout(showUnlocked, 100);
							toast('success', '回复已发布 · 隐藏内容已解锁', 3000);
							log('回复 topic-201 · 解锁隐藏内容', body.slice(0, 60), 'success', 'message');
						});
						clearInterval(iv);
					}
					if (tries > 40) clearInterval(iv);
				}, 50);
			});
		}
		function appendReply(body) {
			var thread = qs('#reply-thread');
			if (!thread) return;
			var div = document.createElement('div');
			div.className = 'topic-reply';
			div.innerHTML =
				'<a href="users-Chaos.html" class="avatar md tone-2">C</a>' +
				'<div class="topic-reply__body">' +
					'<div class="topic-reply__head">' +
						'<a href="users-Chaos.html" class="user-badge"><span class="name">Chaos</span></a>' +
						'<span class="level-badge lv-6">LV.6</span>' +
						'<span class="status-badge soft">楼主</span>' +
						'<span class="time">刚刚</span>' +
					'</div>' +
					'<div class="topic-reply__content">' + nl2br(body) + '</div>' +
					'<div class="topic-reply__foot">' +
						'<button class="btn btn--ghost btn--sm" data-toast="info|已点赞|+1 计数（演示）"><span data-icon="thumbs-up" data-size="12"></span><span>点赞</span></button>' +
						'<button class="btn btn--ghost btn--sm" data-toast="info|已复制|回复链接已复制"><span data-icon="link" data-size="12"></span><span>引用</span></button>' +
						'<button class="btn btn--ghost btn--sm" data-toast="info|已举报|管理员会处理"><span data-icon="circle-alert" data-size="12"></span><span>举报</span></button>' +
					'</div>' +
				'</div>';
			thread.appendChild(div);
			Forum.init && Forum.init();
		}
	});

	/* --- Flow 4: paid unlock --- */
	Forum.flows.register('paidUnlock', function () {
		var KEY = 'chaos:topic-202:unlocked';
		function showUnlocked() {
			var card = qs('.restricted-card--pay');
			var mount = qs('[data-unlocked-mount]');
			var tpl = qs('[data-unlocked-content]');
			var bal = qs('[data-balance]');
			if (card) card.hidden = true;
			if (mount && tpl) {
				mount.innerHTML = '';
				mount.appendChild(tpl.content.cloneNode(true));
			}
			if (bal) bal.textContent = '318';
		}
		if (lsGet(KEY) === '1') showUnlocked();
		qsa('[data-confirm-reason]').forEach(function (btn) {
			btn.addEventListener('click', function () {
				var tries = 0;
				var iv = setInterval(function () {
					var ok = document.querySelector('[data-modal-confirm-ok]');
					tries++;
					if (ok && !ok.__paid) {
						ok.__paid = true;
						ok.addEventListener('click', function () {
							lsSet(KEY, '1');
							setTimeout(showUnlocked, 100);
							log('付费解锁 topic-202 · 扣 10 B币', '余额 328 → 318', 'success', 'coins');
						});
						clearInterval(iv);
					}
					if (tries > 40) clearInterval(iv);
				}, 50);
			});
		});
	});

	/* --- Flow 5: device logout --- */
	Forum.flows.register('devices', function () {
		var KEY = 'chaos:devices:logout';
		var logged = (lsGet(KEY) || '').split(',').filter(Boolean);
		var list = qs('#device-list');
		var count = qs('#device-count');
		function recount() {
			var items = list ? list.querySelectorAll('li[data-device]').length : 0;
			if (count) count.textContent = items;
		}
		if (list) {
			qsa('li', list).forEach(function (li, i) {
				li.setAttribute('data-device', 'dev-' + i);
			});
			logged.forEach(function (id) {
				var li = list.querySelector('[data-device="' + id + '"]');
				if (li) li.parentNode.removeChild(li);
			});
			recount();
		}
		document.addEventListener('click', function (e) {
			var btn = e.target.closest('[data-device-action="logout"]');
			if (!btn) return;
			var li = btn.closest('li[data-device]');
			if (!li) return;
			var tries = 0;
			var iv = setInterval(function () {
				var ok = document.querySelector('.modal .btn--primary:not(.btn--secondary)');
				tries++;
				if (ok && !ok.__dev) {
					ok.__dev = true;
					ok.addEventListener('click', function () {
						var id = li.getAttribute('data-device');
						if (logged.indexOf(id) < 0) logged.push(id);
						lsSet(KEY, logged.join(','));
						var title = li.querySelector('.notif-item__title') ? li.querySelector('.notif-item__title').textContent : id;
						li.style.transition = 'opacity .2s';
						li.style.opacity = '0';
						setTimeout(function () {
							li.parentNode.removeChild(li);
							recount();
							toast('success', '设备已登出', 2400);
							log('设备登出 · ' + title, '在设备列表登出 ' + id, 'info', 'log-out');
						}, 220);
					});
					clearInterval(iv);
				}
				if (tries > 40) clearInterval(iv);
			}, 50);
		});
	});

	/* --- Flow 5b: settings appearance theme preference --- */
	Forum.flows.register('settingsAppearance', function () {
		var PREF_KEY = 'chaos:theme-preference';
		var sel = qs('#set-theme');
		if (!sel) return;
		var labels = { 'default': 'Chaos 默认', 'warm': 'Chaos 暖色变体', 'solarized': 'Solarized · 第三方' };

		// 初始化：读取已保存偏好，应用到全站（与正式 PUT /me/preferences/theme 语义一致）
		var saved = lsGet(PREF_KEY);
		if (saved && sel.querySelector('option[value="' + saved + '"]')) {
			sel.value = saved;
			applyThemeTokens(THEME_TOKENS[saved]);
		}

		sel.addEventListener('change', function () {
			var v = sel.value;
			if (v === 'site-default') {
				applyThemeTokens(null);
				lsSet(PREF_KEY, '');
				toast('info', '已跟随站点默认（Chaos 默认）', 2400);
			} else if (THEME_TOKENS[v]) {
				applyThemeTokens(THEME_TOKENS[v]);
				lsSet(PREF_KEY, v);
				toast('success', '已应用主题「' + (labels[v] || v) + '」', 2400);
				log('外观 · 主题偏好', '切换为 ' + (labels[v] || v), 'success', 'palette');
			}
		});

		// 保存按钮反馈（外观区 footer）
		var saveBtn = qs('[data-tab-panel="appearance"] .panel-card__foot .btn--primary, [data-tab-panel="appearance"] button[data-toast*="已保存"]');
		if (saveBtn) {
			saveBtn.addEventListener('click', function () {
				toast('success', '外观设置已保存', 2400);
				log('外观设置已保存', '主题偏好 + 配色模式 + 密度 + 动效', 'success', 'save');
			});
		}
	});

	/* --- Flow 6: report timeline --- */
	Forum.flows.register('reportTimeline', function () {
		function appendTimeline(action, danger, reason) {
			var tl = qs('.mod-timeline');
			if (!tl) return;
			var item = document.createElement('div');
			item.className = 'mod-timeline__item';
			item.innerHTML =
				'<div class="mod-timeline__time">' + nowStr() + ' · 刚刚</div>' +
				'<p class="mod-timeline__title">处理：' + escHtml(action) + (danger ? ' <span class="status-badge danger" style="margin-left:.4em;font-size:0.7rem">高风险</span>' : '') + '</p>' +
				'<div class="mod-timeline__body">操作原因：' + escHtml(reason) + ' · 审计已记录</div>';
			tl.appendChild(item);
		}
		function setStatus(text, variant) {
			var sb = qs('.page-head__lede .status-badge');
			if (!sb) return;
			sb.textContent = text;
			sb.className = 'status-badge ' + (variant || 'resolved');
		}
		function setActionsDisabled() {
			qsa('.page-head__actions .btn, .side-card__list .btn').forEach(function (b) {
				b.disabled = true;
				b.setAttribute('aria-disabled', 'true');
				b.style.opacity = .5;
			});
		}
		document.addEventListener('click', function (e) {
			var btn = e.target.closest('[data-confirm-reason]');
			if (!btn) return;
			var inTop = btn.closest('.page-head__actions');
			var inSide = btn.closest('.side-card__list');
			if (!inTop && !inSide) return;
			var action = btn.textContent.trim();
			var danger = btn.hasAttribute('data-confirm-danger');
			var tries = 0;
			var iv = setInterval(function () {
				var ok = document.querySelector('[data-modal-confirm-ok]');
				tries++;
				if (ok && !ok.__mod) {
					ok.__mod = true;
					ok.addEventListener('click', function () {
						var ta = document.querySelector('[data-confirm-reason-input]');
						var reason = (ta && ta.value || '').trim() || '已通过默认检查';
						setTimeout(function () {
							appendTimeline(action, danger, reason);
							setStatus('已处理', 'resolved');
							setActionsDisabled();
							toast('success', '处理完成 · 审计已记录', 3000);
							log('举报处理 · ' + action, reason, danger ? 'danger' : 'success', 'shield');
						}, 100);
					});
					clearInterval(iv);
				}
				if (tries > 40) clearInterval(iv);
			}, 50);
		});
	});

	/* --- Flow 7: points adjust --- */
	Forum.flows.register('pointsAdjust', function () {
		var SEED = {
			Chaos: { coin: 328, exp: 2680, contrib: 146 },
			Yuwen: { coin: 1200, exp: 4820, contrib: 312 },
			Mark:  { coin: 124, exp: 820, contrib: 58 },
			Alice: { coin: 14, exp: 240, contrib: 22 },
			Reo:   { coin: 28, exp: 180, contrib: 14 },
			Nina:  { coin: 412, exp: 1620, contrib: 88 }
		};
		var TONE = { Chaos: 2, Yuwen: 5, Mark: 3, Alice: 1, Reo: 7, Nina: 4 };
		function findUser(name) {
			var k = (name || '').trim();
			if (!k) return null;
			for (var n in SEED) if (n.toLowerCase().indexOf(k.toLowerCase()) === 0) return { name: n, data: SEED[n] };
			return null;
		}
		function badge(tone, ch, name) {
			return '<span class="user-badge"><span class="avatar xs tone-' + tone + '">' + ch + '</span><span>' + name + '</span></span>';
		}
		var result = qs('#points-result');
		function renderResult(u) {
			if (!result) return;
			if (!u) { result.innerHTML = '<p class="muted">未找到用户</p>'; return; }
			result.innerHTML =
				'<table class="admin-table">' +
				'<thead><tr><th>用户</th><th class="col-num">B 币</th><th class="col-num">经验</th><th class="col-num">贡献</th><th class="col-narrow">操作</th></tr></thead>' +
				'<tbody><tr>' +
				'<td>' + badge(TONE[u.name] || 1, u.name[0], u.name) + '</td>' +
				'<td class="col-num"><strong data-coin>' + u.data.coin + '</strong></td>' +
				'<td class="col-num"><strong data-exp>' + u.data.exp + '</strong></td>' +
				'<td class="col-num"><strong data-contrib>' + u.data.contrib + '</strong></td>' +
				'<td><button class="btn btn--primary btn--sm" data-adjust="' + escHtml(u.name) + '"><span data-icon="settings" data-size="13"></span><span>调整</span></button></td>' +
				'</tr></tbody></table>';
			Forum.init && Forum.init();
		}
		var queryBtn = qs('[data-points-query]');
		if (queryBtn) queryBtn.addEventListener('click', function () { renderResult(findUser(qs('#points-query').value)); });
		var queryInput = qs('#points-query');
		if (queryInput) queryInput.addEventListener('keydown', function (e) { if (e.key === 'Enter') { e.preventDefault(); renderResult(findUser(e.target.value)); } });
		document.addEventListener('click', function (e) {
			var btn = e.target.closest('[data-adjust]');
			if (!btn) return;
			var u = btn.getAttribute('data-adjust');
			var data = SEED[u];
			if (!data) return;
			Forum.modal.open('adjust-points');
			setTimeout(function () {
				var bd = document.querySelector('.modal-backdrop .modal');
				if (!bd) return;
				bd.querySelector('[data-adjust-user]').textContent = u;
				bd.querySelector('[data-adjust-coin]').textContent = data.coin;
				bd.querySelector('[data-adjust-exp]').textContent = data.exp;
				bd.querySelector('[data-adjust-contrib]').textContent = data.contrib;
				var okBtn = document.createElement('button');
				okBtn.className = 'btn btn--primary btn--sm';
				okBtn.textContent = '应用调整';
				okBtn.style.marginLeft = 'auto';
				bd.querySelector('.modal__foot').appendChild(okBtn);
				okBtn.addEventListener('click', function () {
					var reason = bd.querySelector('[data-adjust-reason]').value.trim();
					if (!reason) { bd.querySelector('[data-adjust-reason]').focus(); toast('warn', '请填写原因', 2400); return; }
					var kind = bd.querySelector('[data-adjust-kind]').value;
					var dir = bd.querySelector('[data-adjust-dir]').value;
					var amount = parseInt(bd.querySelector('[data-adjust-amount]').value, 10) || 0;
					if (amount <= 0) { toast('warn', '数额必须大于 0', 2400); return; }
					data[kind] = Math.max(0, data[kind] + (dir === '+' ? amount : -amount));
					var sign = dir === '+' ? '+' : '-';
					var ledger = qs('#points-ledger');
					var tr = document.createElement('tr');
					tr.innerHTML =
						'<td>刚刚</td>' +
						'<td>' + badge(TONE[u] || 1, u[0], u) + '</td>' +
						'<td><span class="badge solid">管理员调整</span></td>' +
						'<td class="col-num">' + sign + amount + '</td>' +
						'<td>' + data[kind] + '</td>' +
						'<td>' + escHtml(reason) + '</td>';
					if (ledger) ledger.insertBefore(tr, ledger.firstChild);
					var sel = '#points-result [data-' + kind + ']';
					var r = qs(sel);
					if (r) r.textContent = data[kind];
					bd.parentNode.parentNode.removeChild(bd.parentNode);
					toast('success', '已调整 · 流水已追加', 3000);
					log('积分调整 · ' + u + ' ' + sign + amount + ' ' + kind, reason, 'info', 'coins');
				});
			}, 50);
		});
		renderResult({ name: 'Chaos', data: SEED.Chaos });
	});

	/* --- Flow 8: OAuth create --- */
	Forum.flows.register('oauthCreate', function () {
		function genId() { return 'cli_' + Math.random().toString(36).slice(2, 10) + '_' + Date.now().toString(36); }
		function genSecret() {
			var a = 'abcdefghijklmnopqrstuvwxyz0123456789';
			var s = 'cs_';
			for (var i = 0; i < 32; i++) s += a[Math.floor(Math.random() * a.length)];
			return s;
		}
		function newRow(name, clientId, scopes, type) {
			var tbody = document.querySelector('.admin-table tbody');
			if (!tbody) return;
			var tr = document.createElement('tr');
			tr.innerHTML =
				'<td><strong>' + escHtml(name) + '</strong><br /><span class="muted" style="font-size:0.78rem">' + escHtml(type) + ' · 刚刚创建</span></td>' +
				'<td><code>' + escHtml(clientId.slice(0, 12)) + '_***</code></td>' +
				'<td>' + escHtml(scopes) + '</td>' +
				'<td><span class="status-badge pending">待审批</span></td>' +
				'<td>未使用</td>' +
				'<td><div class="popover-anchor">' +
					'<button class="dot-menu" data-popover aria-label="更多操作"><span data-icon="more-vertical" data-size="14"></span></button>' +
					'<div class="popover-panel">' +
						'<button class="popover-item"><span data-icon="check" data-size="13"></span><span class="popover-item-label">通过</span></button>' +
						'<button class="popover-item danger"><span data-icon="x" data-size="13"></span><span class="popover-item-label">驳回</span></button>' +
					'</div>' +
				'</div></td>';
			tbody.insertBefore(tr, tbody.firstChild);
			var t = qsa('.muted').find(function (el) { return el.textContent.indexOf('个 ·') === 0; });
			if (t) {
				var rows = document.querySelectorAll('.admin-table tbody tr').length;
				var active = document.querySelectorAll('.admin-table tbody .status-badge.resolved').length;
				t.textContent = rows + ' 个 · ' + active + ' 个活跃';
			}
			var pageLede = qs('.page-head__lede');
			if (pageLede) {
				var m = pageLede.textContent.match(/(\d+) 个客户端/);
				if (m) pageLede.textContent = pageLede.textContent.replace(/(\d+) 个客户端/, (parseInt(m[1], 10) + 1) + ' 个客户端');
			}
			Forum.init && Forum.init();
		}
		document.addEventListener('click', function (e) {
			var opener = e.target.closest('[data-modal="new-client-modal"]');
			if (!opener) return;
			setTimeout(function () {
				var bd = document.querySelector('.modal-backdrop .modal');
				if (!bd) return;
				var foot = bd.querySelector('.modal__foot');
				var defBtn = foot.querySelector('[data-modal-close]');
				if (defBtn) defBtn.remove();
				var cancel = document.createElement('button');
				cancel.className = 'btn btn--secondary btn--sm';
				cancel.textContent = '取消';
				cancel.addEventListener('click', function () { bd.parentNode.parentNode.removeChild(bd.parentNode); });
				var create = document.createElement('button');
				create.className = 'btn btn--primary btn--sm';
				create.textContent = '创建';
				foot.appendChild(cancel);
				foot.appendChild(create);
				create.addEventListener('click', function () {
					var name = bd.querySelector('#oauth-name').value.trim();
					if (!name) { bd.querySelector('#oauth-name').focus(); toast('warn', '请填写名称', 2400); return; }
					var scopes = qsa('.checkbox input:checked + span', bd).map(function (s) { return s.textContent.trim(); }).join(' · ');
					if (!scopes) { toast('warn', '至少勾选 1 个 scope', 2400); return; }
					var type = bd.querySelector('#oauth-type').value;
					var clientId = genId();
					var secret = genSecret();
					bd.parentNode.parentNode.removeChild(bd.parentNode);
					newRow(name, clientId, scopes, type);
					toast('info', '已创建 · 显示凭据', 2000);
					setTimeout(function () {
						Forum.modal.open('oauth-success');
						setTimeout(function () {
							var sbd = document.querySelector('.modal-backdrop .modal');
							if (!sbd) return;
							sbd.querySelector('#oauth-success-id').value = clientId;
							sbd.querySelector('#oauth-success-secret').value = secret;
							var sfoot = sbd.querySelector('.modal__foot');
							var def = sfoot.querySelector('[data-modal-close]');
							if (def) def.textContent = '我已保存';
						}, 50);
					}, 200);
					log('OAuth 创建客户端 · ' + name, 'scopes: ' + scopes, 'success', 'shield');
				});
			}, 50);
		});
	});

	/* --- Flow 9: themes (activate + preview + upload story-A) --- */
	Forum.flows.register('themes', function () {
		var ACTIVE_KEY = 'chaos:active-theme';
		var TOKEN_KEY = 'chaos:applied-theme-tokens';
		var ISOLATED_KEY = 'chaos:uploaded-themes';
		var uploaded = (lsGet(ISOLATED_KEY) || '').split('|').filter(Boolean);
		var grid = qs('.theme-grid');
		function renderUploaded() {
			if (!grid) return;
			uploaded.forEach(function (id) {
				if (qs('[data-theme-id="' + id + '"]')) return;
				var card = document.createElement('div');
				card.className = 'theme-card';
				card.setAttribute('data-theme-id', id);
				card.setAttribute('data-theme-name', '上传主题 ' + id);
				card.setAttribute('data-isolated', '1');
				card.innerHTML =
					'<div class="theme-card__preview"><div class="theme-card__preview-bar" style="background:#3a3a5a"></div>' +
					'<div class="theme-card__preview-body"><div class="theme-card__preview-line mid"></div><div class="theme-card__preview-line"></div><div class="theme-card__preview-line short"></div></div></div>' +
					'<div class="row gap-2" style="align-items:flex-start"><div class="col gap-1" style="flex:1;min-width:0">' +
					'<div class="theme-card__name">上传主题 ' + escHtml(id) + '</div>' +
					'<div class="theme-card__meta">自定义 · 隔离态</div></div>' +
					'<span class="badge" style="border-color:var(--warn);color:var(--warn)">隔离</span></div>' +
					'<div class="theme-card__mode"><div class="field" style="margin:0"><label class="field__label" style="font-size:0.78rem">配色模式</label>' +
					'<div class="row gap-4" style="gap:16px"><label class="radio radio--sm"><input type="radio" name="' + id + '-mode" checked /><span>浅色</span></label>' +
					'<label class="radio radio--sm"><input type="radio" name="' + id + '-mode" /><span>跟随系统</span></label>' +
					'<label class="radio radio--sm"><input type="radio" name="' + id + '-mode" /><span>深色</span></label></div></div></div>' +
					'<div class="theme-card__foot"><button type="button" class="btn btn--primary btn--sm" data-confirm-reason="隔离态主题将被扫描 schema/包大小/危险函数后启用。启用时自动停用当前主题。" data-confirm-title="启用主题"><span data-icon="check" data-size="12"></span><span>启用</span></button>' +
					'<button type="button" class="btn btn--ghost btn--sm"><span data-icon="eye" data-size="12"></span><span>预览</span></button>' +
					'<span class="spacer"></span><div class="popover-anchor">' +
					'<button type="button" class="dot-menu" data-popover aria-label="更多操作"><span data-icon="more-vertical" data-size="15"></span></button>' +
					'<div class="popover-panel">' +
					'<button class="popover-item"><span data-icon="edit-3" data-size="13"></span><span class="popover-item-label">编辑 Token</span></button>' +
					'<button class="popover-item danger" data-confirm-reason="隔离态主题未发布前可直接删除。" data-confirm-title="删除主题" data-confirm-danger><span data-icon="trash-2" data-size="13"></span><span class="popover-item-label">删除</span></button>' +
					'</div></div></div>';
				var corrupt = qs('[data-theme-id="broken"]');
				if (corrupt) grid.insertBefore(card, corrupt);
				else grid.appendChild(card);
				// wire confirm buttons in the new card
				qsa('[data-confirm-reason]', card).forEach(function (btn) { wireConfirm(btn, id, '上传主题 ' + id); });
				// wire preview
				var foot = card.querySelector('.theme-card__foot');
				if (foot) {
					foot.querySelectorAll('button').forEach(function (b) {
						if (b.textContent.indexOf('预览') >= 0) {
							b.addEventListener('click', function () {
								var prev = lsGet(TOKEN_KEY) || 'default';
								applyTokens(id);
								Forum.toast('info', '已临时预览「上传主题 ' + id + '」', 2400);
								setTimeout(function () { applyTokens(prev); }, 2500);
							});
						}
					});
				}
			});
		}
		renderUploaded();
		var newBtn = qsa('.page-head__actions button').find(function (b) { return b.textContent.indexOf('新建主题') >= 0; });
		if (newBtn) {
			newBtn.removeAttribute('data-toast');
			newBtn.setAttribute('data-drawer-open', 'upload-theme');
		}
		var uploadBtn = qsa('[data-drawer="upload-theme"] .drawer__foot .btn--primary')[0];
		if (uploadBtn) {
			uploadBtn.addEventListener('click', function () {
				var drawer = qs('[data-drawer="upload-theme"]');
				if (!drawer) return;
				var nameInput = drawer.querySelectorAll('input.input')[0];
				var name = (nameInput && nameInput.value || '').trim();
				if (!name) { if (nameInput) nameInput.focus(); toast('warn', '请填写主题名', 2400); return; }
				var id = name.toLowerCase().replace(/[^a-z0-9-]/g, '-').slice(0, 32);
				if (uploaded.indexOf(id) < 0) uploaded.push(id);
				lsSet(ISOLATED_KEY, uploaded.join('|'));
				setTimeout(function () {
					renderUploaded();
					Forum.init && Forum.init();
					toast('success', '已上传 · 隔离态', 2400);
					log('上传主题 · ' + id, '隔离态，待启用', 'info', 'upload');
				}, 100);
			});
		}
		function applyActive(id, name) {
			qsa('.theme-card').forEach(function (c) {
				c.classList.remove('active');
				var oldBadge = c.querySelector('.badge.solid');
				if (oldBadge && oldBadge.textContent.indexOf('已启用') === 0) oldBadge.outerHTML = '<span class="badge">已停用</span>';
			});
			var card = qs('[data-theme-id="' + id + '"]');
			if (!card) return;
			card.classList.add('active');
			var existing = card.querySelector('.badge');
			if (existing) existing.outerHTML = '<span class="badge solid">已启用</span>';
			lsSet(ACTIVE_KEY, id);
			qsa('.theme-card:not(.active)').forEach(function (c) {
				var foot = c.querySelector('.theme-card__foot');
				if (!foot) return;
				var b = foot.querySelector('.btn--secondary[disabled][title*="当前生效"]');
				if (b) {
					b.outerHTML = '<button type="button" class="btn btn--primary btn--sm" data-confirm-reason="启用此主题将自动停用当前「' + name + '」主题，切换站点默认。" data-confirm-title="启用主题"><span data-icon="check" data-size="12"></span><span>启用</span></button>';
					var nb = foot.querySelector('.btn--primary');
					if (nb) wireConfirm(nb, c.dataset.themeId, c.dataset.themeName);
				}
				var ab = c.querySelector('.btn--secondary[disabled]');
				if (ab && ab.textContent.indexOf('已启用') >= 0) {
					ab.outerHTML = '<button type="button" class="btn btn--primary btn--sm" data-confirm-reason="启用此主题将自动停用当前「' + name + '」主题，切换站点默认。" data-confirm-title="启用主题"><span data-icon="check" data-size="12"></span><span>启用</span></button>';
				}
			});
			toast('success', '已启用「' + name + '」· 全站立即生效', 3000);
			log('主题切换 · ' + name, '全站立即生效', 'success', 'palette');
		}
		function applyTokens(id) {
			var tokens = THEME_TOKENS[id];
			if (tokens === undefined) {
				// 上传/隔离态主题：用一套可辨识的预览 token 演示全站投影
				tokens = THEME_TOKENS['warm'];
			}
			applyThemeTokens(tokens);
			lsSet(TOKEN_KEY, id || 'default');
		}
		function wireConfirm(btn, id, name) {
			btn.addEventListener('click', function () {
				var tries = 0;
				var iv = setInterval(function () {
					var ok = document.querySelector('[data-modal-confirm-ok]');
					tries++;
					if (ok && !ok.__themed) {
						ok.__themed = true;
						ok.addEventListener('click', function () {
							setTimeout(function () { applyActive(id, name); applyTokens(id); }, 100);
						});
						clearInterval(iv);
					}
					if (tries > 40) clearInterval(iv);
				}, 50);
			});
		}
		qsa('[data-confirm-reason]').forEach(function (btn) {
			var card = btn.closest('.theme-card');
			if (!card) return;
			var id = card.dataset.themeId;
			var name = card.dataset.themeName;
			if (!id) return;
			wireConfirm(btn, id, name);
		});
		qsa('.theme-card__foot').forEach(function (foot) {
			var card = foot.closest('.theme-card');
			var id = card.dataset.themeId;
			var name = card.dataset.themeName;
			if (!id || id === 'broken') return;
			foot.querySelectorAll('button').forEach(function (b) {
				if (b.textContent.indexOf('预览') >= 0) {
					b.addEventListener('click', function () {
						var prev = lsGet(TOKEN_KEY) || 'default';
						applyTokens(id);
						Forum.toast('info', '已临时预览「' + name + '」', 2400);
						setTimeout(function () { applyTokens(prev); }, 2500);
					});
				}
			});
		});
		try {
			var id = lsGet(ACTIVE_KEY);
			if (id) {
				var card = qs('[data-theme-id="' + id + '"]');
				if (card) { applyActive(id, card.dataset.themeName || ''); applyTokens(id); }
			}
		} catch (e) {}
	});

	/* --- dispatcher --- */
	var ROUTES = {
		'topic-101.html': 'favorite',
		'topic-201.html': 'replyUnlock',
		'topic-202.html': 'paidUnlock',
		'admin-reports-r-1024.html': 'reportTimeline',
		'admin-points.html': 'pointsAdjust',
		'admin-oauth.html': 'oauthCreate',
		'admin-themes.html': 'themes',
		'favorites.html': 'favorites',
		'settings.html': ['devices', 'settingsAppearance']
	};
	var path = location.pathname.split('/').pop().toLowerCase();
	var flowName = ROUTES[path];
	if (Array.isArray(flowName)) {
		flowName.forEach(function (f) { Forum.flows.run(f); });
	} else if (flowName) {
		Forum.flows.run(flowName);
	}
})();
