#!/usr/bin/env node
/* BBLBB 原型静态端口
 * 零依赖 Node 静态服务器：把 prototype/ 以 HTTP 服务起来，供浏览器与验收脚本访问。
 *
 * 用法:
 *   node serve.mjs                          # 默认 http://127.0.0.1:8765
 *   PROTOTYPE_PORT=9000 node serve.mjs      # 换端口
 *   PROTOTYPE_HOST=0.0.0.0 node serve.mjs   # 对外暴露（调试/分享时用）
 *
 * 约定:
 *   - 端口 8765 与 verify.mjs / verify-mock-runtime.mjs / visual-score.mjs 的
 *     PROTOTYPE_BASE 默认值保持一致，无需额外配置即可跑验收。
 *   - 原型是 Hash 路由 SPA（入口 index.html），无扩展名的未知路径回退到 index.html。
 *   - 全部响应 no-cache：原型资产无 hash 指纹，开发期刷新必须拿到最新文件。
 */
import { createServer } from 'node:http';
import { createReadStream, existsSync, statSync } from 'node:fs';
import { extname, join, normalize, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(fileURLToPath(new URL('.', import.meta.url)));
const HOST = process.env.PROTOTYPE_HOST || '127.0.0.1';
const PORT = Number.parseInt(process.env.PROTOTYPE_PORT || '8765', 10);

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.webmanifest': 'application/manifest+json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.gif': 'image/gif',
  '.webp': 'image/webp',
  '.avif': 'image/avif',
  '.ico': 'image/x-icon',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
  '.ttf': 'font/ttf',
  '.txt': 'text/plain; charset=utf-8',
  '.md': 'text/plain; charset=utf-8',
  '.mp4': 'video/mp4',
  '.webm': 'video/webm',
  '.mp3': 'audio/mpeg',
  '.wasm': 'application/wasm',
  '.map': 'application/json',
};

function notFound(res, message = '404 Not Found') {
  res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
  res.end(message);
}

function sendFile(res, filePath, headOnly) {
  const type = MIME[extname(filePath).toLowerCase()] || 'application/octet-stream';
  let size;
  try {
    size = statSync(filePath).size;
  } catch {
    notFound(res);
    return;
  }
  res.writeHead(200, {
    'Content-Type': type,
    'Content-Length': size,
    'Cache-Control': 'no-cache',
  });
  if (headOnly) {
    res.end();
    return;
  }
  const stream = createReadStream(filePath);
  stream.on('error', () => res.destroy());
  stream.pipe(res);
}

/** 把请求路径解析到 ROOT 内的真实文件；越界返回 null。 */
function resolveUnderRoot(urlPath) {
  let pathname;
  try {
    pathname = decodeURIComponent(urlPath.split('?')[0].split('#')[0]);
  } catch {
    return null; // 非法百分号编码
  }
  if (pathname.includes('\0')) return null;
  const candidate = normalize(join(ROOT, pathname));
  if (candidate !== ROOT && !candidate.startsWith(ROOT + sep)) return null;
  return candidate;
}

const server = createServer((req, res) => {
  const url = (req.url || '/').split('?')[0].split('#')[0];
  const headOnly = req.method === 'HEAD';
  let filePath;

  try {
    filePath = resolveUnderRoot(req.url || '/');
  } catch {
    filePath = null;
  }

  if (!filePath || req.method !== 'GET' && req.method !== 'HEAD') {
    if (filePath) {
      res.writeHead(405, { 'Content-Type': 'text/plain; charset=utf-8', Allow: 'GET, HEAD' });
      res.end('405 Method Not Allowed');
    } else {
      notFound(res, filePath ? '405 Method Not Allowed' : '404 Not Found');
    }
    return;
  }

  if (filePath === ROOT) {
    filePath = join(ROOT, 'index.html');
  } else if (existsSync(filePath) && statSync(filePath).isDirectory()) {
    const indexInDir = join(filePath, 'index.html');
    filePath = existsSync(indexInDir) ? indexInDir : null;
    if (!filePath) {
      notFound(res, '404 Not Found (no index in directory)');
      return;
    }
  }

  if (!filePath || !existsSync(filePath)) {
    // Hash SPA 回退：无扩展名的未知路径视为前端路由，交给 index.html 处理。
    if (!extname(url)) {
      sendFile(res, join(ROOT, 'index.html'), headOnly);
      return;
    }
    notFound(res);
    return;
  }

  sendFile(res, filePath, headOnly);
  console.log(`${req.method} ${url} ${res.statusCode}`);
});

server.on('error', (err) => {
  if (err.code === 'EADDRINUSE') {
    console.error(`端口 ${PORT} 已被占用：${err.message}\n用 PROTOTYPE_PORT=<其他端口> node serve.mjs 换一个端口。`);
  } else {
    console.error(err);
  }
  process.exit(1);
});

server.listen(PORT, HOST, () => {
  console.log(`BBLBB 原型端口已启动：http://${HOST}:${PORT}/`);
  console.log(`静态根目录：${ROOT}`);
  console.log('Ctrl+C 停止。验收脚本默认 BASE 即此端口（verify.mjs / verify-mock-runtime.mjs / visual-score.mjs）。');
});
