#!/usr/bin/env node
// M14-A11Y-01：Playwright webServer 编排脚本。
//
// 职责（一条命令启动 E2E 全栈）：
//   1. 清理并重建 e2e SQLite 库（data/e2e.sqlite）；
//   2. 以 --migrate 启动真实 Rust 后端（BBLBB__DATABASE_URL 指向 e2e 库），
//      等待 /healthz；
//   3. 运行 seed-personas.mjs 铸成 persona（DB 会话 + 角色 + 处罚 + 内容）；
//   4. 启动 vite dev（--port 4173 --strictPort），/api 代理到后端；
//   5. 保持存活直至收到 SIGTERM/SIGINT，随后按序关闭子进程。
//
// 该编排脚本是 Playwright config 的 webServer.command；可单独运行以复现
// E2E 环境（`node tests/playwright/fixtures/serve.mjs`）。
import { spawn } from 'node:child_process';
import { existsSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO = join(__dirname, '..', '..', '..', '..');
const FRONTEND = join(REPO, 'frontend');
function resolveBackendBin() {
  const explicit = process.env.E2E_BACKEND_BIN;
  if (explicit) {
    if (existsSync(explicit)) return explicit;
    throw new Error(`[serve:error] 环境变量 E2E_BACKEND_BIN 指定的文件不存在: ${explicit}`);
  }

  const candidates = [
    process.env.CARGO_TARGET_DIR ? join(process.env.CARGO_TARGET_DIR, 'debug', 'bblbb-backend') : null,
    join(REPO, 'target', 'debug', 'bblbb-backend'),
    '/data/cargo-target/bblbb/debug/bblbb-backend',
    join(REPO, 'backend', 'target', 'debug', 'bblbb-backend'),
    join(REPO, 'target', 'release', 'bblbb-backend'),
    join(REPO, 'backend', 'target', 'release', 'bblbb-backend'),
  ].filter(Boolean);

  for (const c of candidates) {
    if (existsSync(c)) {
      return c;
    }
  }

  throw new Error(
    `[serve:error] 未找到后端二进制文件 bblbb-backend。\n` +
    `已尝试探测以下候选路径：\n${candidates.map((p) => `  - ${p}`).join('\n')}\n` +
    `请先执行 cargo build 构建后端（或通过 E2E_BACKEND_BIN 环境变量显式指定二进制路径）。`
  );
}

const BACKEND_BIN = resolveBackendBin();
// 视觉检测等复用方可用环境变量改端口/库/输出；默认值与 Playwright 语义不变。
const DB_PATH = join(REPO, 'data', process.env.E2E_DB_PATH ?? 'e2e.sqlite');
const VITE_BIN = join(FRONTEND, 'node_modules', '.bin', 'vite');

const BACKEND_PORT = Number(process.env.E2E_BACKEND_PORT ?? 8080);
const FRONTEND_PORT = Number(process.env.E2E_FRONTEND_PORT ?? 4173);
const PUBLIC_ORIGIN = process.env.E2E_PUBLIC_ORIGIN ?? `https://localhost:${FRONTEND_PORT}`;
const ALLOWED_ORIGINS = process.env.E2E_ALLOWED_ORIGINS ?? PUBLIC_ORIGIN;
const HEALTH_URL = `http://127.0.0.1:${BACKEND_PORT}/healthz`;

const children = [];

function log(prefix, message) {
  process.stdout.write(`[serve:${prefix}] ${message}\n`);
}

function spawnChild(name, command, args, opts = {}) {
  const child = spawn(command, args, { stdio: ['ignore', 'pipe', 'pipe'], ...opts });
  child.stdout.on('data', (d) => {
    const text = String(d).trim();
    if (text) log(name, text.split('\n').slice(0, 20).join('\n  '));
  });
  child.stderr.on('data', (d) => {
    const text = String(d).trim();
    if (text) log(name, `stderr: ${text.split('\n').slice(0, 10).join('\n  ')}`);
  });
  children.push(child);
  return child;
}

async function waitForHealth(timeoutMs = 30000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const resp = await fetch(HEALTH_URL);
      if (resp.ok) return true;
    } catch {
      /* backend not up yet */
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error('backend did not become healthy in time');
}

function cleanup(exitCode = 0) {
  for (const child of children.reverse()) {
    try {
      child.kill('SIGTERM');
    } catch {
      /* ignore */
    }
  }
  setTimeout(() => process.exit(exitCode), 800);
}

process.on('SIGTERM', () => cleanup(0));
process.on('SIGINT', () => cleanup(0));

async function main() {
  // 1. 清理旧 e2e 库（含 WAL/SHM）。
  for (const suffix of ['', '-wal', '-shm']) {
    const path = `${DB_PATH}${suffix}`;
    if (existsSync(path)) rmSync(path);
  }

  // 2. 启动后端（迁移 + 角色种子）。
  const backend = spawnChild(
    'backend',
    BACKEND_BIN,
    ['--migrate'],
    {
      cwd: join(REPO, 'backend'),
      env: {
        ...process.env,
        BBLBB__DATABASE_URL: `sqlite://${DB_PATH}`,
        BBLBB__MFA_ENCRYPTION_KEY: 'e2e-mfa-encryption-key-0000',
        BBLBB__PUBLIC_ORIGIN: PUBLIC_ORIGIN,
         BBLBB__ALLOWED_ORIGINS: ALLOWED_ORIGINS,
        // 绑定地址跟随 BACKEND_PORT（默认 127.0.0.1:8080 与旧行为一致）。
        BBLBB__BIND_ADDRESS: `127.0.0.1:${BACKEND_PORT}`,
        BBLBB__LOG_FILTER: 'info'
      }
    }
  );

  await waitForHealth();
  log('main', 'backend healthy');

  // 3. 铸 persona。
  const seed = spawn('node', [join(__dirname, 'seed-personas.mjs')], {
    cwd: FRONTEND,
    env: {
      ...process.env,
      BBLBB_E2E_BACKEND: `http://127.0.0.1:${BACKEND_PORT}`,
      BBLBB_E2E_DB: DB_PATH,
      ...(process.env.BBLBB_E2E_PERSONAS
        ? { BBLBB_E2E_PERSONAS: process.env.BBLBB_E2E_PERSONAS }
        : {})
    },
    stdio: ['ignore', 'inherit', 'inherit']
  });
  const seedExit = await new Promise((resolve) => seed.on('exit', resolve));
  if (seedExit !== 0) {
    log('main', `seed failed with exit ${seedExit}`);
    cleanup(1);
    return;
  }
  log('main', 'personas seeded');

  // 4. 启动 vite dev。
  // INTERNAL_API_ORIGIN：SSR 侧 fetch 的 API 基址（默认 127.0.0.1:8080），
  // 必须与 BACKEND_PORT 一致，否则多实例并行时 SSR 会打到别的后端。
  const vite = spawnChild('vite', VITE_BIN, ['dev', '--port', String(FRONTEND_PORT), '--strictPort'], {
    cwd: FRONTEND,
    env: {
      ...process.env,
      INTERNAL_API_ORIGIN: `http://127.0.0.1:${BACKEND_PORT}`,
       E2E_API_TARGET: `http://127.0.0.1:${BACKEND_PORT}`
    }
  });
  await new Promise((r) => setTimeout(r, 4000));

  // 5. 存活直到被信号终止。
  await new Promise((resolve) => {
    for (const child of children) {
      child.on('exit', resolve);
    }
  });
  cleanup(0);
}

main().catch((err) => {
  log('main', `fatal: ${err.message}`);
  cleanup(1);
});
