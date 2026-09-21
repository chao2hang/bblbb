<!-- M07-SHOP-STUDIO：个人主页装饰的多场景 mock 预览。
  按 CosmeticDefStyle 结构化参数（baseColor/accentColor/texture/animate）
  渲染封面纹理；色值经 #rrggbb 白名单校验后才进入内联样式。 -->
<script lang="ts">
  import { onMount } from 'svelte';
  import CosmeticAvatar from '$lib/components/wardrobe/CosmeticAvatar.svelte';
  import CosmeticName from '$lib/components/wardrobe/CosmeticName.svelte';
  import type { CosmeticDefStyle, PublicPresentationTokens } from '$lib/api/types';

  let {
    style = {},
    presentation = null,
    name = '用户昵称'
  }: {
    style?: CosmeticDefStyle;
    /** 昵称/头像的组合试穿 Token（可携带昵称色、头像框等其他槽位）。 */
    presentation?: PublicPresentationTokens | null;
    name?: string;
  } = $props();

  let coverEl: HTMLDivElement | null = $state(null);

  function safeColor(v: unknown, fallback: string): string {
    return typeof v === 'string' && /^#[0-9a-fA-F]{6}$/.test(v) ? v : fallback;
  }

  const base = $derived(safeColor(style.baseColor, '#101827'));
  const accent = $derived(safeColor(style.accentColor, '#8b5cf6'));
  const texture = $derived(style.texture || 'sparkle');
  const duration = $derived(
    typeof style.durationMs === 'number' && Number.isFinite(style.durationMs)
      ? Math.min(Math.max(style.durationMs, 800), 30000)
      : 5000
  );
  const opacity = $derived(
    typeof style.opacity === 'number' && Number.isFinite(style.opacity) ? style.opacity / 100 : 1
  );

  const bgImageUrl = $derived(
    typeof (style as Record<string, unknown>).url === 'string'
      ? (style as Record<string, unknown>).url as string
      : typeof (style as Record<string, unknown>).image === 'string'
        ? `/api/v1/steam-assets/backgrounds/${(style as Record<string, unknown>).image}`
        : null
  );
  const bgWebmUrl = $derived(
    typeof (style as Record<string, unknown>).webm === 'string'
      ? (style as Record<string, unknown>).webm as string
      : null
  );

  /** 纹理 → 图案背景 */
  const background = $derived.by(() => {
    if (bgImageUrl) {
      return `url('${bgImageUrl}') center / cover no-repeat, ${base}`;
    }
    const a = accent;
    if (texture === 'grid') {
      return `linear-gradient(${a}22 1px, transparent 1px),linear-gradient(90deg, ${a}22 1px, transparent 1px),${base}`;
    }
    if (texture === 'dots') {
      return `radial-gradient(circle, ${a}55 1.5px, transparent 1.6px),${base}`;
    }
    if (texture === 'dark_stars') {
      return `radial-gradient(circle at 30% 40%, ${a}40, transparent 45%),radial-gradient(circle at 70% 60%, ${a}26, transparent 45%),${base}`;
    }
    if (texture === 'aurora') {
      return `radial-gradient(ellipse at 20% 0%, ${a}80, transparent 60%), radial-gradient(ellipse at 80% 100%, ${a}55, transparent 60%), ${base}`;
    }
    if (texture === 'matrix') {
      return `linear-gradient(rgba(0,255,128,0.15) 1px, transparent 1px), linear-gradient(90deg, ${a}15 1px, transparent 1px), ${base}`;
    }
    if (texture === 'waves') {
      return `radial-gradient(ellipse at 50% 120%, ${a}66 30%, transparent 70%), ${base}`;
    }
    if (texture === 'snow') {
      return `radial-gradient(circle, ${a}88 1.2px, transparent 1.5px), radial-gradient(circle, ${a}55 2px, transparent 2.5px), ${base}`;
    }
    if (texture === 'bubbles') {
      return `radial-gradient(circle at 35% 45%, ${a}33 12px, transparent 13px), radial-gradient(circle at 75% 65%, ${a}22 20px, transparent 21px), ${base}`;
    }
    return `radial-gradient(circle at 20% 30%, ${a}52, transparent 42%),radial-gradient(circle at 80% 70%, ${a}30, transparent 42%),${base}`;
  });

  const backgroundSize = $derived.by(() => {
    if (texture === 'grid' || texture === 'matrix') return '28px 28px,28px 28px,auto';
    if (texture === 'dots') return '22px 22px,auto';
    if (texture === 'snow') return '40px 40px,70px 70px,auto';
    if (texture === 'bubbles') return '90px 90px,120px 120px,auto';
    return 'auto';
  });

  const animClass = $derived.by(() => {
    if (style.animate === 'shimmer') return 'profile-mock__cover--shimmer';
    if (style.animate === 'aurora') return 'profile-mock__cover--aurora';
    if (style.animate === 'flow') return 'profile-mock__cover--flow';
    if (style.animate === 'pulse') return 'profile-mock__cover--pulse';
    if (style.animate === 'scanline') return 'profile-mock__cover--scanline';
    return '';
  });

  onMount(() => {
    if (style.js && coverEl) {
      try {
        const fn = new Function('el', 'style', style.js);
        fn(coverEl, style);
      } catch (err) {
        console.warn('Profile effect custom JS execution error:', err);
      }
    }
  });
