#!/usr/bin/env python3
"""
生成静态装扮资产挑选器 frontend/static/cosmetics/curator.html。

单文件 HTML：内嵌头像框/背景目录与本地文件体积，媒体用相对路径引用
（/cosmetics/ 下的 backgrounds 与 frames/steam），既可由 Vite 静态服务，
也可直接双击打开。选择状态存 localStorage，可导出 JSON/CSV 保留清单。
"""
import json
import os
from datetime import date

BASE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DATA_DIR = os.path.join(BASE, 'frontend', 'src', 'lib', 'data')
OUT_DIR = os.path.join(BASE, 'frontend', 'static', 'cosmetics')
FRAMES_DIR = os.path.join(OUT_DIR, 'frames', 'steam')
BG_DIR = os.path.join(OUT_DIR, 'backgrounds')


def fsize(path):
    try:
        return os.path.getsize(path)
    except OSError:
        return 0


def build_frames():
    items = json.load(open(os.path.join(DATA_DIR, 'steam-avatar-frames.json'), encoding='utf-8'))
    out = []
    for x in items:
        img = x.get('image')
        if not img:
            continue
        out.append({
            'defid': x.get('defid'),
            'id': x.get('id'),
            'name': x.get('name'),
            'appid': x.get('appid'),
            'image': img,
            'bytes': fsize(os.path.join(FRAMES_DIR, img)),
        })
    return out


def build_backgrounds():
    items = json.load(open(os.path.join(DATA_DIR, 'steam-profile-backgrounds.json'), encoding='utf-8'))
    out = []
    for x in items:
        img = x.get('image')
        if not img:
            continue
        entry = {
            'defid': x.get('defid'),
            'id': x.get('id'),
            'name': x.get('name'),
            'appid': x.get('appid'),
            'image': img,
            'bytes': fsize(os.path.join(BG_DIR, img)),
            'webm': x.get('webm'),
            'webmBytes': fsize(os.path.join(BG_DIR, x['webm'])) if x.get('webm') else 0,
            'mp4': x.get('mp4'),
            'mp4Bytes': fsize(os.path.join(BG_DIR, x['mp4'])) if x.get('mp4') else 0,
        }
        out.append(entry)
    return out


