# BBLBB 前端

SvelteKit + TypeScript 的最小前端骨架，使用 `adapter-node`，面向文档约定的同源 SSR/API 部署方式。Node 版本固定为 22（仓库根 `.nvmrc`）。

## 开发

```sh
npm install
npm run dev -- --host 0.0.0.0 --port 5173
```

开发服务器默认启用 HTTPS，并监听 `0.0.0.0:5173`，因此远程浏览器使用：

```text
https://10.10.10.10:5173/
```

仓库内 `dev/certs/dev.crt` / `dev/certs/dev.key` 会自动加载，证书路径不依赖启动时的当前目录。证书是开发自签名证书，首次访问需要在浏览器中接受证书警告。可通过 `BBLBB_DEV_TLS_CERT`、`BBLBB_DEV_TLS_KEY` 覆盖证书，通过 `BBLBB_DEV_HOST=127.0.0.1` 恢复仅本机监听。

浏览器端通过同源 `/healthz` 展示后端健康状态。后续 SSR loader 可使用 `INTERNAL_API_ORIGIN` 访问内部 Rust API；不要将服务端密钥或内部地址暴露到客户端 bundle。

## 设计规范

前端组件、按钮、页面布局、状态和 BLBUI 接入边界见 [`../docs/BLBUI-DESIGN-SYSTEM.md`](../docs/BLBUI-DESIGN-SYSTEM.md)。

## 验证

```sh
npm run check
npm run build
npm run preview
```

业务 API 以仓库根目录的 `openapi/openapi.yaml` 为事实来源，浏览器 API 请求使用原生 `fetch` 和 `credentials: same-origin`。
