# BBLBB — 全角色 RC 冒烟报告（M17-SMOKE-01..09）

> 执行：platform/quality-release；日期：2026-08-08。
> 自动化覆盖来源：`frontend/tests/playwright/*`（194 用例，desktop+mobile，真实后端 +
> seeded personas）、`frontend/src/lib/testing/ssr/*`（vitest no-JS）、`backend/tests/**`
> （147 个测试二进制）。每行标注自动化证据；未覆盖行由人工验收清单
> `reports/rc/smoke/checklist.md` 跟踪。

| 冒烟行 | 自动化证据 | 状态 |
|---|---|---|
| SMOKE-01 匿名公开浏览/搜索/RSS/主页/资料卡/公开媒体 | flows-public.spec.ts + feeds-seo-nojs.test.ts + seo.spec.ts | 绿 |
| SMOKE-02 注册/邮箱验证/重发/登录/登出/密码恢复/Session | flows-public.spec.ts（register/verify/login）+ auth-nojs-regression + session 后端套件 | 绿 |
| SMOKE-03 未验证/冷静期用户发帖/回复/上传/交易全部服务端拒绝 | flows-member.spec.ts persona 差异行（bob/cooldown）+ 后端 authz 套件 | 绿 |
| SMOKE-04 member 发文章/讨论/草稿/回复/可见性/编辑冲突/删除 | flows-member.spec.ts（发帖/回复）+ posts_* 后端套件 | 绿 |
| SMOKE-05 举报/审核/处罚/申诉/通知 + 范围/利益冲突/审计 | flows-member.spec.ts（举报/申诉）+ moderation 后端套件 | 绿 |
| SMOKE-06 附件/Cover/S3/local/配额/URL 重签/Range/删除保留 | flows-economy.spec.ts + storage/download 后端套件 + profile_cover.rs | 绿 |
| SMOKE-07 B币/签到/等级/商城/装扮/Reaction/补偿 | flows-economy.spec.ts + shop/economy/reactions 后端套件 | 绿 |
| SMOKE-08 管理员/版主越权/recent-auth/2FA/危险设置确认 | flows-admin.spec.ts + admin_* 后端套件 + mfa_stepup | 绿 |
| SMOKE-09 无 JS 公开阅读/关键表单退化 + 手机/键盘/减少动效 | nojs.spec.ts + responsive.spec.ts + keyboard-focus.spec.ts + vitest SSR no-JS 套件 | 绿 |

## 执行记录

- `npx playwright test` → 194 passed, 2 skipped（skipped = axe 扫描中 mobile 项目 2 项
  有意跳过项，见 a11y-axe.spec.ts）。
- `npm run test` → 567 passed。
- `cargo test --all-features` → 147 binaries 0 failures。
- axe：`tests/a11y/axe-report.json` serious/critical = 0。
- 浏览器版本/viewport/locale/commit：`tests/a11y/records.json`。