TEMPLATE = r"""<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>装扮资产挑选器 · 保留清单</title>
<style>
  :root{
    --bg:#0d1117; --panel:#151b28; --panel-2:#1b2333; --line:#26304a;
    --ink:#e8ecf4; --muted:#93a0b8; --keep:#34d399; --accent:#f5a623; --danger:#f87171;
    --radius:10px;
  }
  *{box-sizing:border-box}
  html,body{margin:0;padding:0}
  body{background:var(--bg);color:var(--ink);font:14px/1.5 -apple-system,"PingFang SC","Microsoft YaHei",system-ui,sans-serif}
  button{font:inherit;color:inherit;cursor:pointer}
  :focus-visible{outline:2px solid var(--accent);outline-offset:2px}

  .bar{position:sticky;top:0;z-index:20;background:linear-gradient(180deg,var(--panel) 0%,var(--panel) 85%,transparent);padding:14px 18px 10px;border-bottom:1px solid var(--line)}
  .bar-row{display:flex;flex-wrap:wrap;gap:10px;align-items:center;max-width:1720px;margin:0 auto}
  .brand{font-weight:800;font-size:16px;letter-spacing:.5px;color:var(--accent);white-space:nowrap}
  .brand small{color:var(--muted);font-weight:500;margin-left:8px;letter-spacing:0}
  .tabs{display:flex;gap:4px;background:var(--panel-2);border:1px solid var(--line);border-radius:8px;padding:3px}
  .tabs button{border:0;background:transparent;color:var(--muted);padding:6px 14px;border-radius:6px;font-weight:600}
  .tabs button[aria-selected="true"]{background:var(--accent);color:#151007}
  .search{flex:1;min-width:200px;max-width:360px}
  .search input{width:100%;background:var(--panel-2);border:1px solid var(--line);border-radius:8px;color:var(--ink);padding:7px 12px}
  .search input::placeholder{color:var(--muted)}
  .batch{display:flex;gap:6px;flex-wrap:wrap}
  .batch button,.ghost{background:var(--panel-2);border:1px solid var(--line);border-radius:8px;padding:6px 12px;color:var(--ink)}
  .batch button:hover,.ghost:hover{border-color:var(--accent)}
  select{background:var(--panel-2);border:1px solid var(--line);border-radius:8px;color:var(--ink);padding:7px 8px}
  .spacer{flex:1}
  .stats{font-size:13px;color:var(--muted);white-space:nowrap}
  .stats b{color:var(--ink)}
  .stats .size{color:var(--keep);font-weight:700}
  .export{background:var(--accent);color:#151007;border:0;border-radius:8px;padding:8px 16px;font-weight:800}
  .export:hover{filter:brightness(1.08)}
  .hint{max-width:1720px;margin:6px auto 0;font-size:12px;color:var(--muted)}

  main{max-width:1720px;margin:0 auto;padding:16px 18px 80px}
  .grid{display:grid;gap:12px;grid-template-columns:repeat(auto-fill,minmax(168px,1fr))}
  .card{position:relative;background:var(--panel);border:1px solid var(--line);border-radius:var(--radius);overflow:hidden;cursor:pointer;transition:opacity .15s,border-color .15s}
  .card:hover{border-color:var(--accent)}
  .card.off{opacity:.38;border-style:dashed}
  .card .ph{position:relative;display:flex;align-items:center;justify-content:center;background:
    radial-gradient(120% 100% at 50% 0%, #1c2436 0%, #121826 70%)}
  .card.k-frames .ph{aspect-ratio:1/1;padding:10px}
  .card.k-bg .ph{aspect-ratio:16/9}
  .card img{width:100%;height:100%;object-fit:contain}
  .card.k-bg img{object-fit:cover}
  .card .zoom{position:absolute;left:6px;top:6px;width:26px;height:26px;border-radius:6px;border:1px solid var(--line);background:rgba(13,17,23,.72);color:var(--ink);display:none;align-items:center;justify-content:center;font-size:13px}
  .card:hover .zoom{display:flex}
  @media (hover:none){.card .zoom{display:flex}}
  .tick{position:absolute;right:8px;top:8px;width:24px;height:24px;border-radius:7px;border:2px solid var(--muted);background:rgba(13,17,23,.72);display:flex;align-items:center;justify-content:center;color:transparent;font-size:14px;font-weight:900}
  .card.on .tick{border-color:var(--keep);background:var(--keep);color:#06281c}
  .meta{padding:8px 10px 10px}
  .name{font-size:12.5px;font-weight:600;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .sub{margin-top:2px;font:11px/1.4 ui-monospace,SFMono-Regular,Menlo,monospace;color:var(--muted);display:flex;justify-content:space-between;gap:6px}
  .empty{color:var(--muted);padding:60px 0;text-align:center}

  .modal{position:fixed;inset:0;z-index:50;display:none;align-items:center;justify-content:center;background:rgba(5,8,14,.82);padding:24px}
  .modal.open{display:flex}
  .box{max-width:920px;width:100%;background:var(--panel);border:1px solid var(--line);border-radius:14px;overflow:hidden}
  .stage{position:relative;background:#0a0e16;display:flex;align-items:center;justify-content:center;min-height:320px;max-height:64vh}
  .stage img,.stage video{max-width:100%;max-height:64vh;object-fit:contain}
  .stage video{width:100%}
  .box .info{display:flex;gap:12px;align-items:center;padding:14px 16px;border-top:1px solid var(--line);flex-wrap:wrap}
  .box .info .name{font-size:15px;white-space:normal}
  .nav{display:flex;gap:6px;margin-left:auto}
  .toast{position:fixed;left:50%;bottom:28px;transform:translateX(-50%);background:var(--panel-2);border:1px solid var(--accent);color:var(--ink);padding:10px 18px;border-radius:10px;z-index:60;opacity:0;pointer-events:none;transition:opacity .2s}
  .toast.show{opacity:1}
  @media (prefers-reduced-motion:reduce){*{transition:none!important}}
</style>
</head>
<body>
<header class="bar">
  <div class="bar-row">
    <span class="brand">装扮资产挑选器<small>勾选 = 保留</small></span>
    <div class="tabs" role="tablist">
      <button id="tab-frames" role="tab" aria-selected="true">头像框 <span id="cnt-frames"></span></button>
      <button id="tab-bg" role="tab" aria-selected="false">全景背景 <span id="cnt-bg"></span></button>
    </div>
    <div class="search"><input id="q" type="search" placeholder="搜索名称 / appid / defid…"></div>
    <select id="viewFilter" aria-label="筛选">
      <option value="all">全部</option>
      <option value="on">只看已选</option>
      <option value="off">只看未选</option>
    </select>
    <div class="batch">
      <button id="b-on">选中当前</button>
      <button id="b-off">取消当前</button>
      <button id="b-inv">反选当前</button>
      <button id="b-all">全选</button>
      <button id="b-none">清空</button>
    </div>
    <span class="spacer"></span>
    <span class="stats">已选 <b id="statCount">0</b>/<span id="statTotal">0</span> 项 · 保留约 <span class="size" id="statSize">0 B</span> / 全部 <span id="statAll">0 B</span></span>
    <button class="export" id="btnExport">导出清单</button>
    <button class="ghost" id="btnCopy">复制 JSON</button>
  </div>
  <div class="hint">单击卡片切换保留状态 · 点左上角 ⛶ 看大图（背景会播放动态视频）· 导出的 JSON 直接发给开发者即可</div>
</header>
<main><div class="grid" id="grid"></div><div class="empty" id="empty" hidden>没有匹配的条目</div></main>

<div class="modal" id="modal" role="dialog" aria-modal="true">
  <div class="box">
    <div class="stage" id="stage"></div>
    <div class="info">
      <div style="min-width:200px">
        <div class="name" id="mName"></div>
        <div class="sub" id="mMeta" style="margin-top:4px"></div>
      </div>
      <button class="ghost" id="mToggle">切换保留</button>
      <div class="nav">
        <button class="ghost" id="mPrev">‹ 上一项</button>
        <button class="ghost" id="mNext">下一项 ›</button>
        <button class="ghost" id="mClose">关闭 (Esc)</button>
      </div>
    </div>
  </div>
</div>
<div class="toast" id="toast"></div>

<script>
const DATA = __PAYLOAD__;
const LS_KEY = 'cosmetic-keep-v1';

const KINDS = {
  frames: {list: DATA.frames, dir: 'frames/steam/', label: '头像框'},
  backgrounds: {list: DATA.backgrounds, dir: 'backgrounds/', label: '全景背景'}
};

// 选择状态：按文件名去重（多个条目共用同一文件时同选同弃）
let selected = {frames: new Set(), backgrounds: new Set()};
try {
  const saved = JSON.parse(localStorage.getItem(LS_KEY) || 'null');
  if (saved && saved.frames && saved.backgrounds) {
    selected.frames = new Set(saved.frames);
    selected.backgrounds = new Set(saved.backgrounds);
  } else {
    for (const k of Object.keys(KINDS)) selected[k] = new Set(KINDS[k].list.map(x => x.image));
  }
} catch (e) {
  for (const k of Object.keys(KINDS)) selected[k] = new Set(KINDS[k].list.map(x => x.image));
}

let view = 'frames', query = '', vFilter = 'all';
let filtered = [], modalIdx = -1;

const $ = s => document.querySelector(s);
const grid = $('#grid'), q = $('#q');

function fmtSize(n) {
  if (!n) return '0 B';
  const u = ['B','KB','MB','GB']; let i = 0;
  while (n >= 1024 && i < u.length - 1) { n /= 1024; i++; }
  return n.toFixed(n >= 100 || i === 0 ? 0 : 2) + ' ' + u[i];
}

function fileBytes(kind, name) {
  const item = KINDS[kind].list.find(x => x.image === name);
  if (!item) return 0;
  return (item.bytes || 0) + (item.webmBytes || 0) + (item.mp4Bytes || 0);
}

function save() {
  localStorage.setItem(LS_KEY, JSON.stringify({
    frames: [...selected.frames], backgrounds: [...selected.backgrounds]
  }));
}

function match(item) {
  if (vFilter === 'on' && !selected[view].has(item.image)) return false;
  if (vFilter === 'off' && selected[view].has(item.image)) return false;
  if (!query) return true;
  const t = query.toLowerCase();
  return (item.name || '').toLowerCase().includes(t)
    || String(item.appid || '').includes(t)
    || String(item.defid || '').includes(t);
}

function refilter() { filtered = KINDS[view].list.filter(match); }

function stats() {
  let on = 0, total = 0, keepBytes = 0, allBytes = 0;
  for (const kind of Object.keys(KINDS)) {
    const seen = new Map();
    for (const x of KINDS[kind].list) {
      total++;
      if (selected[kind].has(x.image)) on++;
      if (!seen.has(x.image)) {
        const b = fileBytes(kind, x.image);
        seen.set(x.image, b);
        allBytes += b;
        if (selected[kind].has(x.image)) keepBytes += b;
      }
    }
  }
  $('#statCount').textContent = on;
  $('#statTotal').textContent = total;
  $('#statSize').textContent = fmtSize(keepBytes);
  $('#statAll').textContent = fmtSize(allBytes);
  $('#cnt-frames').textContent = '(' + selected.frames.size + '/' + KINDS.frames.list.length + ')';
  $('#cnt-bg').textContent = '(' + selected.backgrounds.size + '/' + KINDS.backgrounds.list.length + ')';
}

function card(item, idx) {
  const on = selected[view].has(item.image);
  const bytes = (item.bytes || 0) + (item.webmBytes || 0) + (item.mp4Bytes || 0);
  const div = document.createElement('div');
  div.className = 'card ' + (view === 'frames' ? 'k-frames' : 'k-bg') + (on ? ' on' : ' off');
  div.dataset.idx = idx;
  const media = view === 'frames'
    ? '<img loading="lazy" decoding="async" src="' + KINDS[view].dir + item.image + '" alt="' + item.name.replace(/"/g,'&quot;') + '">'
    : '<img loading="lazy" decoding="async" src="' + KINDS[view].dir + item.image + '" alt="' + item.name.replace(/"/g,'&quot;') + '">';
  div.innerHTML =
    '<div class="ph">' + media +
    '<button class="zoom" data-zoom="' + idx + '" aria-label="放大预览">⛶</button>' +
    '<span class="tick" aria-hidden="true">✓</span></div>' +
    '<div class="meta"><div class="name">' + item.name + '</div>' +
    '<div class="sub"><span>#' + item.defid + '</span><span>' + fmtSize(bytes) + '</span></div></div>';
  return div;
}

function render() {
  refilter();
  grid.innerHTML = '';
  const frag = document.createDocumentFragment();
  filtered.forEach((item, idx) => frag.appendChild(card(item, idx)));
  grid.appendChild(frag);
  $('#empty').hidden = filtered.length > 0;
  stats();
}

function toggle(kind, file) {
  const s = selected[kind];
  s.has(file) ? s.delete(file) : s.add(file);
  save(); stats(); render();
}

grid.addEventListener('click', e => {
  const zoom = e.target.closest('[data-zoom]');
  if (zoom) { e.stopPropagation(); openModal(+zoom.dataset.zoom); return; }
  const cardEl = e.target.closest('.card');
  if (!cardEl) return;
  const item = filtered[+cardEl.dataset.idx];
  if (item) toggle(view, item.image);
});

// ── 批量操作 ──
function act(fn) { fn(selected[view]); save(); render(); }
$('#b-on').onclick = () => act(s => filtered.forEach(x => s.add(x.image)));
$('#b-off').onclick = () => act(s => filtered.forEach(x => s.delete(x.image)));
$('#b-inv').onclick = () => act(s => filtered.forEach(x => s.has(x.image) ? s.delete(x.image) : s.add(x.image)));
$('#b-all').onclick = () => act(s => KINDS[view].list.forEach(x => s.add(x.image)));
$('#b-none').onclick = () => act(s => s.clear());

// ── 页签 / 搜索 / 筛选 ──
function setView(v) {
  view = v;
  $('#tab-frames').setAttribute('aria-selected', v === 'frames');
  $('#tab-bg').setAttribute('aria-selected', v === 'backgrounds');
  render();
}
$('#tab-frames').onclick = () => setView('frames');
$('#tab-bg').onclick = () => setView('backgrounds');
let deb;
q.addEventListener('input', () => { clearTimeout(deb); deb = setTimeout(() => { query = q.value.trim(); render(); }, 150); });
$('#viewFilter').onchange = e => { vFilter = e.target.value; render(); };

// ── 大图预览 ──
const modal = $('#modal'), stage = $('#stage');
function openModal(idx) {
  modalIdx = idx;
  const item = filtered[idx];
  if (!item) return;
  const dir = KINDS[view].dir;
  if (view === 'backgrounds' && (item.webm || item.mp4)) {
    stage.innerHTML = '<video autoplay loop muted playsinline preload="auto"' +
      (item.image ? ' poster="' + dir + item.image + '"' : '') + '>' +
      (item.webm ? '<source src="' + dir + item.webm + '" type="video/webm">' : '') +
      (item.mp4 ? '<source src="' + dir + item.mp4 + '" type="video/mp4">' : '') + '</video>';
  } else {
    stage.innerHTML = '<img src="' + dir + item.image + '" alt="">';
  }
  $('#mName').textContent = item.name;
  const bytes = (item.bytes || 0) + (item.webmBytes || 0) + (item.mp4Bytes || 0);
  $('#mMeta').innerHTML = '<span>#' + item.defid + '</span><span>appid ' + item.appid + '</span><span>' + fmtSize(bytes) + '</span>' +
    '<span style="color:' + (selected[view].has(item.image) ? 'var(--keep)' : 'var(--danger)') + '">' +
    (selected[view].has(item.image) ? '保留' : '不保留') + '</span>';
  modal.classList.add('open');
}
function closeModal() { modal.classList.remove('open'); stage.innerHTML = ''; modalIdx = -1; }
function step(d) { if (modalIdx < 0) return; openModal((modalIdx + d + filtered.length) % filtered.length); }
$('#mClose').onclick = closeModal;
$('#mPrev').onclick = () => step(-1);
$('#mNext').onclick = () => step(1);
$('#mToggle').onclick = () => { const it = filtered[modalIdx]; if (it) { toggle(view, it.image); openModal(modalIdx); } };
modal.addEventListener('click', e => { if (e.target === modal) closeModal(); });
document.addEventListener('keydown', e => {
  if (modalIdx < 0) return;
  if (e.key === 'Escape') closeModal();
  if (e.key === 'ArrowLeft') step(-1);
  if (e.key === 'ArrowRight') step(1);
});

// ── 导出 ──
function buildExport() {
  const pick = kind => KINDS[kind].list.filter(x => selected[kind].has(x.image)).map(x => ({
    defid: x.defid, id: x.id, name: x.name, appid: x.appid, image: x.image,
    ...(kind === 'backgrounds' ? {webm: x.webm || null, mp4: x.mp4 || null} : {})
  }));
  const frames = pick('frames'), backgrounds = pick('backgrounds');
  let keepBytes = 0;
  for (const kind of Object.keys(KINDS)) {
    const seen = new Set();
    for (const x of KINDS[kind].list) {
      if (selected[kind].has(x.image) && !seen.has(x.image)) { seen.add(x.image); keepBytes += fileBytes(kind, x.image); }
    }
  }
  return {
    version: 1,
    exportedAt: new Date().toISOString(),
    note: '勾选项 = 保留清单；未勾选的本地文件将被删除，目录 JSON 同步裁剪',
    stats: {
      frames: {selected: frames.length, total: KINDS.frames.list.length},
      backgrounds: {selected: backgrounds.length, total: KINDS.backgrounds.list.length},
      approxKeepBytes: keepBytes
    },
    frames, backgrounds
  };
}
function download(name, text, type) {
  const blob = new Blob([text], {type});
  const a = document.createElement('a');
  a.href = URL.createObjectURL(blob); a.download = name;
  document.body.appendChild(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(a.href), 4000);
}
function toast(msg) {
  const t = $('#toast'); t.textContent = msg; t.classList.add('show');
  setTimeout(() => t.classList.remove('show'), 2200);
}
$('#btnExport').onclick = () => {
  const data = buildExport();
  download('keep-list.json', JSON.stringify(data, null, 1), 'application/json');
  toast('已导出 keep-list.json（' + (data.frames.length + data.backgrounds.length) + ' 项）');
};
$('#btnCopy').onclick = async () => {
  const data = buildExport();
  try { await navigator.clipboard.writeText(JSON.stringify(data)); toast('JSON 已复制到剪贴板'); }
  catch (e) { download('keep-list.json', JSON.stringify(data, null, 1), 'application/json'); toast('剪贴板不可用，已改为下载文件'); }
};

// ── 启动 ──
setView('frames');
</script>
</body>
</html>
"""


def main():
    frames = build_frames()
    backgrounds = build_backgrounds()
    payload = json.dumps({'frames': frames, 'backgrounds': backgrounds}, ensure_ascii=False)
    payload = payload.replace('</', '<\\/')  # 防止 </script> 提前闭合
    html = TEMPLATE.replace('__PAYLOAD__', payload)
    out = os.path.join(OUT_DIR, 'curator.html')
    with open(out, 'w', encoding='utf-8') as f:
        f.write(html)
    print(f'curator.html written: {os.path.getsize(out) / 1024:.0f} KB')
    print(f'  frames: {len(frames)} entries, total {sum(x["bytes"] for x in frames) / 1e9:.2f} GB')
    bg_total = sum(x["bytes"] + x["webmBytes"] + x["mp4Bytes"] for x in backgrounds)
    print(f'  backgrounds: {len(backgrounds)} entries, total {bg_total / 1e9:.2f} GB')
    print(f'  generated: {date.today().isoformat()}')


if __name__ == '__main__':
    main()
