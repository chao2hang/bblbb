#!/usr/bin/env node
/**
 * BBLBB 小程序端 → 后端 全链路冒烟测试
 *
 * 模拟微信小程序请求行为（与 miniprogram/utils/request.js 同一序列）：
 * - 手动 cookie jar（解析 Set-Cookie / 回带 Cookie）
 * - 预认证 CSRF（GET /auth/csrf 无会话）与会话 CSRF（有会话）
 * - 微信运行时请求头：Referer: https://servicewechat.com/{appid}/{page}/{ver}
 *   与 MicroMessenger UA
 * - Problem JSON 错误归一化
 *
 * 用法：
 *   node miniprogram/dev/smoke-test.js [base]   # 默认 http://127.0.0.1:18181
 *
 * 前置：dev/start-backend.sh 已启动（独立 SQLite）。
 * 每次运行使用随机用户名，可重复执行。
 * 用户「发帖/评论/签到」需要邮箱已验证 + 冷静期已过——脚本自动通过
 * sqlite3 激活（等价于 dev 环境激活流程；无 sqlite3 时跳过写路径并警告）。
 */

'use strict';

const { execFileSync } = require('child_process');
const crypto = require('crypto');

const BASE = process.argv[2] || 'http://127.0.0.1:18181';
const APPID = 'wx0001234567890abc';
const DB = process.env.BBLBB_MP_DB || '/tmp/bblbb-miniprogram.sqlite';

let passed = 0;
let failed = 0;
const fails = [];

function ok(name, cond, extra) {
  if (cond) {
    passed += 1;
    console.log(`  ✓ ${name}`);
  } else {
    failed += 1;
    fails.push(name);
    console.log(`  ✗ ${name}${extra ? ` — ${extra}` : ''}`);
  }
}

function uuid() {
  return crypto.randomUUID();
}

// ── 极简 cookie jar（模拟小程序运行时 / 手动 cookie 模式） ──────────────────
class Jar {
  constructor() {
    this.cookies = new Map(); // name → value
  }
  applySetCookies(headers) {
    const raw = headers.getSetCookie ? headers.getSetCookie() : [headers.get('set-cookie')].filter(Boolean);
    raw.forEach((line) => {
      const first = String(line).split(';')[0];
      const eq = first.indexOf('=');
      if (eq <= 0) return;
      const name = first.slice(0, eq).trim();
      const value = first.slice(eq + 1).trim();
      const attrs = String(line).toLowerCase();
      const maxAge = attrs.match(/max-age=(-?\d+)/);
      if (maxAge && Number(maxAge[1]) <= 0) this.cookies.delete(name);
      else if (value === '' && /expires=thu, 01 jan 1970/.test(attrs)) this.cookies.delete(name);
      else this.cookies.set(name, value);
    });
  }
  header() {
    return [...this.cookies.entries()].map(([k, v]) => `${k}=${v}`).join('; ');
  }
}

function wechatHeaders(page) {
  return {
    referer: `https://servicewechat.com/${APPID}/${page}/1.0.0`,
    'user-agent':
      'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 MicroMessenger/8.0.40.24(0x68002839)',
  };
}

async function req(jar, method, path, { body, headers = {}, expect } = {}) {
  const url = `${BASE}${path}`;
  const h = { Accept: 'application/json', 'X-Client': 'bblbb-miniprogram-smoke/1.0', ...headers };
  const cookie = jar.header();
  if (cookie) h.cookie = cookie;
  let payload;
  if (body !== undefined) {
    h['content-type'] = h['content-type'] || 'application/json';
    payload = typeof body === 'string' ? body : JSON.stringify(body);
  }
  const res = await fetch(url, { method, headers: h, body: payload, redirect: 'manual' });
  jar.applySetCookies(res.headers);
  const text = await res.text();
  let data = null;
  try {
    data = text ? JSON.parse(text) : null;
  } catch (e) {
    data = null;
  }
  const statusOk = expect ? res.status === expect : res.status >= 200 && res.status < 300;
  return { status: res.status, data, headers: res.headers, ok: statusOk };
}

function activateUser(username) {
  try {
    execFileSync(
      'sqlite3',
      [
        DB,
        `UPDATE users SET status='active', email_verified=1, email_verified_at=(strftime('%s','now') - 25*3600)*1000 WHERE username_normalized='${username}';`,
      ],
      { stdio: 'pipe' }
    );
    return true;
  } catch (e) {
    return false;
  }
}