</script>

{#if style.css}
  <svelte:element this={'style'}>
    {style.css}
  </svelte:element>
{/if}

<div class="profile-mock" aria-label="个人主页预览">
  <div
    bind:this={coverEl}
    class="profile-mock__cover {animClass}"
    style="background:{background};background-size:{bgImageUrl ? 'cover' : backgroundSize};animation-duration:{duration}ms;opacity:{opacity};position:relative;overflow:hidden;"
  >
    {#if bgWebmUrl}
      <video
        src={bgWebmUrl}
        autoplay
        loop
        muted
        playsinline
        style="position:absolute;inset:0;width:100%;height:100%;object-fit:cover;pointer-events:none;"
      ></video>
    {/if}
  </div>
  <div class="profile-mock__body">
    <span class="profile-mock__avatar">
      <CosmeticAvatar name="用户" seed="studio-preview" size={56} presentation={presentation ?? {}} />
    </span>
    <div class="profile-mock__name">
      <CosmeticName {name} presentation={presentation ?? {}} />
    </div>
    <p class="profile-mock__bio">这里是个人简介示例文本，用于预览主页装饰的整体观感。</p>
  </div>
</div>

<style>
  .profile-mock {
    overflow: hidden;
    border: 1px solid var(--color-border, #ccc);
    border-radius: 10px;
    background: var(--color-bg-card, #fff);
  }
  .profile-mock__cover {
    height: 72px;
    transition: opacity 0.2s ease;
  }
  .profile-mock__cover--shimmer {
    animation: studio-profile-shimmer ease-in-out infinite;
  }
  .profile-mock__cover--aurora {
    animation: studio-profile-aurora ease-in-out infinite;
  }
  .profile-mock__cover--flow {
    animation: studio-profile-flow linear infinite;
  }
  .profile-mock__cover--pulse {
    animation: studio-profile-pulse ease-in-out infinite;
  }
  .profile-mock__cover--scanline {
    animation: studio-profile-scanline linear infinite;
  }
  .profile-mock__body {
    padding: 0 var(--space-3, 12px) var(--space-3, 12px);
  }
  .profile-mock__avatar {
    display: inline-flex;
    margin-top: -28px;
    border-radius: 50%;
    background: var(--color-bg-card, #fff);
    padding: 2px;
  }
  .profile-mock__name {
    margin-top: var(--space-2, 8px);
    font-size: var(--text-lg, 17px);
    font-weight: 700;
  }
  .profile-mock__bio {
    margin: var(--space-1, 4px) 0 0;
    color: var(--color-text-secondary, #666);
    font-size: var(--text-sm, 13px);
  }
  @keyframes studio-profile-shimmer {
    0%, 100% { filter: brightness(1); }
    50% { filter: brightness(1.24); }
  }
  @keyframes studio-profile-aurora {
    0%, 100% { filter: hue-rotate(0deg) brightness(1); }
    50% { filter: hue-rotate(45deg) brightness(1.2); }
  }
  @keyframes studio-profile-flow {
    0% { background-position: 0% 50%; }
    50% { background-position: 100% 50%; }
    100% { background-position: 0% 50%; }
  }
  @keyframes studio-profile-pulse {
    0%, 100% { opacity: 0.85; transform: scale(1); }
    50% { opacity: 1; transform: scale(1.02); }
  }
  @keyframes studio-profile-scanline {
    0% { filter: contrast(1); }
    50% { filter: contrast(1.15) brightness(1.1); }
    100% { filter: contrast(1); }
  }
  @media (prefers-reduced-motion: reduce) {
    .profile-mock__cover--shimmer,
    .profile-mock__cover--aurora,
    .profile-mock__cover--flow,
    .profile-mock__cover--pulse,
    .profile-mock__cover--scanline { animation: none; }
  }
</style>
