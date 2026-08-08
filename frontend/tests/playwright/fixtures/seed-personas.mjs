#!/usr/bin/env node
// M14-A11Y-01/02：E2E 数据库 persona 种子脚本。
//
// 说明（诚实记录）：生产注册端点限流为每 IP 3 次/小时（REGISTER_IP_LIMIT，
// src/ratelimit.rs），因此最多 3 个 persona 走真实注册 HTTP 流程；其余 persona
// 直接写入 DB（含角色/处罚/会话），密码哈希为占位值（不用于密码登录，只用于
// 会话注入）。所有 persona 的会话（user_sessions 行 + sha256 token hash）均按
// 后端 create_session 的精确 schema 直接生成，随后浏览器注入 `__Host-bblbb_session`
// cookie 即成为真实认证会话；会话 CSRF 由 (session id, csrf_secret_hash) 确定性
// 派生（src/routes/auth.rs get_csrf_token），与真实登录会话行为一致。
//
// 输出：fixtures/personas.json —— 各 persona 的 username/password/session token/
// 角色/处罚，供 Playwright 用例读取。
//
// 依赖：运行中的后端（BBLBB__DATABASE_URL 指向 e2e DB）+ node:sqlite。
import { spawn } from 'node:child_process';
import { createHash, createHmac, randomBytes, randomUUID } from 'node:crypto';
import { DatabaseSync } from 'node:sqlite';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const BACKEND = process.env.BBLBB_E2E_BACKEND ?? 'http://127.0.0.1:8080';
const DB_PATH = process.env.BBLBB_E2E_DB;
const OUT_PATH = join(__dirname, 'personas.json');

if (!DB_PATH) {
  console.error('BBLBB_E2E_DB not set');
  process.exit(1);
}

const PASSWORD = 'E2e-test-pass-123!';
const PREAUTH_COOKIE = '__Host-bblbb_csrf';
const SESSION_COOKIE = '__Host-bblbb_session';

const db = new DatabaseSync(DB_PATH);
const now = () => Date.now();

/** 极简 cookie jar：维护 preauth/session cookie 字符串。 */
function makeJar() {
  const cookies = new Map();
  return {
    set(name, value) {
      cookies.set(name, value);
    },
    header() {
      return [...cookies.entries()].map(([k, v]) => `${k}=${v}`).join('; ');
    }
  };
}

async function fetchPreauth(jar) {
  const resp = await fetch(`${BACKEND}/api/v1/auth/csrf`, {
    headers: { Accept: 'application/json', Cookie: jar.header() || undefined }
  });
  if (!resp.ok) throw new Error(`preauth csrf failed: ${resp.status}`);
  const body = await resp.json();
  for (const line of resp.headers.getSetCookie()) {
    const [pair] = line.split(';');
    const eq = pair.indexOf('=');
    if (eq > 0) jar.set(pair.slice(0, eq), pair.slice(eq + 1));
  }
  return body.token;
}

async function api(method, path, { jar, token, body, extra = {}, raw = false } = {}) {
  const headers = { Accept: 'application/json', ...extra };
  if (body !== undefined) headers['Content-Type'] = 'application/json';
  if (token) headers['X-CSRF-Token'] = token;
  if (jar) {
    const cookie = jar.header();
    if (cookie) headers.Cookie = cookie;
  }
  const resp = await fetch(`${BACKEND}${path}`, { method, headers, body: body !== undefined ? JSON.stringify(body) : undefined });
  for (const line of resp.headers.getSetCookie()) {
    const [pair] = line.split(';');
    const eq = pair.indexOf('=');
    if (eq > 0) jar?.set(pair.slice(0, eq), pair.slice(eq + 1));
  }
  if (raw) return resp;
  const text = await resp.text();
  return { status: resp.status, body: text ? JSON.parse(text) : null };
}

