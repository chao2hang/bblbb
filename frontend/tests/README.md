# 前端冒烟检查说明（键盘/焦点/无 JS）

对应 M00-FRONTEND-07 / M00-FRONTEND-08。当前未引入测试框架依赖
（Playwright/Vitest 归 M02-UX 与 M00-TOOL-05），以下使用仓库现有能力：
`npm run check`（svelte-check，含 a11y 静态规则）、`npm run build`、
以及浏览器开发者工具/`curl` 手工冒烟。

## 1. 自动检查（现有能力，无新依赖）

```sh
cd frontend
npm run check   # svelte-check：a11y 静态规则（warnings 全清，0 errors 0 warnings）
npm run build   # adapter-node 构建，验证 SSR 产物可生成
```

## 2. 键盘 / 焦点手工冒烟清单

- [ ] Tab 顺序：导航链接 → 搜索框 → 发布 → 通知 → 登录/注册，焦点环可见（`:focus-visible` 样式）。
- [ ] 移动端抽屉菜单：`Enter` 打开，`Escape` 关闭，关闭后焦点回到触发按钮。
- [ ] 用户下拉菜单：`Escape` 可收起；点击外部区域关闭。
- [ ] 登录/注册/发帖/回复表单：Tab 依次进入每个输入框与提交按钮，`Enter` 可提交。
- [ ] 表单错误：注册页字段错误通过 `aria-describedby` 关联到对应输入框；错误带 `role="alert"`。
- [ ] 图标按钮（通知、搜索提交、菜单）均有 `aria-label`。
- [ ] `prefers-reduced-motion` 下无强制动画（当前无过渡动画，符合基线）。

## 3. 无 JavaScript 基线评估（M00-FRONTEND-08）

结论（当前阶段）：

- **公开阅读**：所有页面（首页、板块、帖子详情、用户主页、搜索）均为 SvelteKit SSR
  渲染的静态 HTML + 客户端 `onMount` fetch 增强。禁用 JS 后导航链接、标题、公开正文
  可读；数据区域保持骨架态，不会报错。
- **登录/注册/发帖/回复**：依赖客户端 `fetch` 提交（渐进增强交互）。无 JS 时表单不可提交，
  但页面 HTML 本身可访问。认证裁决始终在后端，浏览器关闭 JS 不会改变安全边界。
- **决策**：v1 先保持"公开阅读无 JS 可用；关键表单为渐进增强"，**不**在本批次
  强行引入 server actions 重构（会改动交互与 CSRF 链路，成本高）。无 JS 表单提交
  的 `+page.server.ts` form action 方案留待 M02-UX-08 / M14-A11Y 批次实施。

## 4. 隐私 / 缓存冒烟核对（M00-FRONTEND-06/-09）

- [ ] 任意页面响应头含 `Cache-Control: private, no-store`（根 layout 设置，见
      `src/routes/+layout.server.ts`）。
- [ ] SSR HTML（`curl -s <url> | grep`）不含邮箱、Session token 或隐藏正文；
      用户态由客户端 fetch 后渲染，hydration payload 无私有字段。
- [ ] 错误页显示 request_id（登录/注册/发帖/帖子页/用户页已接入 `$lib/errors`）。
