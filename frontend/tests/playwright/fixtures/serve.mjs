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
const BACKEND_BIN = join(REPO, 'backend', 'target', 'debug', 'bblbb-backend');
const DB_PATH = join(REPO, 'data', 'e2e.sqlite');
const VITE_BIN = join(FRONTEND, 'node_modules', '.bin', 'vite');

const BACKEND_PORT = 8080;
const FRONTEND_PORT = 4173;
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
        BBLBB__PUBLIC_ORIGIN: `http://127.0.0.1:${FRONTEND_PORT}`,
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
      BBLBB_E2E_DB: DB_PATH
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
  const vite = spawnChild('vite', VITE_BIN, ['dev', '--port', String(FRONTEND_PORT), '--strictPort'], {
    cwd: FRONTEND
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
