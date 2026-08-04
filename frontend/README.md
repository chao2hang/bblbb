# BBLBB 前端

SvelteKit + TypeScript 的最小前端骨架，使用 `adapter-node`，面向文档约定的同源 SSR/API 部署方式。Node 版本固定为 22（仓库根 `.nvmrc`）。

## 开发

```sh
npm install
npm run dev             # 默认 http://localhost:5173
```

浏览器端通过同源 `/healthz` 展示后端健康状态。后续 SSR loader 可使用 `INTERNAL_API_ORIGIN` 访问内部 Rust API；不要将服务端密钥或内部地址暴露到客户端 bundle。

## 验证

```sh
npm run check
npm run build
npm run preview
```

业务 API 以仓库根目录的 `openapi/openapi.yaml` 为事实来源，浏览器 API 请求使用原生 `fetch` 和 `credentials: same-origin`。
