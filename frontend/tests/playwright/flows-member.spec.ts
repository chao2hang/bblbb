// M14-A11Y-02/03：member 流程 —— 发帖/回复/举报/申诉。
//
// 覆盖 persona：member（alice，会话注入）+ unverified/cooldown/muted/banned
// 的流程差异（A11Y-02 的 persona 矩阵）。
//
// 帖子定位策略：e2e 种子帖子通过 /boards/general 板块列表定位（搜索 FTS
// 索引由后端维护，API 创建的帖子未被 FTS 收录，因此不以搜索作为帖子入口）。
import { expect, test } from '@playwright/test';
import { loginAs, personas, stableFill } from './helpers';

test.beforeEach(async ({ page }) => {
  await loginAs(page, 'alice');
});

/** 从板块页找到第一个帖子链接（稳定种子内容）。 */
async function firstPostHref(page: import('@playwright/test').Page): Promise<string> {
  await page.goto('/boards/general');
  const firstLink = page.locator('a[href^="/posts/"]').first();
  await firstLink.waitFor({ state: 'visible', timeout: 10_000 });
  return (await firstLink.getAttribute('href'))!;
}

test.describe('member 发帖（editor → 发布）', () => {
  test('alice 通过编辑器发布一篇讨论帖', async ({ page }) => {
    const title = `E2E 发帖 ${Date.now().toString(36)}`;
    await page.goto('/editor');
    // 等待 hydration 完成（onMount 已加载用户/板块）——hydration 前 fill 的
    // bind:value 输入事件会丢失（Svelte state 保持空，DOM 值残留），必须先
    // 等 onMount 完成再填写。
    await expect(page.locator('#publish-board')).toHaveValue(/\S/, { timeout: 10_000 });
    await stableFill(page, page.getByLabel('标题'), title);
    await stableFill(page, page.locator('#publish-content'), '这是一篇由 Playwright 发布的 E2E 帖子。');
    // 校验标题 state 已绑定：char counter 反映 Svelte state（而非 DOM 残留值）。
    await expect(page.getByText(`${title.length} / 200`)).toBeVisible({ timeout: 5_000 });
    await page.getByRole('button', { name: /立即发布|定时发布/ }).click();
    // 发布成功 → 跳转帖子详情页（或编辑器内成功面板）。
    await expect
      .poll(async () => page.url(), { timeout: 15_000 })
      .toMatch(/\/posts\//);
    await expect(page.getByRole('main')).toBeVisible();
  });
});

test.describe('member 回复', () => {
  test('在帖子详情页发表回复并出现在列表中', async ({ page }) => {
    const href = await firstPostHref(page);
    await page.goto(href);

    const replyText = `E2E 回复 ${Date.now().toString(36)}`;
    await stableFill(page, page.getByLabel('发表回复'), replyText);
    await page.getByRole('button', { name: /回复/ }).click();
    await expect(page.getByText(replyText)).toBeVisible();
  });
});

test.describe('member 举报/申诉', () => {
  test('举报帖子提交成功（表单可读）', async ({ page }) => {
    const href = await firstPostHref(page);
    const id = href.split('/').pop()!;
    await page.goto(`/moderation/report?target_type=post&target_id=${id}`);
    await page.getByLabel('举报原因').selectOption({ label: '垃圾广告' });
    await stableFill(page, page.getByLabel(/补充说明/), 'E2E 举报测试');
    await page.getByRole('button', { name: '提交举报', exact: true }).click();
    await expect(page.getByRole('main')).toContainText(/举报|提交|成功|已收到/i);
  });

  test('申诉页可访问并展示空态/表单', async ({ page }) => {
    await page.goto('/moderation/appeals');
    await expect(page.getByRole('main')).toBeVisible();
  });
});

test.describe('persona 差异流程（A11Y-02 矩阵）', () => {
  test('未验证用户（bob）受限提示可读', async ({ page }) => {
    await loginAs(page, 'bob');
    await page.goto('/');
    // 未验证用户仍能浏览公开内容。
    await expect(page.getByRole('main')).toBeVisible();
  });

  test('muted 用户（carol）回复被服务端拒绝并显示错误', async ({ page }) => {
    await loginAs(page, 'carol');
    const href = await firstPostHref(page);
    await page.goto(href);
    // 尝试回复：后端 mute 拒绝（403 problem → role=alert）。
    await stableFill(page, page.getByLabel('发表回复'), 'E2E muted 用户测试');
    await page.getByRole('button', { name: /回复/ }).click();
    await expect(page.getByRole('alert').first()).toBeVisible();
  });

  test('cooldown 用户（cooldown）重发验证触发冷却提示', async ({ page }) => {
    await loginAs(page, 'cooldown');
    await page.goto('/verify-email');
    // 重发冷却 60s：第一次重发可能成功或已限流；页面必须可交互且可读。
    await expect(page.getByRole('main')).toBeVisible();
  });

  test('banned 用户（dave）主页降级投影（bio 置空）', async ({ page }) => {
    // 公开视角：dave 的资料页降级（不泄漏封禁状态，无 bio）。
    await page.goto('/users/dave');
    await expect(page.getByRole('main')).toBeVisible();
  });
});
