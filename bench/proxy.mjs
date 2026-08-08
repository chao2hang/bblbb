// M16-PERF-03 辅助：release 前端 + /api 反代（模拟生产 Caddy 拓扑）。
// 用法：SSR_PORT=<ssrPort> node bench/proxy.mjs <listenPort> <backendPort>
//   - /api、/healthz、/readyz → 后端（127.0.0.1:<backendPort>）
//   - 其余（SSR 页面 + 静态资源）→ SvelteKit adapter-node（127.0.0.1:<SSR_PORT>）
import http from 'node:http';

const [listenPort, backendPort] = process.argv.slice(2);
const ssrPort = Number(process.env.SSR_PORT || 0);
if (!listenPort || !backendPort || !ssrPort) {
  console.error('usage: SSR_PORT=<ssrPort> node bench/proxy.mjs <listenPort> <backendPort>');
  process.exit(1);
}

const server = http.createServer((req, res) => {
  const target = req.url.startsWith('/api/') || req.url.startsWith('/healthz') || req.url.startsWith('/readyz')
    ? Number(backendPort)
    : ssrPort;
  const proxy = http.request(
    { host: '127.0.0.1', port: target, path: req.url, method: req.method, headers: req.headers },
    (upstream) => {
      res.writeHead(upstream.statusCode, upstream.headers);
      upstream.pipe(res);
    },
  );
  proxy.on('error', () => {
    res.writeHead(502, { 'content-type': 'application/json' });
    res.end('{"code":"internal_error","title":"proxy upstream unavailable"}');
  });
  req.pipe(proxy);
});

server.listen(Number(listenPort), '127.0.0.1', () => {
  console.log(`proxy listening on 127.0.0.1:${listenPort} -> /api backend 127.0.0.1:${backendPort}, SSR 127.0.0.1:${ssrPort}`);
});
