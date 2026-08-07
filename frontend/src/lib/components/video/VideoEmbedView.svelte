<script lang="ts">
  // M10-UI-03/04/05：安全视频投影组件（阅读/预览通用）。
  //
  // Security 边界（VIDEO-PLUGIN.md §3/§5，M10-UI-03）：
  //  - 只渲染后端投影字段（media_url/official_url/source_url/poster_url/
  //    caption_url/title），绝不从用户输入拼接 URL，绝不构造原始 iframe
  //    HTML（本文件无 {@html}，渲染用安全元素 + 属性绑定）；
  //  - xigua iframe 带 sandbox/allow 最小权限、referrerpolicy=no-referrer、
  //    loading=lazy；默认不自动播放、不启用摄像头/麦克风；
  //  - blocked/removed 不渲染任何视频 URL 或播放器配置（M10-UI-04）；
  //  - 无 JS（SSR）恒只渲染安全外链卡片；JS 挂载后才按需内联播放器
  //    （M10-UI-05）；CSP 指令由后端按启用 Provider 逐页生成，
  //    本组件只渲染落在 safeHttpsUrl/canRenderInlinePlayer 判定内的 URL。
  import { onMount } from 'svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { canRenderInlinePlayer, safeExternalHref } from '$lib/video/csp';
  import { safeHttpsUrl } from '$lib/video/projection';
  import {
    formatVideoDuration,
    videoProviderLabel,
    videoStatusLabel,
    videoStatusTone
  } from '$lib/video/labels';
  import type { VideoEmbedView } from '$lib/api/types';

  let { view }: { view: VideoEmbedView } = $props();

  let mounted = $state(false);
  onMount(() => {
    mounted = true;
  });

  const playable = $derived(canRenderInlinePlayer(view));
  const externalHref = $derived(safeExternalHref(view));
  const status = $derived(view.status);
  const provider = $derived(view.provider);
  const title = $derived(view.title?.trim() || null);
  // 防御：渲染一律使用经 safeHttpsUrl 复核的 URL（即使调用方误传未投影对象，
  // 非 https / userinfo / javascript: 等也绝不进入 src/href）。
  const mediaUrl = $derived(safeHttpsUrl(view.media_url));
  const officialUrl = $derived(safeHttpsUrl(view.official_url));
  const posterUrl = $derived(safeHttpsUrl(view.poster_url));
  const captionUrl = $derived(safeHttpsUrl(view.caption_url));
  const duration = $derived(view.duration_seconds ?? null);
  const mediaType = $derived(view.media_type ?? null);
  const iframeTitle = $derived(title ?? '嵌入视频（来自来源网站）');
  const videoLabel = $derived(title ?? '视频（来自来源网站）');
  const statusText = $derived(
    status === 'blocked'
      ? '内容已按来源要求下架，不再提供播放'
      : status === 'removed'
        ? '视频引用已移除，不再提供播放'
        : status === 'error'
          ? '视频解析失败，可尝试从来源网站打开'
          : status === 'pending'
            ? '视频解析中…'
            : null
  );
</script>

<figure class="video-embed" data-video-status={status} data-video-provider={provider}>
  <div class="video-embed-frame" style="aspect-ratio:16/9;max-width:100%;">
    {#if mounted && playable}
      {#if provider === 'xigua' && officialUrl}
        <iframe
          class="video-embed-iframe"
          style="width:100%;height:100%;border:0;"
          src={officialUrl}
          title={iframeTitle}
          sandbox="allow-scripts allow-same-origin allow-presentation"
          allow="encrypted-media; picture-in-picture"
          referrerpolicy="no-referrer"
          loading="lazy"
          allowfullscreen
        ></iframe>
      {:else if mediaUrl}
        <video
          class="video-embed-player"
          style="width:100%;height:100%;"
          controls
          playsinline
          preload="metadata"
          crossorigin="anonymous"
          controlsList="nodownload noremoteplayback"
          poster={posterUrl ?? undefined}
          aria-label={videoLabel}
        >
          {#if captionUrl}
            <track kind="captions" src={captionUrl} srclang="zh" label="中文字幕" />
          {/if}
          <source src={mediaUrl} type={mediaType ?? undefined} />
          你的浏览器不支持 HTML5 视频，请使用外链打开。
        </video>
      {/if}
    {:else if externalHref}
      <a
        class="video-embed-external"
        style="display:flex;align-items:center;justify-content:center;width:100%;height:100%;text-decoration:none;color:inherit;"
        href={externalHref}
        rel="noopener noreferrer nofollow"
        target="_blank"
      >
        {#if posterUrl}
          <img
            src={posterUrl}
            alt={title ?? ''}
            class="video-embed-poster"
            style="width:100%;height:100%;object-fit:cover;"
          />
        {:else}
          <span
            class="video-embed-placeholder"
            style="display:flex;flex-direction:column;align-items:center;gap:var(--space-1);"
          >
            <Icon name="video" size={28} />
            <span class="text-secondary" style="font-size:var(--text-sm);">在来源网站打开视频</span>
          </span>
        {/if}
      </a>
    {:else if posterUrl}
      <img
        src={posterUrl}
        alt={title ?? ''}
        class="video-embed-poster"
        style="width:100%;height:100%;object-fit:cover;"
      />
    {:else}
      <div class="video-embed-placeholder" style="display:flex;align-items:center;justify-content:center;width:100%;height:100%;">
        <Icon name="video" size={28} />
      </div>
    {/if}
  </div>

  <figcaption class="video-embed-meta" style="display:flex;flex-wrap:wrap;align-items:center;gap:var(--space-2);margin-top:var(--space-1);">
    <span class="badge {videoStatusTone(status)}">{videoStatusLabel(status)}</span>
    {#if provider}
      <span class="badge badge-neutral">{videoProviderLabel(provider)}</span>
    {/if}
    {#if title}
      <strong>{title}</strong>
    {/if}
    {#if duration !== null && status === 'ready'}
      <span class="text-secondary" style="font-size:var(--text-xs);">{formatVideoDuration(duration)}</span>
    {/if}
    {#if statusText}
      <span class="text-secondary" style="font-size:var(--text-xs);">{statusText}</span>
    {/if}
    {#if externalHref}
      <a class="text-link" style="font-size:var(--text-xs);" href={externalHref} rel="noopener noreferrer nofollow" target="_blank">来源 / 外链</a>
    {/if}
  </figcaption>
</figure>
