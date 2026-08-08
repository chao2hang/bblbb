<script lang="ts">
  // M14-SEO-01：页面级 SEO 头注入组件（title/description/canonical/OG/
  // Twitter/robots/JSON-LD）。
  //
  // - canonical 缺省时由 page.url 派生（origin + pathname，去掉查询参数，
  //   保证搜索/筛选页不被重复 canonical 收录）；
  // - noindex 由调用方按后端投影决定（隐藏/未发布/审核中/删除/封禁），
  //   组件不做业务判断；
  // - JSON-LD 经 JsonLd.svelte 白名单注入（escapeJsonLdScript 转义）。
  import { page as kitPage } from '$app/state';
  import JsonLd from './JsonLd.svelte';
  import { buildSeo, type SeoInput } from '$lib/seo/meta';

  let {
    title,
    description = '',
    canonical = '',
    noindex = false,
    og,
    twitter,
    jsonLd = null
  }: SeoInput = $props();

  // 独立 SSR 渲染（vitest ssr 项目直接 render 组件）没有 SvelteKit page 上下文；
  // page.url 访问会抛错 → 回退为空 canonical（buildSeo 视为无 canonical）。
  const fallbackCanonical = $derived.by(() => {
    try {
      return kitPage.url.origin + kitPage.url.pathname;
    } catch {
      return '';
    }
  });

  const meta = $derived(
    buildSeo({
      title,
      description,
      canonical: canonical || fallbackCanonical,
      noindex,
      og,
      twitter,
      jsonLd
    })
  );
</script>

<svelte:head>
  <title>{meta.title}</title>
  {#if meta.description}<meta name="description" content={meta.description} />{/if}
  {#if meta.canonical}<link rel="canonical" href={meta.canonical} />{/if}
  {#if meta.robots}<meta name="robots" content={meta.robots} />{/if}
  <meta property="og:type" content={meta.ogType} />
  <meta property="og:title" content={meta.ogTitle} />
  {#if meta.ogDescription}<meta property="og:description" content={meta.ogDescription} />{/if}
  {#if meta.ogUrl}<meta property="og:url" content={meta.ogUrl} />{/if}
  {#if meta.ogImage}<meta property="og:image" content={meta.ogImage} />{/if}
  {#if meta.ogSiteName}<meta property="og:site_name" content={meta.ogSiteName} />{/if}
  <meta name="twitter:card" content={meta.twitterCard} />
  {#if meta.twitterImage}<meta name="twitter:image" content={meta.twitterImage} />{/if}
</svelte:head>

{#if meta.jsonLd}<JsonLd data={meta.jsonLd} />{/if}