async function register(username, email) {
  const jar = makeJar();
  const token = await fetchPreauth(jar);
  const resp = await api('POST', '/api/v1/auth/register', {
    jar,
    token,
    body: { username, email, password: PASSWORD }
  });
  if (resp.status !== 201 && resp.status !== 200) {
    throw new Error(`register ${username} failed: ${resp.status} ${JSON.stringify(resp.body)}`);
  }
  return { username, email, password: PASSWORD, session: null };
}

/** 按用户名取 user_id（DB 侧）。 */
function userIdByUsername(username) {
  const row = db.prepare('SELECT id FROM users WHERE username_normalized = ?').get(username);
  if (!row) throw new Error(`user ${username} not found in DB`);
  return row.id;
}

/** 按角色名取 role_id。 */
function roleIdByName(name) {
  const row = db.prepare('SELECT id FROM roles WHERE name = ?').get(name);
  if (!row) throw new Error(`role ${name} not found`);
  return row.id;
}

/** 直接插入用户（绕过注册限流），返回 user_id。 */
function insertUser(username, email) {
  const id = randomUUID();
  const nowMs = now();
  db.prepare(
    `INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, email_verified, email_verified_at, level, version, created_at, updated_at)
     VALUES (?, ?, ?, ?, 'active', 1, ?, 1, 1, ?, ?)`
  ).run(id, username.toLowerCase(), email.toLowerCase(), `placeholder-${createHash('sha256').update(id).digest('hex')}`, nowMs - 2 * 86400000, nowMs - 2 * 86400000, nowMs);
  return id;
}

/** 生成后端等价会话行（token → sha256 hash，csrf_secret_hash 同 hash）。
 *  空闲/绝对超时取长值（30 天），避免长 E2E 会话期间会话 idle 过期导致
 *  401 波动（后端 IDLE_TIMEOUT_MS 默认较短，测试会话须覆盖整个套件运行）。 */
function mintSession(userId) {
  const token = randomBytes(24).toString('base64url');
  const hash = createHash('sha256').update(token).digest('hex');
  const sessionId = randomUUID();
  const nowMs = now();
  db.prepare(
    `INSERT INTO user_sessions (id, user_id, token_hash, csrf_secret_hash, user_agent, created_at, last_seen_at, idle_expires_at, absolute_expires_at, version)
     VALUES (?, ?, ?, ?, 'bblbb-e2e', ?, ?, ?, ?, 0)`
  ).run(sessionId, userId, hash, hash, nowMs, nowMs, nowMs + 30 * 86400000, nowMs + 90 * 86400000);
  return token;
}

/** base32 解码（RFC 4648）。 */
function base32Decode(input) {
  const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';
  const bits = input.toUpperCase().replace(/=+$/g, '').split('').map((c) => alphabet.indexOf(c));
  let buffer = 0;
  let bitsLeft = 0;
  const out = [];
  for (const value of bits) {
    buffer = (buffer << 5) | value;
    bitsLeft += 5;
    if (bitsLeft >= 8) {
      out.push((buffer >>> (bitsLeft - 8)) & 0xff);
      bitsLeft -= 8;
    }
  }
  return Buffer.from(out);
}

/** TOTP 6 位码（HMAC-SHA1，30s 周期，与后端 totp_at 对齐）。 */
function totpCode(secretBase32) {
  const secret = base32Decode(secretBase32);
  const counter = Math.floor(now() / 1000 / 30);
  const buf = Buffer.alloc(8);
  buf.writeBigUInt64BE(BigInt(counter));
  const hmac = createHmac('sha1', secret).update(buf).digest();
  const offset = hmac[hmac.length - 1] & 0x0f;
  const code =
    (((hmac[offset] & 0x7f) << 24) |
      ((hmac[offset + 1] & 0xff) << 16) |
      ((hmac[offset + 2] & 0xff) << 8) |
      (hmac[offset + 3] & 0xff)) %
    1000000;
  return String(code).padStart(6, '0');
}

