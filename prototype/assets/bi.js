/* ============================================================================
 * BI Dashboard — data + charts + interactions
 * All data is mock-generated on load. Charts are pure CSS/SVG, no deps.
 * ============================================================================
 */
(function () {
	'use strict';
	if (!window.Forum) return;

	var RANGE = 30;
	var BOARD = '';
	var USER_TYPE = '';
	var SEED = 42;

	/* --- PRNG (seeded, so refresh is stable until manual refresh) --- */
	function srand() {
		SEED = (SEED * 9301 + 49297) % 233280;
		return SEED / 233280;
	}
	function rand(min, max) { return min + srand() * (max - min); }
	function randInt(min, max) { return Math.floor(rand(min, max + 1)); }
	function pick(arr) { return arr[Math.floor(srand() * arr.length)]; }

	/* --- Data generation --- */
	function genTimeSeries(days, baseMin, baseMax, variance) {
		var arr = [];
		var trend = (baseMax - baseMin) / days * 0.3;
		for (var i = 0; i < days; i++) {
			var base = baseMin + trend * i + (srand() - 0.5) * variance;
			arr.push(Math.max(0, Math.round(base)));
		}
		return arr;
	}

	var DATA = {
		users: {
			dau: genTimeSeries(90, 800, 1450, 200),
			mau: genTimeSeries(90, 9200, 11800, 600),
			registers: genTimeSeries(90, 12, 48, 15),
			retention: genTimeSeries(90, 62, 78, 8),
			levels: [
				{ label: 'LV.1-3', value: 4820, color: 'var(--info)' },
				{ label: 'LV.4-6', value: 5240, color: 'var(--ok)' },
				{ label: 'LV.7-9', value: 1840, color: 'var(--warn)' },
				{ label: 'LV.10+', value: 312, color: 'var(--bad)' }
			]
		},
		content: {
			boards: [
				{ label: 'rust', value: 486 },
				{ label: 'web-dev', value: 412 },
				{ label: 'chat', value: 318 },
				{ label: 'essay', value: 254 },
				{ label: 'opensource', value: 198 },
				{ label: 'meta', value: 86 }
			],
			tags: [
				{ label: 'Rust', count: 412 },
				{ label: 'SvelteKit', count: 318 },
				{ label: 'SQLite', count: 246 },
				{ label: 'OAuth', count: 184 },
				{ label: 'axum', count: 168 },
				{ label: '自托管', count: 142 },
				{ label: '性能优化', count: 128 },
				{ label: 'SSR', count: 112 },
				{ label: 'TypeScript', count: 98 },
				{ label: 'Docker', count: 86 },
				{ label: 'WebAssembly', count: 64 },
				{ label: 'Linux', count: 52 }
			],
			featuredRate: 8.4,
			totalTopics: 8420,
			featured: 708
		},
		economy: {
			coinOut: genTimeSeries(90, 2800, 4200, 600),
			coinIn: genTimeSeries(90, 1800, 3200, 500),
			checkin: genTimeSeries(90, 62, 88, 10),
			shop: [
				{ label: '头像框', value: 4820, color: 'var(--info)' },
				{ label: '主题装饰', value: 3210, color: 'var(--ok)' },
				{ label: '徽章', value: 2180, color: 'var(--warn)' },
				{ label: '表情包', value: 1640, color: 'var(--bad)' },
				{ label: '其他', value: 920, color: 'var(--line-strong)' }
			]
		},
		moderation: {
			reports: genTimeSeries(90, 8, 42, 12),
			penalties: [
				{ label: '警告', value: 184, color: 'var(--info)' },
				{ label: '禁言 24h', value: 92, color: 'var(--warn)' },
				{ label: '禁言 7d', value: 38, color: 'var(--bad)' },
				{ label: '封禁', value: 14, color: 'var(--ink)' }
			],
			handleTime: genTimeSeries(90, 2, 8, 3)
		},
		achievements: {
			funnel: [
				{ label: '展示', value: 12480, color: 'var(--info)' },
				{ label: '点击', value: 8240, color: 'var(--info)' },
				{ label: '推进', value: 5180, color: 'var(--ok)' },
				{ label: '解锁', value: 2341, color: 'var(--warn)' },
				{ label: '装备', value: 1648, color: 'var(--bad)' }
			],
			equipRate: 70.4,
			equipTotal: 1648,
			equipUnlocked: 2341,
			tiers: [
				{ label: '铜', value: 1240, color: '#f3d6c2' },
				{ label: '银', value: 682, color: '#e1e4e8' },
				{ label: '金', value: 318, color: '#f8e6a6' },
				{ label: '传奇', value: 101, color: 'var(--warn)' }
			],
			detail: [
				{ name: 'first_like', tier: '铜', unlocked: 1204, equipped: 612 },
				{ name: 'featured_once', tier: '银', unlocked: 186, equipped: 142 },
				{ name: 'level_8', tier: '金', unlocked: 42, equipped: 38 },
				{ name: 'anniversary_1y', tier: '传奇', unlocked: 7, equipped: 7 },
				{ name: 'first_topic', tier: '铜', unlocked: 980, equipped: 410 },
				{ name: 'chatter', tier: '银', unlocked: 312, equipped: 180 }
			]
		},
		marketplace: {
			themeSwitch: genTimeSeries(90, 12, 48, 15),
			plugins: [
				{ label: 'markdown-extended', value: 1840 },
				{ label: 'code-highlight', value: 1620 },
				{ label: 'image-gallery', value: 1280 },
				{ label: 'video-embed', value: 920 },
				{ label: 'latex-math', value: 680 },
				{ label: 'mention-notif', value: 540 }
			],
			oauth: [
				{ label: 'yuwen.dev blog', value: 1240, color: 'var(--info)' },
				{ label: 'rss-reader-bot', value: 680, color: 'var(--ok)' },
				{ label: 'mobile-app', value: 420, color: 'var(--warn)' },
				{ label: 'other', value: 180, color: 'var(--line-strong)' }
			]
		}
	};

	/* --- Helpers --- */
	function el(tag, cls, html) {
		var e = document.createElement(tag);
		if (cls) e.className = cls;
		if (html != null) e.innerHTML = html;
		return e;
	}
	function qs(sel, root) { return (root || document).querySelector(sel); }
	function qsa(sel, root) { return Array.prototype.slice.call((root || document).querySelectorAll(sel)); }
	function fmt(n) {
		if (n >= 10000) return (n / 1000).toFixed(1) + 'k';
		return String(n);
	}
	function pct(n) { return Math.round(n * 10) / 10 + '%'; }
	function esc(s) { return String(s).replace(/[&<>"']/g, function (c) { return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]; }); }

	/* --- Chart renderers --- */
	function slice(arr, n) { return arr.slice(-n); }

	function renderBar(container, data, opts) {
		opts = opts || {};
		var max = Math.max.apply(null, data) * 1.15 || 1;
		var bar = el('div', 'bi-bar');
		data.forEach(function (v, i) {
			var col = el('div', 'bi-bar__col' + (opts.alt && opts.alt(i) ? ' bi-bar__col--alt' : ''));
			col.style.height = (v / max * 100) + '%';
			col.setAttribute('data-value', fmt(v));
			bar.appendChild(col);
		});
		container.innerHTML = '';
		container.appendChild(bar);
		if (opts.axis) {
			var axis = el('div', 'bi-bar__axis');
			axis.innerHTML = '<span>' + opts.axis[0] + '</span><span>' + opts.axis[1] + '</span>';
			bar.appendChild(axis);
		}
	}

	function renderHBar(container, items, opts) {
		opts = opts || {};
		var max = Math.max.apply(null, items.map(function (i) { return i.value; })) * 1.1 || 1;
		var wrap = el('div', 'bi-hbar');
		items.forEach(function (item) {
			var row = el('div', 'bi-hbar__row');
			row.innerHTML =
				'<span class="bi-hbar__label" title="' + esc(item.label) + '">' + esc(item.label) + '</span>' +
				'<div class="bi-hbar__track"><div class="bi-hbar__fill' + (item.color ? '' : '') + '" style="width:' + (item.value / max * 100) + '%; background:' + (item.color || 'var(--info)') + '"></div></div>' +
				'<span class="bi-hbar__value">' + fmt(item.value) + '</span>';
			wrap.appendChild(row);
		});
		container.innerHTML = '';
		container.appendChild(wrap);
	}

	function renderLine(container, series, opts) {
		opts = opts || {};
		var w = 280, h = 160, pad = 8;
		var all = series.reduce(function (a, s) { return a.concat(s.data); }, []);
		var max = Math.max.apply(null, all) * 1.1 || 1;
		var min = Math.min.apply(null, all) * 0.9;
		if (min > 0) min = 0;
		var n = series[0].data.length;
		var stepX = (w - pad * 2) / Math.max(1, n - 1);
		function toPath(data) {
			return data.map(function (v, i) {
				var x = pad + i * stepX;
				var y = h - pad - (v - min) / (max - min || 1) * (h - pad * 2);
				return (i === 0 ? 'M' : 'L') + x.toFixed(1) + ',' + y.toFixed(1);
			}).join(' ');
		}
		function toArea(data) {
			var path = toPath(data);
			var lastX = pad + (n - 1) * stepX;
			return path + ' L' + lastX.toFixed(1) + ',' + (h - pad) + ' L' + pad + ',' + (h - pad) + ' Z';
		}
		var svg = '<svg class="bi-line" viewBox="0 0 ' + w + ' ' + h + '" preserveAspectRatio="none">';
		// grid lines
		for (var g = 0; g < 4; g++) {
			var y = pad + (h - pad * 2) * g / 3;
			svg += '<line class="bi-line__grid" x1="' + pad + '" x2="' + (w - pad) + '" y1="' + y + '" y2="' + y + '"/>';
		}
		series.forEach(function (s) {
			if (s.area !== false) svg += '<path class="bi-line__area" d="' + toArea(s.data) + '"/>';
			svg += '<path class="bi-line__path' + (s.alt ? ' bi-line__path--alt' : '') + '" d="' + toPath(s.data) + '"/>';
		});
		// dots on last point
		series.forEach(function (s) {
			var lastVal = s.data[s.data.length - 1];
			var x = pad + (n - 1) * stepX;
			var y = h - pad - (lastVal - min) / (max - min || 1) * (h - pad * 2);
			svg += '<circle class="bi-line__dot" cx="' + x.toFixed(1) + '" cy="' + y.toFixed(1) + '" r="3"/>';
		});
		svg += '</svg>';
		container.innerHTML = svg;
		// axis
		var axis = el('div', 'bi-bar__axis');
		axis.innerHTML = '<span>' + (opts.axis ? opts.axis[0] : '') + '</span><span>' + (opts.axis ? opts.axis[1] : '') + '</span>';
		container.appendChild(axis);
	}

	function renderDonut(container, items, opts) {
		opts = opts || {};
		var total = items.reduce(function (a, i) { return a + i.value; }, 0) || 1;
		var cx = 60, cy = 60, r = 48, ir = 32;
		var svg = '<svg class="bi-donut__svg" width="120" height="120" viewBox="0 0 120 120">';
		var angle = -Math.PI / 2;
		items.forEach(function (item) {
			var frac = item.value / total;
			var end = angle + frac * Math.PI * 2;
			var large = frac > 0.5 ? 1 : 0;
			var x1 = cx + r * Math.cos(angle), y1 = cy + r * Math.sin(angle);
			var x2 = cx + r * Math.cos(end), y2 = cy + r * Math.sin(end);
			var xi1 = cx + ir * Math.cos(angle), yi1 = cy + ir * Math.sin(angle);
			var xi2 = cx + ir * Math.cos(end), yi2 = cy + ir * Math.sin(end);
			var d = 'M' + x1 + ',' + y1 + ' A' + r + ',' + r + ' 0 ' + large + ' 1 ' + x2 + ',' + y2 +
				' L' + xi2 + ',' + yi2 + ' A' + ir + ',' + ir + ' 0 ' + large + ' 0 ' + xi1 + ',' + yi1 + ' Z';
			svg += '<path d="' + d + '" fill="' + item.color + '" stroke="var(--surface)" stroke-width="1"/>';
			angle = end;
		});
		if (opts.centerLabel) {
			svg += '<text class="bi-donut__center" x="' + cx + '" y="' + (cy - 4) + '">' + esc(opts.centerLabel) + '</text>';
			svg += '<text class="bi-donut__sub" x="' + cx + '" y="' + (cy + 12) + '">' + esc(opts.centerSub || '') + '</text>';
		}
		svg += '</svg>';
		var legend = '<div class="bi-donut__legend">';
		items.forEach(function (item) {
			legend += '<div class="bi-donut__legend-item">' +
				'<span class="bi-donut__legend-dot" style="background:' + item.color + '"></span>' +
				'<span class="bi-donut__legend-label">' + esc(item.label) + '</span>' +
				'<span class="bi-donut__legend-value">' + fmt(item.value) + (opts.showPct ? ' · ' + pct(item.value / total * 100) : '') + '</span>' +
			'</div>';
		});
		legend += '</div>';
		container.innerHTML = '<div class="bi-donut">' + svg + legend + '</div>';
	}

	function renderFunnel(container, stages) {
		var max = stages[0].value || 1;
		var wrap = el('div', 'bi-funnel');
		stages.forEach(function (stage, i) {
			var div = el('div', 'bi-funnel__stage');
			div.style.background = stage.color;
			div.style.marginLeft = (i * 12) + 'px';
			div.style.marginRight = (i * 12) + 'px';
			var pctVal = i === 0 ? 100 : Math.round(stage.value / max * 100);
			div.innerHTML =
				'<span class="bi-funnel__stage-label">' + esc(stage.label) + '</span>' +
				'<span><span class="bi-funnel__stage-value">' + fmt(stage.value) + '</span>' +
				'<span class="bi-funnel__stage-pct">' + pctVal + '%</span></span>';
			wrap.appendChild(div);
		});
		container.innerHTML = '';
		container.appendChild(wrap);
	}

	function renderTagCloud(container, tags) {
		var max = Math.max.apply(null, tags.map(function (t) { return t.count; })) || 1;
		var wrap = el('div', 'bi-tagcloud');
		tags.forEach(function (t) {
			var size = 0.75 + (t.count / max) * 0.5;
			var a = el('a', null, '<span class="hash">#</span>' + esc(t.label));
			a.href = '#';
			a.style.fontSize = size + 'rem';
			a.title = t.count + ' 个主题';
			wrap.appendChild(a);
		});
		container.innerHTML = '';
		container.appendChild(wrap);
	}

	function renderSparkline(data, down) {
		var w = 80, h = 20, pad = 2;
		var max = Math.max.apply(null, data) || 1;
		var min = Math.min.apply(null, data);
		var stepX = (w - pad * 2) / Math.max(1, data.length - 1);
		var pts = data.map(function (v, i) {
			var x = pad + i * stepX;
			var y = h - pad - (v - min) / (max - min || 1) * (h - pad * 2);
			return (i === 0 ? 'M' : 'L') + x.toFixed(1) + ',' + y.toFixed(1);
		}).join(' ');
		var areaPts = pts + ' L' + (pad + (data.length - 1) * stepX) + ',' + (h - pad) + ' L' + pad + ',' + (h - pad) + ' Z';
		return '<svg class="bi-spark" viewBox="0 0 ' + w + ' ' + h + '">' +
			'<path class="bi-spark__area" d="' + areaPts + '"/>' +
			'<path class="bi-spark__path' + (down ? ' bi-spark__path--down' : '') + '" d="' + pts + '"/>' +
			'</svg>';
	}

	/* --- KPIs --- */
	function renderKPIs() {
		var container = qs('[data-bi-kpis]');
		if (!container) return;
		var kpis = [
			{ label: '总用户', value: 12212, delta: '+184', dir: 'up' },
			{ label: 'DAU', value: DATA.users.dau[DATA.users.dau.length - 1], delta: '+5.2%', dir: 'up' },
			{ label: '总主题', value: 8420, delta: '+42', dir: 'up' },
			{ label: '流通 B 币', value: 284128, delta: '+1,820', dir: 'up' },
			{ label: '待处理举报', value: 12, delta: '-3', dir: 'down' },
			{ label: '签到率', value: pct(DATA.economy.checkin[DATA.economy.checkin.length - 1]), delta: '+1.4%', dir: 'up' }
		];
		container.innerHTML = '';
		kpis.forEach(function (k) {
			var div = el('div', 'bi-kpi');
			var deltaIcon = k.dir === 'up' ? '↑' : (k.dir === 'down' ? '↓' : '→');
			div.innerHTML =
				'<div class="bi-kpi__label">' + esc(k.label) + '</div>' +
				'<div class="bi-kpi__value">' + fmt(k.value) + '</div>' +
				'<div class="bi-kpi__delta bi-kpi__delta--' + k.dir + '">' + deltaIcon + ' ' + k.delta + ' · 近 ' + RANGE + ' 天</div>';
			container.appendChild(div);
		});
	}

	/* --- Section renderers --- */
	function renderAll() {
		renderKPIs();
		// 1. Users
		renderLine(qs('[data-bi-chart="dau-line"]'), [
			{ data: slice(DATA.users.dau, RANGE) },
			{ data: slice(DATA.users.mau, RANGE).map(function (v) { return v / 30; }), alt: true, area: false }
		], { axis: [RANGE + ' 天前', '今天'] });
		renderBar(qs('[data-bi-chart="register-bar"]'), slice(DATA.users.registers, RANGE));
		renderDonut(qs('[data-bi-chart="level-donut"]'), DATA.users.levels, { centerLabel: fmt(12212), centerSub: '总用户', showPct: true });

		// 2. Content
		renderHBar(qs('[data-bi-chart="board-bar"]'), DATA.content.boards.map(function (b) { return { label: b.label, value: b.value, color: 'var(--info)' }; }));
		renderTagCloud(qs('[data-bi-chart="tag-cloud"]'), DATA.content.tags);
		renderDonut(qs('[data-bi-chart="featured-donut"]'), [
			{ label: '精选', value: DATA.content.featured, color: 'var(--warn)' },
			{ label: '普通', value: DATA.content.totalTopics - DATA.content.featured, color: 'var(--surface-subtle)' }
		], { centerLabel: pct(DATA.content.featuredRate), centerSub: '精华率' });

		// 3. Economy
		renderBar(qs('[data-bi-chart="coin-bar"]'), slice(DATA.economy.coinOut, RANGE), {
			alt: function (i) { return i % 2 === 0; }
		});
		renderLine(qs('[data-bi-chart="checkin-line"]'), [{ data: slice(DATA.economy.checkin, RANGE) }], { axis: [RANGE + ' 天前', '今天'] });
		renderDonut(qs('[data-bi-chart="shop-donut"]'), DATA.economy.shop, { centerLabel: '12.8k', centerSub: 'B 币', showPct: true });

		// 4. Moderation
		renderLine(qs('[data-bi-chart="report-line"]'), [{ data: slice(DATA.moderation.reports, RANGE) }], { axis: [RANGE + ' 天前', '今天'] });
		renderDonut(qs('[data-bi-chart="penalty-donut"]'), DATA.moderation.penalties, { centerLabel: '328', centerSub: '总处罚', showPct: true });
		renderBar(qs('[data-bi-chart="handle-bar"]'), slice(DATA.moderation.handleTime, RANGE), { alt: function (i) { return i % 3 === 0; } });

		// 5. Achievements
		renderFunnel(qs('[data-bi-chart="funnel-bar"]'), DATA.achievements.funnel);
		renderDonut(qs('[data-bi-chart="equip-donut"]'), [
			{ label: '已装备', value: DATA.achievements.equipTotal, color: 'var(--ok)' },
			{ label: '未装备', value: DATA.achievements.equipUnlocked - DATA.achievements.equipTotal, color: 'var(--surface-subtle)' }
		], { centerLabel: pct(DATA.achievements.equipRate), centerSub: '装备率' });
		renderHBar(qs('[data-bi-chart="tier-bar"]'), DATA.achievements.tiers.map(function (t) { return { label: t.label, value: t.value, color: t.color }; }));

		// 6. Marketplace
		renderBar(qs('[data-bi-chart="theme-bar"]'), slice(DATA.marketplace.themeSwitch, RANGE));
		renderHBar(qs('[data-bi-chart="plugin-bar"]'), DATA.marketplace.plugins.map(function (p) { return { label: p.label, value: p.value, color: 'var(--info)' }; }));
		renderDonut(qs('[data-bi-chart="oauth-donut"]'), DATA.marketplace.oauth, { centerLabel: '2.5k', centerSub: '授权量', showPct: true });

		renderDrilldowns();
	}

	/* --- Drilldown tables --- */
	function renderDrilldowns() {
		// users
		var usersBody = qs('[data-bi-drill-body="users"]');
		if (usersBody) {
			var dau = slice(DATA.users.dau, RANGE);
			var reg = slice(DATA.users.registers, RANGE);
			var ret = slice(DATA.users.retention, RANGE);
			var html = '';
			for (var i = dau.length - 1; i >= 0 && i >= dau.length - 10; i--) {
				var down = i > 0 && dau[i] < dau[i - 1];
				html += '<tr><td>第 ' + (i + 1) + ' 天</td><td class="col-num">' + reg[i] + '</td><td class="col-num">' + dau[i] + '</td><td class="col-num">' + ret[i] + '%</td><td>' + renderSparkline(slice(dau, Math.min(7, i + 1)), down) + '</td></tr>';
			}
			usersBody.innerHTML = html;
		}
		// content
		var contentBody = qs('[data-bi-drill-body="content"]');
		if (contentBody) {
			var html2 = '';
			DATA.content.boards.forEach(function (b, i) {
				var replies = Math.round(b.value * 6.2);
				var fr = (4 + srand() * 8).toFixed(1);
				html2 += '<tr><td>' + b.label + '</td><td class="col-num">' + b.value + '</td><td class="col-num">' + replies + '</td><td class="col-num">' + fr + '%</td><td>' + renderSparkline(genTimeSeries(7, b.value * 0.1, b.value * 0.15, 5)) + '</td></tr>';
			});
			contentBody.innerHTML = html2;
		}
		// economy
		var econBody = qs('[data-bi-drill-body="economy"]');
		if (econBody) {
			var out = slice(DATA.economy.coinOut, RANGE);
			var inc = slice(DATA.economy.coinIn, RANGE);
			var html3 = '';
			for (var j = out.length - 1; j >= 0 && j >= out.length - 10; j--) {
				var net = out[j] - inc[j];
				html3 += '<tr><td>第 ' + (j + 1) + ' 天</td><td class="col-num">+' + out[j] + '</td><td class="col-num">-' + inc[j] + '</td><td class="col-num">' + (net >= 0 ? '+' : '') + net + '</td><td>' + renderSparkline(slice(out, Math.min(7, j + 1))) + '</td></tr>';
			}
			econBody.innerHTML = html3;
		}
		// moderation
		var modBody = qs('[data-bi-drill-body="moderation"]');
		if (modBody) {
			var total = DATA.moderation.penalties.reduce(function (a, p) { return a + p.value; }, 0);
			var html4 = '';
			DATA.moderation.penalties.forEach(function (p) {
				var t = (2 + srand() * 6).toFixed(1);
				html4 += '<tr><td>' + p.label + '</td><td class="col-num">' + p.value + '</td><td class="col-num">' + pct(p.value / total * 100) + '</td><td class="col-num">' + t + 'h</td><td>' + renderSparkline(genTimeSeries(7, p.value * 0.1, p.value * 0.2, 3)) + '</td></tr>';
			});
			modBody.innerHTML = html4;
		}
		// achievements
		var achBody = qs('[data-bi-drill-body="achievements"]');
		if (achBody) {
			var html5 = '';
			DATA.achievements.detail.forEach(function (a) {
				var rate = pct(a.equipped / a.unlocked * 100);
				html5 += '<tr><td><code>' + a.name + '</code></td><td>' + a.tier + '</td><td class="col-num">' + a.unlocked + '</td><td class="col-num">' + a.equipped + '</td><td class="col-num">' + rate + '</td></tr>';
			});
			achBody.innerHTML = html5;
		}
		// marketplace
		var mpBody = qs('[data-bi-drill-body="marketplace"]');
		if (mpBody) {
			var html6 = '';
			DATA.marketplace.plugins.forEach(function (p) {
				var active = Math.round(p.value * 0.7);
				html6 += '<tr><td>' + p.label + '</td><td>插件</td><td class="col-num">' + p.value + '</td><td class="col-num">' + active + '</td><td>' + renderSparkline(genTimeSeries(7, p.value * 0.1, p.value * 0.15, 4)) + '</td></tr>';
			});
			DATA.marketplace.oauth.forEach(function (o) {
				var active = Math.round(o.value * 0.65);
				html6 += '<tr><td>' + o.label + '</td><td>OAuth</td><td class="col-num">' + o.value + '</td><td class="col-num">' + active + '</td><td>' + renderSparkline(genTimeSeries(7, o.value * 0.1, o.value * 0.15, 3)) + '</td></tr>';
			});
			mpBody.innerHTML = html6;
		}
	}

	/* --- Interactions --- */
	// time range
	qsa('[data-bi-range] .tabs__btn').forEach(function (btn) {
		btn.addEventListener('click', function () {
			qsa('[data-bi-range] .tabs__btn').forEach(function (b) { b.classList.remove('active'); });
			btn.classList.add('active');
			RANGE = parseInt(btn.getAttribute('data-range'), 10) || 30;
			var label = qs('[data-range-label]');
			if (label) label.textContent = '近 ' + RANGE + ' 天';
			SEED = RANGE * 42 + 7;
			regenData();
			renderAll();
			Forum.toast('info', '已切换为近 ' + RANGE + ' 天', 2000);
		});
	});
	// dimension filters
	var boardSel = qs('[data-bi-board]');
	if (boardSel) boardSel.addEventListener('change', function () {
		BOARD = boardSel.value;
		SEED = (BOARD || 'all').length * 100 + RANGE;
		regenData();
		renderAll();
		Forum.toast('info', '已筛选板块：' + (BOARD || '全部'), 2000);
	});
	var userSel = qs('[data-bi-user-type]');
	if (userSel) userSel.addEventListener('change', function () {
		USER_TYPE = userSel.value;
		SEED = (USER_TYPE || 'all').length * 200 + RANGE;
		regenData();
		renderAll();
		Forum.toast('info', '已筛选用户：' + (USER_TYPE || '全部'), 2000);
	});
	// drill toggles
	qsa('[data-bi-drill]').forEach(function (btn) {
		btn.addEventListener('click', function () {
			var key = btn.getAttribute('data-bi-drill');
			var panel = qs('[data-bi-drill-panel="' + key + '"]');
			if (panel) {
				panel.hidden = !panel.hidden;
				btn.querySelector('[data-icon]') && (btn.querySelector('[data-icon]').setAttribute('data-icon', panel.hidden ? 'arrow-down-right' : 'arrow-up-right'));
				if (window.Forum) Forum.init();
			}
		});
	});
	// export CSV
	var exportBtn = qs('[data-bi-export]');
	if (exportBtn) exportBtn.addEventListener('click', function () {
		var csv = 'section,metric,value,delta\n';
		csv += 'users,总用户,12212,+184\n';
		csv += 'users,DAU,' + DATA.users.dau[DATA.users.dau.length - 1] + ',+5.2%\n';
		csv += 'content,总主题,8420,+42\n';
		csv += 'content,精华率,' + DATA.content.featuredRate + '%,-0.3%\n';
		csv += 'economy,流通B币,284128,+1820\n';
		csv += 'economy,签到率,' + DATA.economy.checkin[DATA.economy.checkin.length - 1] + '%,+1.4%\n';
		csv += 'moderation,待处理举报,12,-3\n';
		csv += 'moderation,总处罚,328,+12\n';
		csv += 'achievements,解锁总数,2341,+184\n';
		csv += 'achievements,装备率,' + DATA.achievements.equipRate + '%,+2.1%\n';
		csv += 'marketplace,主题切换,' + DATA.marketplace.themeSwitch[DATA.marketplace.themeSwitch.length - 1] + ',+8\n';
		csv += 'marketplace,插件安装,6880,+142\n';
		var blob = new Blob(['\ufeff' + csv], { type: 'text/csv;charset=utf-8' });
		var url = URL.createObjectURL(blob);
		var a = document.createElement('a');
		a.href = url;
		a.download = 'bi-export-' + RANGE + 'd-' + Date.now() + '.csv';
		document.body.appendChild(a);
		a.click();
		document.body.removeChild(a);
		URL.revokeObjectURL(url);
		Forum.toast('success', 'CSV 已导出', 2400);
		Forum.activity && Forum.activity.log({ label: 'BI 导出 CSV', detail: '近 ' + RANGE + ' 天 · ' + (BOARD || '全部板块'), kind: 'info', icon: 'download' });
	});
	// refresh
	var refreshBtn = qs('[data-bi-refresh]');
	if (refreshBtn) refreshBtn.addEventListener('click', function () {
		SEED = Date.now() % 233280;
		regenData();
		renderAll();
		Forum.toast('success', '数据已刷新', 2000);
	});

	function regenData() {
		DATA.users.dau = genTimeSeries(90, 800, 1450, 200);
		DATA.users.mau = genTimeSeries(90, 9200, 11800, 600);
		DATA.users.registers = genTimeSeries(90, 12, 48, 15);
		DATA.users.retention = genTimeSeries(90, 62, 78, 8);
		DATA.economy.coinOut = genTimeSeries(90, 2800, 4200, 600);
		DATA.economy.coinIn = genTimeSeries(90, 1800, 3200, 500);
		DATA.economy.checkin = genTimeSeries(90, 62, 88, 10);
		DATA.moderation.reports = genTimeSeries(90, 8, 42, 12);
		DATA.moderation.handleTime = genTimeSeries(90, 2, 8, 3);
		DATA.marketplace.themeSwitch = genTimeSeries(90, 12, 48, 15);
	}

	// initial render
	renderAll();
	Forum.activity && Forum.activity.log({ label: 'BI 看板查看', detail: '近 ' + RANGE + ' 天', kind: 'info', icon: 'bar-chart' });
})();
