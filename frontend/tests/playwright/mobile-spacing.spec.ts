import { expect, test } from '@playwright/test';
import { loginAs } from './helpers';

const MEMBER_ROUTES = [
  '/me/balance',
  '/me',
  '/me/level',
  '/me/billing',
  '/me/drafts',
  '/me/wardrobe',
  '/me/attachments',
  '/me/security',
  '/settings',
  '/settings/privacy',
  '/notifications',
  '/favorites',
  '/search?q=mobile-spacing',
  '/boards',
  '/shop',
  '/marketplace',
  '/',
  '/discover'
];

test.describe('mobile spacing visual contracts', () => {
  test('mobile topic replies do not get pushed away by the empty state', async ({ page }) => {
    await loginAs(page, 'alice');
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto('/boards/tech', { waitUntil: 'domcontentloaded' });
    const postLink = page.locator('a[href^="/posts/"]').filter({ hasText: 'E2E 移动端间距测试：空回复区' }).first();
    await expect(postLink).toBeVisible();
    const postHref = await postLink.getAttribute('href');
    expect(postHref).toBeTruthy();
    await page.goto(postHref!, { waitUntil: 'domcontentloaded' });
    await expect(page.locator('.topic-comments__head')).toBeVisible();
    const emptyState = page.locator('.topic-comments > .app-empty');
    await expect(emptyState).toContainText('暂无回复');
    await expect(page.locator('.topic-editor')).toBeVisible();
    await expect(page.locator('.side-col')).toBeHidden();
    await expect(page.locator('.topic-side-board-nav')).toBeHidden();

    await page.setViewportSize({ width: 767, height: 844 });
    await expect(page.locator('.side-col')).toBeHidden();
    await page.setViewportSize({ width: 768, height: 844 });
    await expect(page.locator('.side-col')).toBeVisible();
    await expect(page.locator('.topic-side-board-nav')).toBeVisible();
    await page.setViewportSize({ width: 1280, height: 844 });
    await expect(page.locator('.side-col')).toBeVisible();
    await page.setViewportSize({ width: 390, height: 844 });

    const spacing = await page.evaluate(() => {
      const heading = document.querySelector('.topic-comments__head');
      const empty = document.querySelector('.topic-comments > .app-empty');
      const editor = document.querySelector('.topic-editor');
      if (!heading || !empty || !editor) return null;
      const headingRect = heading.getBoundingClientRect();
      const emptyRect = empty.getBoundingClientRect();
      const editorRect = editor.getBoundingClientRect();
      return {
        emptyHeight: Math.round(emptyRect.height),
        headingToEmpty: Math.round(emptyRect.top - headingRect.bottom),
        emptyToEditor: Math.round(editorRect.top - emptyRect.bottom),
        headingLeft: Math.round(headingRect.left),
        emptyLeft: Math.round(emptyRect.left),
        editorLeft: Math.round(editorRect.left),
        headingRight: Math.round(headingRect.right),
        emptyRight: Math.round(emptyRect.right),
        editorRight: Math.round(editorRect.right)
      };
    });

    expect(spacing).not.toBeNull();
    expect(spacing!.emptyHeight).toBeLessThanOrEqual(110);
    expect(spacing!.headingToEmpty).toBeLessThanOrEqual(12);
    expect(spacing!.emptyToEditor).toBeLessThanOrEqual(14);
    expect(spacing!.headingLeft).toBe(spacing!.emptyLeft);
    expect(spacing!.emptyLeft).toBe(spacing!.editorLeft);
    expect(spacing!.headingRight).toBe(spacing!.emptyRight);
    expect(spacing!.emptyRight).toBe(spacing!.editorRight);
  });

  test('wardrobe layout stays within the mobile viewport and clears the bottom navigation', async ({ page }) => {
    await loginAs(page, 'alice');

    for (const width of [360, 390, 430, 464]) {
      await page.setViewportSize({ width, height: 1040 });
      await page.goto('/me/wardrobe', { waitUntil: 'domcontentloaded' });
      await expect(page.locator('#page-wardrobe .wardrobe-header')).toBeVisible();
      const layout = await page.evaluate(() => {
        const canvas = document.querySelector<HTMLElement>('#page-wardrobe');
        const layout = document.querySelector<HTMLElement>('.wardrobe-layout');
        const equipment = document.querySelector<HTMLElement>('.wardrobe-equipment-deck');
        const inventory = document.querySelector<HTMLElement>('.wardrobe-inventory-deck');
        const tabs = document.querySelector<HTMLElement>('.inventory-tabs');
        const rects = [layout, equipment, inventory, tabs].filter((element): element is HTMLElement => Boolean(element))
          .map((element) => element.getBoundingClientRect());
        return {
          viewportWidth: document.documentElement.clientWidth,
          rootScrollWidth: document.documentElement.scrollWidth,
          canvasScrollWidth: canvas?.scrollWidth ?? 0,
          canvasClientWidth: canvas?.clientWidth ?? 0,
          allChildrenInsideViewport: rects.every((rect) => rect.left >= 0 && rect.right <= document.documentElement.clientWidth),
          layoutColumns: layout ? getComputedStyle(layout).gridTemplateColumns : '',
          bottomPadding: canvas ? Number.parseFloat(getComputedStyle(canvas).paddingBottom) : 0
        };
      });

      expect(layout.rootScrollWidth, `wardrobe root overflow at ${width}px`).toBeLessThanOrEqual(width);
      expect(layout.canvasScrollWidth, `wardrobe canvas overflow at ${width}px`).toBeLessThanOrEqual(layout.canvasClientWidth);
      expect(layout.allChildrenInsideViewport, `wardrobe child overflow at ${width}px`).toBe(true);
      expect(layout.layoutColumns.split(' ').length, `wardrobe should use one column at ${width}px`).toBe(1);
      expect(layout.bottomPadding, `wardrobe should leave room for bottom navigation at ${width}px`).toBeGreaterThanOrEqual(78);
    }
  });

  test('billing summary and empty state use a compact mobile layout', async ({ page }) => {
    await loginAs(page, 'alice');

    for (const width of [360, 390, 430, 464]) {
      await page.setViewportSize({ width, height: 1040 });
      await page.goto('/me/billing', { waitUntil: 'domcontentloaded' });
      await expect(page.locator('#page-billing')).toBeVisible();
      await expect(page.locator('#page-billing .empty-state')).toBeVisible();
      const layout = await page.evaluate(() => {
        const page = document.querySelector<HTMLElement>('#page-billing');
        const grid = page?.querySelector<HTMLElement>('.stats-grid-3');
        const cards = [...(grid?.querySelectorAll<HTMLElement>(':scope > .stat-card') ?? [])];
        const rects = cards.map((card) => card.getBoundingClientRect());
        const records = page?.querySelector<HTMLElement>('[aria-labelledby="billing-transactions"]');
        const empty = records?.querySelector<HTMLElement>('.empty-state');
        const security = page?.querySelector<HTMLElement>('[aria-labelledby="billing-security"]');
        return {
          rootScrollWidth: document.documentElement.scrollWidth,
          gridColumns: grid ? getComputedStyle(grid).gridTemplateColumns : '',
          statsInsets: grid ? {
            left: Math.round(grid.getBoundingClientRect().left),
            right: Math.round(document.documentElement.clientWidth - grid.getBoundingClientRect().right)
          } : null,
          firstStatFullWidth: rects.length === 3 && Math.abs(rects[0].width - grid!.getBoundingClientRect().width) <= 1,
          secondaryStatsShareRow: rects.length === 3 && Math.abs(rects[1].top - rects[2].top) <= 1,
          statsEnd: grid ? Math.round(grid.getBoundingClientRect().bottom) : null,
          emptyHeight: empty ? Math.round(empty.getBoundingClientRect().height) : null,
          securityStartsAfterRecords: Boolean(security && records && security.getBoundingClientRect().top >= records.getBoundingClientRect().bottom)
        };
      });

      expect(layout.rootScrollWidth, `billing should not overflow at ${width}px`).toBeLessThanOrEqual(width);
      expect(layout.gridColumns.split(' ').length, `billing summary should have two columns at ${width}px`).toBe(2);
      expect(layout.statsInsets, `billing summary page insets at ${width}px`).toEqual({ left: 16, right: 16 });
      expect(layout.firstStatFullWidth, `balance stat should lead at ${width}px`).toBe(true);
      expect(layout.secondaryStatsShareRow, `download count and spend should share a row at ${width}px`).toBe(true);
      expect(layout.emptyHeight, `billing empty state height at ${width}px`).toBeLessThanOrEqual(190);
      expect(layout.securityStartsAfterRecords).toBe(true);
    }
  });

  test('balance overview cards keep equal mobile side insets', async ({ page }) => {
    await loginAs(page, 'alice');

    for (const width of [360, 390, 430, 464]) {
      await page.setViewportSize({ width, height: 1040 });
      await page.goto('/me/balance', { waitUntil: 'domcontentloaded' });
      const cards = page.locator('.credits-overview-grid > [data-slot="card"]');
      await expect(cards.first()).toBeVisible();
      const insets = await page.evaluate(() => [...document.querySelectorAll<HTMLElement>('.credits-overview-grid > [data-slot="card"]')].map((card) => {
        const style = getComputedStyle(card);
        const rect = card.getBoundingClientRect();
        const header = card.querySelector<HTMLElement>('[data-slot="card-header"]')?.getBoundingClientRect();
        const content = card.querySelector<HTMLElement>('[data-slot="card-content"]')?.getBoundingClientRect();
        const footer = card.querySelector<HTMLElement>('[data-slot="card-footer"]')?.getBoundingClientRect();
        return {
          leftPadding: Number.parseFloat(style.paddingLeft),
          rightPadding: Number.parseFloat(style.paddingRight),
          contentLeft: header ? Math.round(header.left - rect.left) : 0,
          contentRight: header ? Math.round(rect.right - header.right) : 0,
          contentEdgesAligned: Boolean(header && content && footer
            && Math.abs(header.left - content.left) <= 1
            && Math.abs(header.right - content.right) <= 1
            && Math.abs(content.left - footer.left) <= 1
            && Math.abs(content.right - footer.right) <= 1)
        };
      }));

      expect(insets, `balance overview cards at ${width}px`).toHaveLength(3);
      for (const inset of insets) {
        expect(inset.leftPadding, `balance card left padding at ${width}px`).toBeGreaterThanOrEqual(12);
        expect(inset.rightPadding, `balance card right padding at ${width}px`).toBeGreaterThanOrEqual(12);
        expect(Math.abs(inset.contentLeft - inset.contentRight), `balance card inset symmetry at ${width}px`).toBeLessThanOrEqual(1);
        expect(inset.contentEdgesAligned, `balance card content/footer alignment at ${width}px`).toBe(true);
      }
    }
  });

  test('member pages keep the viewport and main canvas aligned at phone widths', async ({ page }) => {
    await loginAs(page, 'alice');

    for (const route of MEMBER_ROUTES) {
      await page.setViewportSize({ width: 360, height: 844 });
      await page.goto(route, { waitUntil: 'domcontentloaded' });
      await expect(page.locator('#main-content')).toBeVisible();

      // Resize the same rendered route instead of reloading all 18 pages at each
      // width. This still exercises responsive CSS while avoiding repeated
      // authenticated API requests that can trigger the real anti-abuse challenge.
      for (const width of [360, 390, 430]) {
        await page.setViewportSize({ width, height: 844 });
        const layout = await page.evaluate(() => {
          const main = document.querySelector<HTMLElement>('#main-content');
          const pageCanvas = main?.querySelector<HTMLElement>('[id^="page-"]');
          const rect = pageCanvas?.getBoundingClientRect();
          return {
            viewportWidth: document.documentElement.clientWidth,
            bodyWidth: document.body.clientWidth,
            rootScrollWidth: document.documentElement.scrollWidth,
            canvasLeft: rect ? Math.round(rect.left) : null,
            canvasRight: rect ? Math.round(rect.right) : null,
            hasPageCanvas: Boolean(rect),
            hasLoginRedirect: Boolean(main?.querySelector('#page-login'))
          };
        });

        expect(layout.hasLoginRedirect, `${route} should render for the signed-in member`).toBe(false);
        expect(layout.bodyWidth, `${route} body width at ${width}px`).toBe(width);
        expect(layout.rootScrollWidth, `${route} root should not overflow at ${width}px`).toBeLessThanOrEqual(width);
        if (layout.hasPageCanvas) {
          expect(layout.canvasLeft, `${route} canvas left edge at ${width}px`).toBe(0);
          expect(layout.canvasRight, `${route} canvas right edge at ${width}px`).toBe(width);
        }

        if (width === 390 && route === '/me/balance') {
          await expect(page.locator('.credits-overview-grid > [data-slot="card"]').first()).toBeVisible();
          await expect(page.locator('.credits-primary-section')).toBeVisible();
          const gaps = await page.evaluate(() => {
            const chart = document.querySelector('.credits-chart-column');
            const metrics = document.querySelector('.credits-metrics-column');
            const primary = document.querySelector('.credits-primary-section');
            const overview = document.querySelector('.credits-overview-section');
            if (!chart || !metrics || !primary || !overview) return null;
            return {
              chartToMetrics: Math.round(metrics.getBoundingClientRect().top - chart.getBoundingClientRect().bottom),
              primaryToOverview: Math.round(overview.getBoundingClientRect().top - primary.getBoundingClientRect().bottom)
            };
          });
          expect(gaps).not.toBeNull();
          expect(gaps!.chartToMetrics).toBeLessThanOrEqual(28);
          expect(gaps!.primaryToOverview).toBeLessThanOrEqual(36);
        }
      }
    }
  });
});
