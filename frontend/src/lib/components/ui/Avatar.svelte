<script lang="ts">
  import { browser } from '$app/environment';
  import { attachmentContentUrl } from '$lib/api/client';
  import {
    AVATAR_PALETTES,
    getAvatarInitial,
    getAvatarColorIndex
  } from '$lib/utils/avatar';

  const SIZES: Record<string, number> = {
    xs: 20,
    sm: 24,
    md: 32,
    lg: 40,
    xl: 64,
    '2xl': 72,
    xxl: 72
  };

  let {
    name = '?',
    size = 'md',
    title,
    src = null,
    attachmentId = null,
    seed = null,
    username = null,
    userId = null,
    loading = false,
    radius = null,
    class: klass = ''
  }: {
    name?: string;
    size?: string | number;
    title?: string;
    src?: string | null;
    attachmentId?: string | null;
    seed?: string | null;
    username?: string | null;
    userId?: string | null;
    loading?: boolean;
    radius?: string | null;
    class?: string;
  } = $props();

  let imgLoaded = $state(false);
  let imgFailed = $state(false);

  const initial = $derived(getAvatarInitial(name));
  const effectiveKey = $derived(
    String(seed ?? username ?? userId ?? name ?? '?').trim()
  );
  const paletteIndex = $derived(getAvatarColorIndex(effectiveKey, AVATAR_PALETTES.length));
  const c1 = $derived(AVATAR_PALETTES[paletteIndex][0]);
  const c2 = $derived(AVATAR_PALETTES[paletteIndex][1]);
  const gradientBg = $derived(`linear-gradient(135deg, ${c1}, ${c2})`);

  const px = $derived(
    typeof size === 'number'
      ? size
      : SIZES[size] || parseInt(String(size), 10) || 32
  );

  const fontSize = $derived(
    px <= 20
      ? 10
      : px <= 24
        ? 12
        : px <= 32
          ? 14
          : px <= 40
            ? 18
            : Math.round(px * 0.4)
  );

  const imageSrc = $derived(
    src || (attachmentId ? attachmentContentUrl(attachmentId) : null)
  );

  const hasValidImage = $derived(Boolean(imageSrc && !imgFailed));
  const isLoading = $derived(loading || Boolean(imageSrc && !imgLoaded && !imgFailed));

  $effect(() => {
    void imageSrc;
    imgLoaded = false;
    imgFailed = false;
  });

  function checkImgLoaded(node: HTMLImageElement) {
    if (node.complete && node.naturalWidth > 0) {
      imgLoaded = true;
    }
  }
</script>

<span
  class="avatar avatar-{size} {klass}"
  class:has-img={hasValidImage}
  class:is-loading={isLoading}
  class:is-loaded={hasValidImage && imgLoaded}
  title={title ?? name}
  role="img"
  aria-label={name}
  aria-busy={isLoading ? 'true' : undefined}
  style="position:relative;width:{px}px;height:{px}px;font-size:{fontSize}px;border-radius:{radius ? radius : 'var(--avatar-radius,50%)'} !important;background:{isLoading || hasValidImage ? 'var(--color-bg-subtle, #f1f5f9)' : gradientBg} !important;color:{isLoading || hasValidImage ? 'transparent' : '#ffffff'} !important;--avatar-bg:{gradientBg};--avatar-color:#ffffff;overflow:hidden;display:inline-grid;place-items:center;vertical-align:middle;font-weight:600;line-height:1;user-select:none;"
>
  {#if hasValidImage}
    <img
      use:checkImgLoaded
      src={imageSrc}
      alt=""
      aria-hidden="true"
      loading="lazy"
      class="avatar-img"
      class:is-loading={!imgLoaded}
      class:is-loaded={imgLoaded}
      style="width:100%;height:100%;object-fit:cover;display:block;opacity:{browser && !imgLoaded ? 0 : 1};transition:opacity 0.2s ease-in-out;"
      onload={() => {
        imgLoaded = true;
      }}
      onerror={() => {
        imgFailed = true;
        imgLoaded = false;
      }}
    />
  {/if}

  {#if isLoading}
    <span class="avatar-placeholder" aria-hidden="true">
      <svg
        class="avatar-placeholder-icon"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.6"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <circle cx="12" cy="8" r="4" />
        <path d="M6 21v-2a4 4 0 0 1 4-4h4a4 4 0 0 1 4 4v2" />
      </svg>
    </span>
  {:else if !hasValidImage}
    {initial}
  {/if}
</span>

<style>
  .avatar-placeholder {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(
      90deg,
      var(--color-bg-subtle, #f1f5f9) 25%,
      var(--color-bg-inset, #e2e8f0) 50%,
      var(--color-bg-subtle, #f1f5f9) 75%
    );
    background-size: 200% 100%;
    animation: avatar-shimmer 1.5s ease-in-out infinite;
    pointer-events: none;
    z-index: 1;
  }

  @keyframes avatar-shimmer {
    0% {
      background-position: 200% 0;
    }
    100% {
      background-position: -200% 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .avatar-placeholder {
      animation: none;
      background: var(--color-bg-subtle, #f1f5f9);
    }
  }

  .avatar-placeholder-icon {
    width: 56%;
    height: 56%;
    color: var(--color-text-secondary, #94a3b8);
    opacity: 0.42;
    flex-shrink: 0;
  }
</style>
