<script lang="ts">
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
    class?: string;
  } = $props();

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

  $effect(() => {
    if (imageSrc) {
      imgFailed = false;
    }
  });
</script>

<span
  class="avatar avatar-{size} {klass}"
  class:has-img={hasValidImage}
  title={title ?? name}
  role="img"
  aria-label={name}
  style="width:{px}px;height:{px}px;font-size:{fontSize}px;background:{hasValidImage ? 'var(--color-bg-subtle, #f1f5f9)' : gradientBg} !important;color:{hasValidImage ? 'transparent' : '#ffffff'} !important;--avatar-bg:{gradientBg};--avatar-color:#ffffff;overflow:hidden;display:inline-grid;place-items:center;vertical-align:middle;font-weight:600;line-height:1;user-select:none;"
>
  {#if hasValidImage}
    <img
      src={imageSrc}
      alt=""
      aria-hidden="true"
      loading="lazy"
      style="width:100%;height:100%;object-fit:cover;display:block;"
      onerror={() => (imgFailed = true)}
    />
  {:else}
    {initial}
  {/if}
</span>
