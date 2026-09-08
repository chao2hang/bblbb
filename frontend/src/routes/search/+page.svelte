<script lang="ts">
  // M08-UI-01/02：公开搜索 SSR。
  //
  // - load 已在服务端取回搜索结果（SearchPageData），无 JS 可读；
  // - 表单为原生 GET → /search?q=（无 JS 可提交）；
  // - 结果只渲染后端安全投影字段（SearchResultList，纯文本插值）；
  // - 429/挑战 → ChallengeGate（键盘/读屏/移动/失败回退）；
  // - 分页用稳定 cursor 链接（/search?q=&after=）。
  import { page } from '$app/state';
  import { SEARCH_LIMIT_DEFAULT, searchUrl } from '$lib/search';
  import SearchResultList from '$lib/components/SearchResultList.svelte';
  import ChallengeGate from '$lib/components/ui/ChallengeGate.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  // M14-SEO-01：搜索页统一 SEO（canonical + noindex + JSON-LD）。
  import Seo from '$lib/components/Seo.svelte';
  import { resolveSiteCopy, type SiteCopyView } from '$lib/site/copy';
  import type { SearchPageData } from './+page.server';

  // data.site：根 layout 注入的全站文案（0065）；隔离渲染时兜底解析。
  let { data }: { data: SearchPageData & { site?: SiteCopyView | null } } = $props();

  const tagSlug = $derived(data.tag || page.url.searchParams.get('tag') || '');
  const searched = $derived(data.searched);
  const results = $derived(data.results);
  const invalid = $derived(data.invalid);
  const error = $derived(data.error);
  const rateLimited = $derived(data.rateLimited);
  const challenge = $derived(data.challenge);
  const q = $derived(data.q);

  // ── SEO head（M08-UI-05 语义经 M14-SEO-01 统一生成器）：canonical +
  // 默认 noindex（FRONTEND.md §8；不承诺替代服务端边界）+ JSON-LD。
  const origin = $derived(page.url.origin);
  const canonical = $derived(
    searched ? `${origin}${searchUrl(q, { limit: data.limit, tag: tagSlug })}` : `${origin}/search`
  );
  // 全站文案（0065）：站点名来自 layout data.site（后台系统设置）；标题
  // 后缀由 Seo 统一追加，此处只给页面名。
  const site = $derived(data.site ?? resolveSiteCopy(null));
  const pageTitle = $derived(
    searched
      ? q
        ? tagSlug
          ? `搜索「${q}」· 标签：${tagSlug}`
          : `搜索「${q}」`
        : `标签「${tagSlug}」相关内容`
      : '搜索'
  );
  const pageDesc = $derived(
    searched ? (q ? `搜索「${q}」的公开内容` : `查看标签「${tagSlug}」下的公开内容`) : `搜索${site.siteName}的公开内容`
  );
  const jsonLd = $derived.by(() => ({
    '@context': 'https://schema.org',
    '@type': 'WebSite',
    name: site.siteName,
    url: origin,
    potentialAction: {
      '@type': 'SearchAction',
      target: { '@type': 'EntryPoint', urlTemplate: `${origin}/search?q={search_term_string}` },
      'query-input': 'required name=search_term_string'
    }
  }));
  // 搜索页始终 noindex（M08-UI-05/FRONTEND.md §8），canonical 指向当前筛选。

  const totalLabel = $derived(
    results.length > 0 ? `找到 ${results.length} 条结果` : null
  );

  /** 下一分页链接（稳定 cursor）。 */
  const nextUrl = $derived(
    data.hasMore && data.nextCursor ? searchUrl(q, { limit: data.limit, after: data.nextCursor }) : null
  );
</script>

<Seo
  title={pageTitle}
  description={pageDesc}
  canonical={canonical}
  noindex
  og={{ type: 'website', siteName: site.siteName }}
  jsonLd={jsonLd}
/>

<div class="container app-page">
  <section class="page app-page app-route-search" id="page-search">
    <div class="app-route-head">
      <div class="app-route-head__copy">
        <span class="app-kicker">DISCOVER / SEARCH</span>
        <h1 tabindex="-1">搜索</h1>
        <p>搜索帖子、用户或标签</p>
      </div>
    </div>

    <form class="app-toolbar" role="search" method="get" action="/search" style="margin-bottom:14px;">
      {#if tagSlug}
        <input type="hidden" name="tag" value={tagSlug} />
      {/if}
      <label class="app-search" style="flex:1;">
        <Icon name="search" size={16} />
        <input
          type="search"
          name="q"
          placeholder="搜索帖子、用户或标签…"
          value={q || undefined}
          maxlength={200}
          aria-label="搜索帖子"
        />
      </label>
      <button type="submit" class="btn primary">搜索</button>
    </form>

    <div>
      {#if tagSlug}
        <div class="tag-chip" style="margin-bottom:var(--space-3);" title="按标签筛选">
          <Icon name="tag" size={12} />
          <span>标签：{tagSlug}</span>
          <a class="tag-chip-remove" href="/search" aria-label="移除标签筛选"><Icon name="x" size={12} /></a>
        </div>
      {/if}

      {#if !searched}
        <div class="app-card">
          <div class="app-empty">
            <Icon name="search" size={32} />
            <b>输入关键词搜索公开帖子</b>
            <span>输入关键词即可开始全站搜索帖子、用户或标签</span>
          </div>
        </div>
      {:else if invalid}
        <div class="app-card" role="alert">
          <div class="app-card__body">
            <p class="input-hint is-error" style="margin:0;">{invalid}</p>
            <a class="btn secondary sm" style="margin-top:var(--space-2);" href="/search">重新搜索</a>
          </div>
        </div>
      {:else if rateLimited}
        <ChallengeGate
          retryAfterSecs={rateLimited.retryAfterSecs}
          onRetry={() => window.location.reload()}
          title="搜索过于频繁"
          message="请求频率过高，系统已暂时限制搜索。请稍后再试。"
        />
      {:else if challenge}
        <ChallengeGate
          challengeUrl={challenge.challengeUrl}
          title="需要完成验证"
          message="检测到异常访问行为，请完成验证后继续。"
        />
      {:else if error}
        <div class="app-card" role="alert">
          <div class="app-card__body">
            <p class="input-hint is-error" style="margin:0;">{error}</p>
            <a class="btn secondary sm" style="margin-top:var(--space-2);" href={searchUrl(q, { limit: data.limit })}>重试搜索</a>
          </div>
        </div>
      {:else}
        {#if totalLabel}
          <p class="text-secondary" style="margin:0 0 var(--space-2);" role="status">{totalLabel}</p>
        {/if}
        <div class="app-card">
          <div class="app-card__body" style="padding:0;">
            <SearchResultList
              results={results}
              emptyTitle="没有找到相关内容"
              emptyDesc={q ? `没有与「${q}」匹配的公开内容，换个关键词试试` : '换个关键词试试'}
            />
          </div>
        </div>
        {#if nextUrl}
          <nav aria-label="搜索结果分页" style="display:flex;gap:var(--space-2);margin-top:var(--space-3);align-items:center;">
            <a class="btn secondary sm" href={nextUrl}>下一页</a>
            {#if data.after}
              <a class="btn ghost sm" href={searchUrl(q, { limit: data.limit })}>返回第一页</a>
            {/if}
          </nav>
        {/if}
      {/if}
    </div>
  </section>
</div>
