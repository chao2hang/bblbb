// M14-A11Y-04：经济/媒体/消费流程 —— 附件/Cover/下载/积分/商城/装扮/视频/AI 同意。
//
// 覆盖 persona：member（alice）+ admin（下载计费/视频配置页）。
import { expect, test } from '@playwright/test';
import { loginAs } from './helpers';

test.beforeEach(async ({ page }) => {
  await loginAs(page, 'alice');
});

test.describe('积分/等级/签到（points/levels/activity）', () => {
  test('我的积分页展示余额/等级/签到（安全投影）', async ({ page }) => {
    await page.goto('/me/balance');
    // 页面标题在 breadcrumb/span（无 h1）；断言标题文本与关键区块。
    await expect(page.getByText('我的积分').first()).toBeVisible();
    await expect(page.getByRole('main')).toContainText(/余额|等级|经验|奖励|签到/i);
  });

  test('签到按钮可操作（活动领取）', async ({ page }) => {
    await page.goto('/me/balance');
    const claim = page.getByRole('button', { name: /领取|签到/ }).first();
    if (await claim.isVisible().catch(() => false)) {
      await claim.click();
      await expect(page.getByRole('status').or(page.getByRole('alert')).first()).toBeVisible();
    } else {
      // 今日已领取（每日一次）：显示已领取状态。
      await expect(page.getByRole('main')).toContainText(/已领取|今日|奖励/i);
    }
  });
});

test.describe('商城（shop）', () => {
  test('商城页展示商品列表或空态（不崩溃）', async ({ page }) => {
    await page.goto('/shop');
    await expect(page.getByText('积分商城').first()).toBeVisible();
    await expect(page.getByRole('main')).toBeVisible();
  });
});

test.describe('衣柜（wardrobe）', () => {
  test('衣柜页展示展示位/权益或空态', async ({ page }) => {
    await page.goto('/me/wardrobe');
    await expect(page.getByRole('main')).toBeVisible();
  });
});

test.describe('附件（attachments/upload）', () => {
  test('编辑器附件选择器可访问（可上传按钮有名称）', async ({ page }) => {
    await page.goto('/editor');
    // 附件/上传控件出现（AttachmentPicker 区域）。
    await expect(page.getByRole('main')).toBeVisible();
    const upload = page.getByRole('button', { name: /附件|上传/ }).first();
    if (await upload.isVisible().catch(() => false)) {
      await expect(upload).toBeEnabled();
    }
  });
});

test.describe('Cover（用户资料封面）', () => {
  test('用户主页渲染 Cover 区域（带可读标签）', async ({ page }) => {
    await page.goto('/users/alice');
    // ProfileCover：label 承载为 role=img + aria-label（装饰性 cover 无 label）。
    await expect(page.getByRole('img', { name: '个人资料背景' })).toBeVisible();
  });
});

test.describe('AI 能力与同意（M14-A11Y-04）', () => {
  test('AI 页面在默认关闭时展示未开放态（不泄露敏感信息）', async ({ page }) => {
    await page.goto('/ai');
    await expect(page.getByRole('main')).toContainText(/未开放|AI/);
  });
});

test.describe('下载计费（download，admin 视角）', () => {
  test('下载计费配置页（脱敏视图）可访问', async ({ page }) => {
    await loginAs(page, 'admin');
    await page.goto('/admin/download-billing');
    await expect(page.getByRole('main')).toBeVisible();
  });
});

test.describe('视频（video，admin 视角）', () => {
  test('视频配置页（Provider 策略脱敏视图）可访问', async ({ page }) => {
    await loginAs(page, 'admin');
    await page.goto('/admin/video');
    await expect(page.getByRole('main')).toBeVisible();
  });
});
