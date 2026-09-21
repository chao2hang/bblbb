<!-- M07-SHOP-STUDIO：帖子特效 mock 预览卡。
  按 CosmeticDefStyle（icon/color/animate）渲染帖子卡的色调与角标；
  色值经 #rrggbb 白名单校验后才进入内联样式，支持自定义 CSS 与 JS 脚本。 -->
<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import type { CosmeticDefStyle } from '$lib/api/types';
  import { ICON_OPTIONS } from './style-draft';

  let {
    style = {},
    name = '帖子特效'
  }: {
    style?: CosmeticDefStyle;
    name?: string;
  } = $props();

  let postEl: HTMLElement | null = $state(null);

  const SAFE_ICONS = new Set(ICON_OPTIONS.map((o) => o.value));
  const iconName = $derived(typeof style.icon === 'string' && SAFE_ICONS.has(style.icon) ? style.icon : 'sparkles');
  const color = $derived(typeof style.color === 'string' && /^#[0-9a-fA-F]{6}$/.test(style.color) ? style.color : '#f472b6');
  const duration = $derived(
    typeof style.durationMs === 'number' && Number.isFinite(style.durationMs)
      ? Math.min(Math.max(style.durationMs, 800), 30000)
      : 2800
  );

  const animClass = $derived.by(() => {
    if (style.animate === 'pulse') return 'post-mock__badge--pulse';
    if (style.animate === 'float') return 'post-mock__badge--float';
    if (style.animate === 'bounce') return 'post-mock__badge--bounce';
    if (style.animate === 'sparkle') return 'post-mock__badge--sparkle';
    if (style.animate === 'glitch') return 'post-mock__badge--glitch';
    if (style.animate === 'rainbow') return 'post-mock__badge--rainbow';
    return '';
  });

  onMount(() => {
    if (style.js && postEl) {
      try {
        const fn = new Function('el', 'style', style.js);
        fn(postEl, style);
      } catch (err) {
        console.warn('Post effect custom JS execution error:', err);
      }
    }
  });
</script>

{#if style.css}
  <svelte:element this={'style'}>
    {style.css}
  </svelte:element>
{/if}

<article bind:this={postEl} class="post-mock" style="border-color:{color};" aria-label="帖子特效预览">
  <header class="post-mock__head">
    <span
      class="post-mock__badge {animClass}"
      style="color:{color};border-color:{color};animation-duration:{duration}ms;"
    >
      <Icon name={iconName} size={12} />
      {name}
    </span>
  </header>
  <p class="post-mock__text">这是一条示例帖子的内容摘要，用于预览帖子特效在信息流中的实际效果。</p>
</article>

<style>
  .post-mock {
    border: 1px solid;
    border-radius: 10px;
    padding: var(--space-3, 12px);
    background: var(--color-bg-card, #fff);
  }
  .post-mock__head {
    display: flex;
    align-items: center;
    gap: var(--space-2, 8px);
  }
  .post-mock__badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border: 1px solid;
    border-radius: 999px;
    font-size: var(--text-xs, 12px);
    font-weight: 600;
  }
  .post-mock__badge--pulse {
    animation: studio-post-pulse ease-in-out infinite;
  }
  .post-mock__badge--float {
    animation: studio-post-float ease-in-out infinite;
  }
  .post-mock__badge--bounce {
    animation: studio-post-bounce ease-in-out infinite;
  }
  .post-mock__badge--sparkle {
    animation: studio-post-sparkle ease-in-out infinite;
  }
  .post-mock__badge--glitch {
    animation: studio-post-glitch steps(2, start) infinite;
  }
  .post-mock__badge--rainbow {
    animation: studio-post-rainbow linear infinite;
  }
  .post-mock__text {
    margin: var(--space-2, 8px) 0 0;
    color: var(--color-text-secondary, #666);
    font-size: var(--text-sm, 13px);
    line-height: 1.6;
  }
  @keyframes studio-post-pulse {
    0%, 100% { opacity: 0.75; transform: scale(1); }
    50% { opacity: 1; transform: scale(1.08); }
  }
  @keyframes studio-post-float {
    0%, 100% { transform: translateY(0); }
    50% { transform: translateY(-3px); }
  }
  @keyframes studio-post-bounce {
    0%, 100% { transform: translateY(0); }
    30% { transform: translateY(-4px); }
    60% { transform: translateY(1px); }
  }
  @keyframes studio-post-sparkle {
    0%, 100% { opacity: 0.9; filter: drop-shadow(0 0 2px currentColor); }
    50% { opacity: 1; filter: drop-shadow(0 0 8px currentColor); }
  }
  @keyframes studio-post-glitch {
    0%, 100% { transform: translate(0); }
    25% { transform: translate(-1.5px, 1px); }
    75% { transform: translate(1.5px, -1px); }
  }
  @keyframes studio-post-rainbow {
    0% { filter: hue-rotate(0deg); }
    100% { filter: hue-rotate(360deg); }
  }
  @media (prefers-reduced-motion: reduce) {
    .post-mock__badge--pulse,
    .post-mock__badge--float,
    .post-mock__badge--bounce,
    .post-mock__badge--sparkle,
    .post-mock__badge--glitch,
    .post-mock__badge--rainbow { animation: none; }
  }
</style>
