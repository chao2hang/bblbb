# BBLBB — RC 人工验收清单（M16-RELEASE-TEST-05）

> 用途：v1.0 RC 前逐 persona 人工验收。标记 `[自动]` 的项由自动化测试覆盖
> （无需人工重复），`[人工]` 项必须由 reviewer 在本地/预发布环境执行并在下方签名。

## Persona × 验收矩阵

| Persona | 验收项 | 覆盖 |
|---|---|---|
| anonymous | 公开浏览首页/板块/文章；搜索；注册入口；无 JS 浏览 | `[自动]` Playwright `flows-public.spec.ts` + `nojs.spec.ts`；`[人工]` 浏览器无痕浏览 3 页+搜索 |
| unverified | 登录后看到验证提示；不能发帖（403/重定向）；重发验证邮件限流 | `[自动]` `authz_persona.rs` + Playwright 注册流程；`[人工]` 注册→收验证邮件→点击链接 |
| cooldown（新用户冷静期） | 24h 内发帖进入 pending 或被拒；不绕过 | `[自动]` `posts_publish_preflight.rs` + `authz_persona.rs` |
| member | 发帖/回复/举报/解锁付费内容/积分消费；编辑自己的内容；上传附件 | `[自动]` `flows-member.spec.ts` + 后端 posts/comments/reactions/storage 测试 |
| moderator | 板块内隐藏/恢复/处罚；跨板块无权限；审计留痕 | `[自动]` `authz_persona.rs` + `moderation/*` 测试 + admin Playwright |
| administrator | 用户/角色/板块/配置管理；reason+recent-auth+If-Match 强制；危险确认 | `[自动]` `flows-admin.spec.ts` + `admin_routes.rs` + `admin-*-nojs.test.ts` |
| mute / board_mute / banned | 实时生效；回复/发帖/登录被拒或受限；到期自动恢复 | `[自动]` `authz_persona.rs` + `moderation/sanctions.rs` + Playwright persona |
| restricted | 只读或受限权限；不能提升自己 | `[自动]` `authz_roles.rs` + `authz_persona.rs` |
| 数据主体（privacy） | 数据导出含本人数据；注销后匿名化（讨论保留）；30 天延迟期可撤销；法律保留暂停删除 | `[自动]` `account_deletion.rs` + `deletion_lifecycle.rs`；`[人工]` 真实导出 zip 打开核对 |

## 人工验收记录

| 日期 | reviewer | persona 子集 | 结果 | 备注 |
|---|---|---|---|---|
| （RC 前） | platform/quality-release | anonymous/member/admin | 待执行 | 见 `reports/rc/release-test.md` §5 环境 |

## 无 JS / a11y / 移动端

| 项 | 覆盖 |
|---|---|
| 无 JS 公开浏览 + 注册/登录/搜索表单 | `[自动]` `nojs.spec.ts` + `*-nojs.test.ts`（vitest SSR） |
| axe serious/critical = 0 | `[自动]` `a11y-axe.spec.ts`（artifact `tests/a11y/axe-report.json`） |
| 键盘导航/焦点/减少动效 | `[自动]` `keyboard-focus.spec.ts` |
| 移动端/放大/慢网络 | `[自动]` `responsive.spec.ts`（mobile-chromium 项目） |

## 验收命令

```sh
cd frontend && npx playwright test          # 全量 E2E + axe
cd frontend && npm run test:e2e:axe          # 仅 axe
bash ops/smoke/smoke.sh --base-url <url> --db <db>   # 发布后冒烟
```
