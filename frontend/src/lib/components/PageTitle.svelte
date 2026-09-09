<script lang="ts">
  // 全站文案（0065）：页面标题统一组件 ——「{页面名} — {站点名}」。
  //
  // 站点名取根 layout 注入的 data.site（后台「系统设置 → 站点名称」）；
  // 独立渲染（vitest ssr）/后端不可达时解析内置兜底（与 Seo.svelte 的
  // fallbackCanonical 同容错策略）。各页面不再手写 `— BBLBB` 后缀。
  import { page } from '$app/state';
  import { FALLBACK_SITE_NAME, pageTitle, resolveSiteCopy } from '$lib/site/copy';

  let { title }: { title: string } = $props();

  const siteName = $derived.by(() => {
    try {
      return resolveSiteCopy(page.data?.site ?? null).siteName;
    } catch {
      return FALLBACK_SITE_NAME;
    }
  });
</script>

<svelte:head>
  <title>{pageTitle(title, siteName)}</title>
</svelte:head>