/** 为 elevated 角色账号完成 TOTP enrollment（M02-MFA-05 强制）。 */
async function enrollTotp(userId) {
  const token = mintSession(userId);
  const jar = makeJar();
  jar.set(SESSION_COOKIE, token);
  const csrfResp = await api('GET', '/api/v1/auth/csrf', { jar });
  const csrfToken = csrfResp.body?.token;
  if (!csrfToken) throw new Error('enroll: session csrf missing');
  const enroll = await api('POST', '/api/v1/auth/mfa/enroll', { jar, token: csrfToken });
  if (enroll.status !== 200) {
    // 可能已 enroll（幂等场景）——允许继续。
    if (enroll.status === 409) return;
    throw new Error(`enroll failed: ${enroll.status} ${JSON.stringify(enroll.body)}`);
  }
  const secret = enroll.body.secret_base32;
  const code = totpCode(secret);
  // 允许 ±1 窗口（30s 边界）：重试一次下一个窗口。
  let confirm = await api('POST', '/api/v1/auth/mfa/confirm', { jar, token: csrfToken, body: { code } });
  if (confirm.status !== 200) {
    await new Promise((r) => setTimeout(r, 2000));
    confirm = await api('POST', '/api/v1/auth/mfa/confirm', { jar, token: csrfToken, body: { code: totpCode(secret) } });
  }
  if (confirm.status !== 200) {
    throw new Error(`mfa confirm failed: ${confirm.status} ${JSON.stringify(confirm.body)}`);
  }
}

/** 造一个帖子（作为 alice）。 */
async function createPostAs(userId, { type, title, markdown, boardSlug, visibility = 'public' }) {
  const token = mintSession(userId);
  const jar = makeJar();
  jar.set(SESSION_COOKIE, token);
  const csrfResp = await api('GET', '/api/v1/auth/csrf', { jar });
  const csrfToken = csrfResp.body?.token;
  const boardRow = db.prepare('SELECT id FROM boards WHERE slug = ?').get(boardSlug);
  if (!boardRow) throw new Error(`board ${boardSlug} not found`);
  const resp = await api('POST', '/api/v1/posts', {
    jar,
    token: csrfToken,
    body: {
      type,
      title,
      markdown,
      board_id: boardRow.id,
      visibility_level: 1,
      access_policy: 'public',
      client_request_id: `e2e-${randomBytes(6).toString('hex')}`
    }
  });
  if (resp.status !== 201 && resp.status !== 200) {
    throw new Error(`createPost failed: ${resp.status} ${JSON.stringify(resp.body)}`);
  }
  return resp.body.id;
}