async function main() {
  console.log(`\nBBLBB 小程序冒烟测试 → ${BASE}\n`);

  // 1. 健康检查
  {
    const r = await req(new Jar(), 'GET', '/healthz');
    ok('GET /healthz', r.ok && r.data && r.data.status === 'ok');
  }

  const jar = new Jar();
  // 固定共享测试账号：重复运行时直接登录（注册 IP 限流 3 次/小时，
  // 只在首次运行注册一次）。
  const user = 'mp_smoke_shared';
  const password = 'Smoke12345';

  // 2. 预认证 CSRF
  let csrf;
  {
    const r = await req(jar, 'GET', '/api/v1/auth/csrf', { headers: wechatHeaders('pages/login/login') });
    csrf = r.data && r.data.token;
    ok('GET /auth/csrf（预认证 token + __Host-bblbb_csrf cookie）', r.ok && !!csrf && jar.cookies.has('__Host-bblbb_csrf'));
  }

  // 3. 登录（预认证 CSRF 复用）——首次运行先注册 + 激活
  let me;
  let activated = false;
  {
    let r = await req(jar, 'POST', '/api/v1/auth/login', {
      body: { identifier: user, password, remember: false },
      headers: { 'x-csrf-token': csrf, ...wechatHeaders('pages/login/login') },
    });
    if (r.status === 401) {
      // 账号不存在（首次运行）→ 注册 + dev 激活
      const reg = await req(jar, 'POST', '/api/v1/auth/register', {
        body: { username: user, password, email: `${user}@example.com` },
        headers: { 'x-csrf-token': csrf, ...wechatHeaders('pages/register/register') },
        expect: 201,
      });
      ok('POST /auth/register（预认证 CSRF + 微信 Referer，仅首跑）', reg.ok && reg.data && reg.data.ok === true,
        reg.status === 429 ? '注册 IP 限流（3 次/小时）——等待窗口或改用 BBLBB_MP_DB 重置' : undefined);
      if (!reg.ok) {
        console.error('\n注册失败，无法继续认证流程（写路径依赖登录账号）。');
        console.error(`结果：${passed} 通过，${failed} 失败（中止）`);
        process.exit(1);
      }
      activated = activateUser(user);
      if (!activated) console.log('  ! 无法激活用户（sqlite3 或 DB 不可用），写路径将返回 403——仅验证读路径');
      r = await req(jar, 'POST', '/api/v1/auth/login', {
        body: { identifier: user, password, remember: false },
        headers: { 'x-csrf-token': csrf, ...wechatHeaders('pages/login/login') },
      });
    }
    me = r.data;
    ok(
      'POST /auth/login（Me 投影 + __Host-bblbb_session cookie）',
      r.status === 200 && !!me && !!me.id && jar.cookies.has('__Host-bblbb_session'),
      JSON.stringify(r.data)
    );
    // 幂等激活：dev 环境无 SMTP，确保账号处于 active+已验证+冷静期已过
    // （重复运行对已激活账号是 no-op UPDATE）。
    activated = activateUser(user) ||
      (me && me.status === 'active' && me.email_verified === true);
    if (!activated) console.log('  ! 无法确认账号已激活，写路径可能返回 403——仅验证读路径');
  }

  // 6. 会话 CSRF
  let sCsrf;
  {
    const r = await req(jar, 'GET', '/api/v1/auth/csrf', { headers: wechatHeaders('pages/index/index') });
    sCsrf = r.data && r.data.token;
    ok('GET /auth/csrf（已登录 → 会话 token）', r.ok && !!sCsrf && sCsrf !== csrf);
  }

  // 7. GET /me
  {
    const r = await req(jar, 'GET', '/api/v1/me', { headers: wechatHeaders('pages/profile/profile') });
    ok('GET /me', r.ok && r.data && r.data.username === user && typeof r.data.version === 'number');
  }

  // 8. 板块
  let boardId;
  {
    const r = await req(jar, 'GET', '/api/v1/boards', { headers: wechatHeaders('pages/index/index') });
    const items = (r.data && r.data.items) || [];
    boardId = items[0] && items[0].id;
    ok('GET /boards（种子板块）', r.ok && items.length >= 5 && !!boardId);
  }

  // 9. 发帖
  let postId;
  if (activated) {
    // 正文必须逐次唯一：后端反作弊 duplicate_rule 会在 7 天窗口内
    // 将「其他作者的相同正文指纹」判定为高风险 → pending_review。
    const nonce = `${Date.now()}-${crypto.randomBytes(4).toString('hex')}`;
    const r = await req(jar, 'POST', '/api/v1/posts', {
      body: {
        type: 'discussion',
        title: `冒烟测试帖 ${nonce}`,
        markdown: `冒烟测试正文 ${nonce}\n\n- 列表项`,
        board_id: boardId,
        access_policy: 'public',
        client_request_id: uuid(),
      },
      headers: { 'x-csrf-token': sCsrf, ...wechatHeaders('pages/create/create') },
      expect: 201,
    });
    postId = r.data && r.data.id;
    const published = r.data && r.data.status === 'published';
    ok(
      'POST /posts（会话 CSRF + 微信 Referer；新用户可能进 pending_review 人工审核）',
      r.ok && !!postId,
      r.ok ? `status=${r.data.status}` : JSON.stringify(r.data).slice(0, 200)
    );
    // pending_review 的帖子对匿名/他人不可见（评论/收藏等子资源 404 属预期）
    if (!published) console.log('  ! 帖子进入人工审核（pending_review）——子资源断言跳过');
  }

  // 10. 帖子详情
  if (postId) {
    const r = await req(jar, 'GET', `/api/v1/posts/${postId}`, { headers: wechatHeaders('pages/post/post') });
    ok(
      'GET /posts/{id}（body_html + access_summary + capabilities）',
      r.ok && r.data && typeof r.data.body_html === 'string' && !!r.data.access_summary
    );
  }

  // 11. 全站帖子流
  {
    const r = await req(jar, 'GET', '/api/v1/posts?limit=10', { headers: wechatHeaders('pages/index/index') });
    ok('GET /posts（{items, page{next_cursor, has_more}}）', r.ok && Array.isArray(r.data.items) && !!r.data.page);
  }

  // 12. 评论
  if (postId) {
    const c = await req(jar, 'POST', `/api/v1/posts/${postId}/comments`, {
      body: { markdown: '冒烟评论', client_request_id: uuid() },
      headers: { 'x-csrf-token': sCsrf, ...wechatHeaders('pages/post/post') },
      expect: 201,
    });
    ok('POST /posts/{id}/comments（楼层 1）', c.ok && c.data && c.data.floor === 1, JSON.stringify(c.data).slice(0, 200));
    const l = await req(jar, 'GET', `/api/v1/posts/${postId}/comments`, { headers: wechatHeaders('pages/post/post') });
    ok('GET /posts/{id}/comments', l.ok && l.data.items.length === 1);
  }

  // 13. 点赞（不能赞自己 → 期望 400；作为作者验证错误契约）
  if (postId) {
    const r = await req(jar, 'POST', `/api/v1/posts/${postId}/reactions`, {
      body: { reaction: 'like' },
      headers: { 'x-csrf-token': sCsrf, ...wechatHeaders('pages/post/post') },
      expect: 400,
    });
    ok('POST /posts/{id}/reactions（赞自己 → 400 cannot react to own content）', r.ok);
  }

  // 14. 收藏
  if (postId) {
    const r = await req(jar, 'POST', `/api/v1/posts/${postId}/favorite`, {
      headers: { 'x-csrf-token': sCsrf, ...wechatHeaders('pages/post/post') },
    });
    ok('POST /posts/{id}/favorite', r.status < 300);
    const l = await req(jar, 'GET', '/api/v1/me/favorites', { headers: wechatHeaders('pages/favorites/favorites') });
    ok('GET /me/favorites', l.ok && l.data.items.length >= 1);
  }

  // 15. 签到（MicroMessenger UA 必须）
  {
    const r = await req(jar, 'POST', '/api/v1/activity/visit', {
      body: { path: '/me' },
      headers: { 'x-csrf-token': sCsrf, ...wechatHeaders('pages/profile/profile') },
    });
    ok(
      'POST /activity/visit（签到 +10 经验）',
      r.ok && r.data.checked_in_today === true && Array.isArray(r.data.today_earned),
      JSON.stringify(r.data)
    );
    const s = await req(jar, 'GET', '/api/v1/activity/summary', { headers: wechatHeaders('pages/profile/profile') });
    ok('GET /activity/summary（等级/经验/连续天数）', s.ok && s.data.streak_days === 1 && s.data.level.name === 'L1');
  }

  // 16. 积分流水
  {
    const r = await req(jar, 'GET', '/api/v1/me/point-transactions', { headers: wechatHeaders('pages/transactions/transactions') });
    ok('GET /me/point-transactions', r.ok && r.data.items.length >= 1);
  }

  // 17. 编辑资料（If-Match 乐观并发）
  {
    const meNow = (await req(jar, 'GET', '/api/v1/me', { headers: wechatHeaders('pages/profile-edit/profile-edit') })).data;
    const r = await req(jar, 'PATCH', '/api/v1/me', {
      body: { display_name: `冒烟 ${user}`, bio: 'smoke' },
      headers: { 'if-match': String(meNow.version), 'x-csrf-token': sCsrf, ...wechatHeaders('pages/profile-edit/profile-edit') },
    });
    ok('PATCH /me（If-Match version）', r.ok && r.data.version === meNow.version + 1);
  }

  // 18. 成就
  {
    const a = await req(jar, 'GET', '/api/v1/achievements', { headers: wechatHeaders('pages/achievements/achievements') });
    const m = await req(jar, 'GET', '/api/v1/me/achievements', { headers: wechatHeaders('pages/achievements/achievements') });
    ok('GET /achievements + /me/achievements', a.ok && a.data.items.length >= 3 && m.ok);
  }

  // 19. 商城
  {
    const r = await req(jar, 'GET', '/api/v1/shop/products', { headers: wechatHeaders('pages/shop/shop') });
    ok('GET /shop/products（{products: []}）', r.ok && Array.isArray(r.data.products));
  }

  // 20. 公开用户 + 关注自己（期望 4xx 契约）/ 取关
  {
    const r = await req(jar, 'GET', `/api/v1/users/${user}`, { headers: wechatHeaders('pages/user/user') });
    ok('GET /users/{username}（is_following/post_count）', r.ok && r.data.username === user);
  }

  // 21. 设备会话
  {
    const l = await req(jar, 'GET', '/api/v1/auth/sessions', { headers: wechatHeaders('pages/sessions/sessions') });
    const sid = l.data && l.data[0] && l.data[0].id;
    ok('GET /auth/sessions（设备列表）', l.ok && Array.isArray(l.data) && !!sid, JSON.stringify(l.data).slice(0, 120));
    if (sid) {
      const r = await req(jar, 'DELETE', `/api/v1/auth/sessions/${sid}`, {
        headers: { 'x-csrf-token': sCsrf, ...wechatHeaders('pages/sessions/sessions') },
      });
      ok('DELETE /auth/sessions/{id}（撤销后当前会话失效）', r.status < 300);
      // 当前会话已被撤销 → 下一次写请求应 401（会话 cookie 仍在但记录已撤销）
      const after = await req(jar, 'GET', '/api/v1/me', { headers: wechatHeaders('pages/profile/profile') }, { expect: 401 });
      ok('撤销当前会话后 GET /me → 401', after.status === 401);
    }
  }

  // 22. 搜索（worker 未运行时允许空结果）
  {
    const r = await req(jar, 'GET', `/api/v1/search?q=${encodeURIComponent('冒烟测试')}`, { headers: wechatHeaders('pages/search/search') });
    ok('GET /search（空结果或结果，结构合法）', r.ok && Array.isArray(r.data.items) && !!r.data.page);
  }

  // 23. 未登录写请求（无 cookie 无 preauth）→ 401
  {
    const anon = new Jar();
    const r = await req(anon, 'POST', '/api/v1/posts', {
      body: { type: 'topic', title: 'x', markdown: 'y', board_id: '00000000-0000-0000-0000-000000000000', access_policy: 'public', client_request_id: uuid() },
      headers: wechatHeaders('pages/create/create'),
      expect: 401,
    });
    ok('未登录 POST /posts → 401 authentication required', r.status === 401);
  }

  // 24. 来源校验（Referer 非白名单 → 400 origin_not_allowed）
  {
    const j2 = new Jar();
    await req(j2, 'GET', '/api/v1/auth/csrf');
    const t = (await req(j2, 'GET', '/api/v1/auth/csrf')).data.token;
    await req(j2, 'POST', '/api/v1/auth/register', {
      body: { username: 'badorigin_user', password: 'Smoke12345', email: 'badorigin@example.com' },
      headers: { 'x-csrf-token': t, referer: 'https://evil.example.com/' },
      expect: 400,
    });
    const r = await req(j2, 'POST', '/api/v1/auth/register', {
      body: { username: 'badorigin_user2', password: 'Smoke12345', email: 'badorigin2@example.com' },
      headers: { 'x-csrf-token': t, referer: 'https://evil.example.com/' },
      expect: 400,
    });
    ok('非白名单 Referer → 400 origin_not_allowed', r.status === 400 && r.data.code === 'origin_not_allowed');
  }

  console.log(`\n结果：${passed} 通过，${failed} 失败`);
  if (fails.length) {
    console.log('失败项：\n  - ' + fails.join('\n  - '));
    process.exit(1);
  }
}

main().catch((e) => {
  console.error('冒烟测试异常：', e);
  process.exit(1);
});