async function main() {
  const personas = {};

  // ── 1. 真实注册（受 IP 限流，最多 3 个）──────────────────────────────
  const alice = await register('alice', 'alice@e2e.example');
  const bob = await register('bob', 'bob@e2e.example');
  const carol = await register('carol', 'carol@e2e.example');
  personas.alice = { ...alice, persona: 'member' };
  personas.bob = { ...bob, persona: 'unverified' };
  personas.carol = { ...carol, persona: 'muted' };

  // ── 2. 直接 DB 插入用户（绕过注册限流）───────────────────────────────
  const daveId = insertUser('dave', 'dave@e2e.example');
  const modId = insertUser('mod', 'mod@e2e.example');
  const adminId = insertUser('admin', 'admin@e2e.example');
  const cooldownId = insertUser('cooldown', 'cooldown@e2e.example');
  personas.dave = { username: 'dave', email: 'dave@e2e.example', password: null, persona: 'banned' };
  personas.mod = { username: 'mod', email: 'mod@e2e.example', password: null, persona: 'moderator' };
  personas.admin = { username: 'admin', email: 'admin@e2e.example', password: null, persona: 'admin' };
  personas.cooldown = { username: 'cooldown', email: 'cooldown@e2e.example', password: null, persona: 'cooldown' };

  // ── 3. 验证状态：member/mod/admin/cooldown 已验证；bob 保持未验证 ──────
  for (const [key, meta] of Object.entries(personas)) {
    if (key === 'bob') continue; // unverified
    const userId = userIdByUsername(meta.username);
    const nowMs = now();
    db.prepare(
      "UPDATE users SET email_verified = 1, email_verified_at = ?, status = 'active' WHERE id = ?"
    ).run(nowMs - 2 * 86400000, userId);
  }
  // cooldown：刚注册+刚验证（resend 冷却 60s 场景，注册即触发）。
  const cooldownRow = db.prepare('SELECT id FROM users WHERE username_normalized = ?').get('cooldown');
  db.prepare("UPDATE users SET email_verified = 1, email_verified_at = ?, status = 'active' WHERE id = ?").run(now(), cooldownRow.id);

  // ── 4. 会话铸造（全部 persona）──────────────────────────────────────
  for (const [key, meta] of Object.entries(personas)) {
    const userId = userIdByUsername(meta.username);
    meta.session = mintSession(userId);
    meta.user_id = userId;
  }

  // ── 5. TOTP enrollment（admin/mod：elevated 角色强制）＋角色分配 ───────
  await enrollTotp(personas.admin.user_id);
  await enrollTotp(personas.mod.user_id);

  db.prepare(
    'INSERT INTO user_roles (user_id, role_id, granted_by, granted_at, expires_at) VALUES (?, ?, NULL, ?, NULL)'
  ).run(personas.admin.user_id, roleIdByName('administrator'), now() - 60000);
  db.prepare(
    'INSERT INTO user_roles (user_id, role_id, granted_by, granted_at, expires_at) VALUES (?, ?, NULL, ?, NULL)'
  ).run(personas.mod.user_id, roleIdByName('global_moderator'), now() - 60000);

  // ── 6. 处罚：carol=timed mute（ends_at 未来，mute_until 生效），dave=ban ──
  // 注意：后端 mute 门只对「有 ends_at 的临时 mute」计算 mute_until
  // （src/authz/enforce.rs filter_map ends_at）；永久 mute（ends_at NULL）
  // 不进入 mute_until，无法触发发帖门 —— 因此用 1 小时临时 mute。
  const createdBy = personas.admin.user_id;
  const muteUntil = now() + 3600_000;
  db.prepare(
    `INSERT INTO sanctions (id, user_id, board_id, kind, status, reason, starts_at, ends_at, created_by, created_at)
     VALUES (?, ?, NULL, 'mute', 'active', 'e2e seed', ?, ?, ?, ?)`
  ).run(randomUUID(), personas.carol.user_id, now() - 60000, muteUntil, createdBy, now() - 60000);
  db.prepare(
    `INSERT INTO sanctions (id, user_id, board_id, kind, status, reason, starts_at, ends_at, created_by, created_at)
     VALUES (?, ?, NULL, 'ban', 'active', 'e2e seed', ?, NULL, ?, ?)`
  ).run(randomUUID(), personas.dave.user_id, now() - 60000, createdBy, now() - 60000);

  // ── 7. 内容：alice 发文章/讨论；一条 hidden 帖子（SEO noindex 场景）───
  const articleId = await createPostAs(personas.alice.user_id, {
    type: 'article',
    title: 'E2E 公开文章：Rust 所有权入门',
    markdown: '## 所有权\n\nRust 的所有权系统是内存安全的基础。',
    boardSlug: 'tech'
  });
  await createPostAs(personas.alice.user_id, {
    type: 'discussion',
    title: 'E2E 讨论：你最喜欢的 Rust 特性？',
    markdown: '我最近在学 Rust，最喜欢模式匹配。',
    boardSlug: 'general'
  });
  // hidden 帖子：详情对非作者 404（SEO 隐藏内容场景）。
  db.prepare("UPDATE posts SET status = 'hidden' WHERE id = ?").run(articleId);
  personas.alice.hidden_post_id = articleId;

  writeFileSync(OUT_PATH, JSON.stringify({ personas, backend: BACKEND, password: PASSWORD }, null, 2));
  console.log(`personas seeded → ${OUT_PATH}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
