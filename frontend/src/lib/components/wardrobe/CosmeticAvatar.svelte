<script lang="ts">
  import { onMount } from 'svelte';
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import { attachmentContentUrl } from '$lib/api/client';
  import type { PublicPresentationTokens } from '$lib/api/types';
  import { AVATAR_FRAMES, AVATAR_FRAME_COLORS, avatarFrameRing, AVATAR_ATTACHMENTS, BUILTIN_APNG_FRAMES } from './tokens';

  let {
    name,
    size = 'md',
    presentation = null,
    avatarAttachmentId = null,
    seed = null,
    username = null,
    userId = null,
    title,
    loading = false,
    class: klass = '',
    style: customStyle = ''
  }: {
    name: string;
    size?: string | number;
    presentation?: PublicPresentationTokens | { presentation_tokens?: PublicPresentationTokens | null } | null;
    avatarAttachmentId?: string | null;
    seed?: string | null;
    username?: string | null;
    userId?: string | null;
    title?: string;
    loading?: boolean;
    class?: string;
    style?: string;
  } = $props();

  let avatarEl: HTMLSpanElement | null = $state(null);
  let frameFailed = $state(false);
  function resolveTokens(
    input: PublicPresentationTokens | { presentation_tokens?: PublicPresentationTokens | null } | null
  ): PublicPresentationTokens {
    if (input && 'presentation_tokens' in input) return input.presentation_tokens ?? {};
    return (input as PublicPresentationTokens) ?? {};
  }

  const tokens = $derived(resolveTokens(presentation));
  const frameToken = $derived(typeof tokens.avatar_frame === 'string' ? tokens.avatar_frame : null);
  const frameClass = $derived(frameToken ? AVATAR_FRAMES[frameToken] ?? null : null);
  // 彩色环（M07-SHOP-UI-09）：Token → 固定调色板 CSS 变量，白名单之外不渲染。
  const frameRing = $derived(frameToken ? avatarFrameRing(frameToken) : null);
  // 自定义样式库环（M07-SHOP-UI-10）：服务端投影的结构化参数，客户端再校验色值。
  const frameCustom = $derived.by(() => {
    const s = tokens.avatar_frame_style;
    if (!s || (s.mode !== 'ring' && s.mode !== 'steam_frame')) return null;
    const color = typeof s.color === 'string' && /^#[0-9a-fA-F]{6}$/.test(s.color) ? s.color : null;
    if (!color) return null;
    const clamp = (v: unknown, min: number, max: number, d: number): number =>
      typeof v === 'number' && Number.isFinite(v) ? Math.min(Math.max(Math.round(v), min), max) : d;
    const width = clamp(s.widthPx, 1, 12, 3);
    const glow = clamp(s.glowPx, 0, 32, 10);
    const glowSpread = clamp(s.glowSpread, 0, 20, 0);
    const borderStyle = s.borderStyle && ['solid', 'dashed', 'dotted', 'double', 'groove'].includes(s.borderStyle) ? s.borderStyle : 'solid';
    const accent = typeof s.accentColor === 'string' && /^#[0-9a-fA-F]{6}$/.test(s.accentColor) ? s.accentColor : null;
    const raw = color.slice(1);
    const r = parseInt(raw.slice(0, 2), 16);
    const g = parseInt(raw.slice(2, 4), 16);
    const b = parseInt(raw.slice(4, 6), 16);
    const radius = 'var(--avatar-radius, 50%)';
    const duration = clamp(s.durationMs, 800, 30000, 3000);
    const style = [
      `--avatar-frame-color:${color}`,
      `--avatar-frame-glow:rgba(${r},${g},${b},0.45)`,
      accent ? `--avatar-frame-accent:${accent}` : '',
      `border-width:${width}px`,
      `border-style:${borderStyle}`,
      accent ? `border-color:${color}` : '',
      `border-radius:${radius}`,
      `box-shadow:0 0 ${glow}px ${glowSpread}px rgba(${r},${g},${b},0.55)`,
      `animation-duration:${duration}ms`
    ].filter(Boolean).join(';');
    return {
      style,
      animate: s.animate || 'none',
      pulse: s.animate === 'pulse',
      spin: s.animate === 'spin',
      ripple: s.animate === 'ripple',
      float: s.animate === 'float',
      glitch: s.animate === 'glitch',
      rainbow: s.animate === 'rainbow',
      breathe: s.animate === 'breathe',
      title: tokens.avatar_frame_name ?? null,
      css: s.css,
      js: s.js
    };
  });
  const frameAssetId = $derived(
    typeof tokens.avatar_frame_attachment_id === 'string' ? tokens.avatar_frame_attachment_id : null
  );
  // 直接图片 URL 边框（来自 avatar_frame_url 或 avatar_frame_style.url 或 avatar_frame_style.image）
  const frameDirectUrl = $derived.by(() => {
    if (typeof tokens.avatar_frame_url === 'string' && tokens.avatar_frame_url) {
      return tokens.avatar_frame_url;
    }
    const s = tokens.avatar_frame_style;
    if (s) {
      const u = (s as Record<string, unknown>).url;
      if (typeof u === 'string' && u) return u;
      const img = (s as Record<string, unknown>).image;
      if (typeof img === 'string' && img) return `/api/v1/steam-assets/frames/${img}`;
    }
    return null;
  });
  // 内置 Steam APNG 动效头像框资源
  const builtinApng = $derived(
    frameToken && frameToken in BUILTIN_APNG_FRAMES ? BUILTIN_APNG_FRAMES[frameToken] : null
  );
  const avatarAttachment = $derived(
    typeof tokens.avatar_attachment === 'string' ? AVATAR_ATTACHMENTS[tokens.avatar_attachment] ?? null : null
  );

  // 头像框贴合缩放比（Steam 官方规范：边框为头像的 1.20 倍，四边各向外溢出 10%）
  const frameScale = $derived.by(() => {
    const s = tokens.avatar_frame_style;
    if (s && typeof s.frameScale === 'number' && s.frameScale >= 80 && s.frameScale <= 250) {
      return s.frameScale / 100;
    }
    // 图片头像框（APNG/Steam/上传动图）按 Steam 官方规范设为 1.20
    if (frameAssetId || builtinApng || frameDirectUrl) {
      return 1.20;
    }
    return 1;
  });

  // 头像形状契约（彻底杜绝直角或圆弧穿帮）：
  // 1. 如果样式显式定义了 shape:
  //    - 'circle' -> 50% 完美正圆（适用于圆形光环）
  //    - 'square' -> 0px 纯直角（适用于复古直角相框）
  //    - 'rounded' -> 4px 微圆角（Steam 官方正方形头像规范）
  // 2. 内置圆环/彩色环 -> 50% 正圆
  // 3. Steam 动效头像框 -> 默认 4px 微圆角正方形，与 Steam 官方渲染完全一致
  const avatarRadius = $derived.by(() => {
    const s = tokens.avatar_frame_style;
    if (s?.shape === 'square') return '0px';
    if (s?.shape === 'rounded') return '4px';
    if (s?.shape === 'circle') return '50%';
    if (frameClass || frameRing) return '50%';
    if (frameDirectUrl || builtinApng || frameAssetId) return '4px';
    return null;
  });

  const avatarShape = $derived.by(() => {
    const s = tokens.avatar_frame_style;
    if (s?.shape) return s.shape;
    if (frameClass || frameRing) return 'circle';
    if (frameAssetId || builtinApng || frameDirectUrl) return 'rounded';
    return null;
  });

  const hasFrame = $derived(
    Boolean(frameDirectUrl || (frameAssetId && !frameFailed) || builtinApng || frameClass || frameRing || frameCustom)
  );

  const inlineStyle = $derived(
    [
      frameScale !== 1 ? `--frame-scale:${frameScale}` : '',
      customStyle
    ].filter(Boolean).join(';') || undefined
  );

  onMount(() => {
    if (frameCustom?.js && avatarEl) {
      try {
        const fn = new Function('el', 'style', frameCustom.js);
        fn(avatarEl, tokens.avatar_frame_style);
      } catch (err) {
        console.warn('Avatar custom JS execution error:', err);
      }
    }
  });
</script>

{#if frameCustom?.css}
  <svelte:element this={'style'}>
    {frameCustom.css}
  </svelte:element>
{/if}

<span
  bind:this={avatarEl}
  class="cosmetic-avatar {klass}"
  class:has-frame={hasFrame}
  data-frame-shape={avatarShape}
  style={inlineStyle}
>
  <Avatar
    {name}
    {size}
    {title}
    attachmentId={avatarAttachmentId}
    seed={seed ?? username ?? userId}
    {loading}
    radius={avatarRadius}
  />
  {#if avatarAttachment}
    <span class="cosmetic-avatar__attachment" aria-hidden="true">{avatarAttachment}</span>
  {/if}
  {#if frameDirectUrl}
    <img class="cosmetic-avatar__asset cosmetic-avatar__frame" src={frameDirectUrl} alt="" aria-hidden="true" />
  {:else if frameAssetId && !frameFailed}
    <img class="cosmetic-avatar__asset cosmetic-avatar__frame" src={attachmentContentUrl(frameAssetId)} alt="" aria-hidden="true" onerror={() => (frameFailed = true)} />
  {:else if builtinApng}
    <img class="cosmetic-avatar__asset cosmetic-avatar__frame" src={builtinApng.src} alt="" aria-hidden="true" />
  {:else if frameClass}
    <span class="cosmetic-avatar__fallback-frame {frameClass}" aria-hidden="true"></span>
  {:else if frameRing}
    <span class="cosmetic-avatar__fallback-frame {frameRing.className}" style={frameRing.style} aria-hidden="true"></span>
  {:else if frameCustom}
    <span
      class="cosmetic-avatar__fallback-frame avatar-frame-custom"
      class:avatar-frame-custom-pulse={frameCustom.pulse}
      class:avatar-frame-custom-spin={frameCustom.spin}
      class:avatar-frame-custom-ripple={frameCustom.ripple}
      class:avatar-frame-custom-float={frameCustom.float}
      class:avatar-frame-custom-glitch={frameCustom.glitch}
      class:avatar-frame-custom-rainbow={frameCustom.rainbow}
      class:avatar-frame-custom-breathe={frameCustom.breathe}
      style={frameCustom.style}
      aria-hidden="true"
    ></span>
  {/if}
</span>

<style>
  .cosmetic-avatar { position: relative; display: inline-flex; flex: 0 0 auto; width: max-content; height: max-content; vertical-align: middle; }
  /* 头像与头像边框圆角联动：
     1. 若明确标记为圆形框，头像裁切为 50% 完美正圆；
     2. 若标记为 rounded，裁切为 4px 微圆角正方形（对齐 Steam 官方头像规范）；
     3. 若标记为 square，裁切为 0px 纯直角；
     4. 默认跟随主题契约（--avatar-radius） */
  .cosmetic-avatar.has-frame > :global(.avatar) {
    border-radius: var(--avatar-radius, 50%) !important;
    overflow: hidden !important;
  }
  .cosmetic-avatar.has-frame[data-frame-shape='circle'] > :global(.avatar) {
    border-radius: 50% !important;
    overflow: hidden !important;
  }
  .cosmetic-avatar.has-frame[data-frame-shape='rounded'] > :global(.avatar) {
    border-radius: 4px !important;
    overflow: hidden !important;
  }
  .cosmetic-avatar.has-frame[data-frame-shape='square'] > :global(.avatar) {
    border-radius: 0 !important;
    overflow: hidden !important;
  }

  .cosmetic-avatar__asset,
  .cosmetic-avatar__fallback-frame { position: absolute; pointer-events: none; }
  .cosmetic-avatar__attachment { position: absolute; bottom: -2px; right: -2px; font-size: 14px; line-height: 1; z-index: 3; pointer-events: none; }

  /* 动效头像框：以头像中心为原点，居中放大到 --frame-scale（默认 122%），
     确保透明镂空内圈紧密包裹贴合头像本体，而外部装饰与挂件自然外溢展示 */
  .cosmetic-avatar__frame {
    position: absolute !important;
    top: 50% !important;
    left: 50% !important;
    width: calc(100% * var(--frame-scale, 1.22)) !important;
    height: calc(100% * var(--frame-scale, 1.22)) !important;
    max-width: none !important;
    max-height: none !important;
    min-width: 0 !important;
    min-height: 0 !important;
    transform: translate(-50%, -50%) !important;
    object-fit: contain !important;
    pointer-events: none !important;
    z-index: 2 !important;
  }
  .cosmetic-avatar__fallback-frame { inset: 0; width: 100%; height: 100%; object-fit: contain; border: 3px solid #d4a017; border-radius: var(--avatar-radius, 50%); box-shadow: 0 0 10px rgba(212, 160, 23, .48); box-sizing: border-box; }
  .cosmetic-avatar__fallback-frame.avatar-frame-blue { border-color: #0969da; box-shadow: 0 0 10px rgba(9, 105, 218, .42); }
  .cosmetic-avatar__fallback-frame.avatar-frame-glow { border-color: #8250df; box-shadow: 0 0 14px rgba(130, 80, 223, .75); animation: cosmetic-avatar-pulse 3s ease-in-out infinite; }
  .cosmetic-avatar__fallback-frame.avatar-frame-ring { border-color: var(--avatar-frame-color, #d4a017); box-shadow: 0 0 10px var(--avatar-frame-glow, rgba(212, 160, 23, .48)); }
  .cosmetic-avatar__fallback-frame.avatar-frame-custom { border-color: var(--avatar-frame-color, #d4a017); }
  .cosmetic-avatar__fallback-frame.avatar-frame-custom-pulse { animation: cosmetic-avatar-custom-pulse 3s ease-in-out infinite; }
  .cosmetic-avatar__fallback-frame.avatar-frame-custom-spin { animation: cosmetic-avatar-custom-spin 4s linear infinite; }
  .cosmetic-avatar__fallback-frame.avatar-frame-custom-ripple { animation: cosmetic-avatar-custom-ripple 2.5s cubic-bezier(0, 0.2, 0.8, 1) infinite; }
  .cosmetic-avatar__fallback-frame.avatar-frame-custom-float { animation: cosmetic-avatar-custom-float 3s ease-in-out infinite; }
  .cosmetic-avatar__fallback-frame.avatar-frame-custom-glitch { animation: cosmetic-avatar-custom-glitch 2s steps(2, start) infinite; }
  .cosmetic-avatar__fallback-frame.avatar-frame-custom-rainbow { animation: cosmetic-avatar-custom-rainbow 6s linear infinite; }
  .cosmetic-avatar__fallback-frame.avatar-frame-custom-breathe { animation: cosmetic-avatar-custom-breathe 3s ease-in-out infinite; }

  @keyframes cosmetic-avatar-pulse { 50% { box-shadow: 0 0 22px rgba(130, 80, 223, .95); } }
  @keyframes cosmetic-avatar-custom-pulse { 50% { filter: brightness(1.45); } }
  @keyframes cosmetic-avatar-custom-spin {
    0% { transform: rotate(0deg); }
    100% { transform: rotate(360deg); }
  }
  @keyframes cosmetic-avatar-custom-ripple {
    0% { transform: scale(0.96); opacity: 1; }
    70% { transform: scale(1.08); opacity: 0.6; }
    100% { transform: scale(1.12); opacity: 0; }
  }
  @keyframes cosmetic-avatar-custom-float {
    0%, 100% { transform: translateY(0); }
    50% { transform: translateY(-4px); }
  }
  @keyframes cosmetic-avatar-custom-glitch {
    0%, 100% { transform: translate(0); }
    25% { transform: translate(-2px, 1px); filter: hue-rotate(90deg); }
    50% { transform: translate(2px, -1px); filter: hue-rotate(180deg); }
    75% { transform: translate(-1px, -1px); filter: hue-rotate(270deg); }
  }
  @keyframes cosmetic-avatar-custom-rainbow {
    0% { filter: hue-rotate(0deg); }
    100% { filter: hue-rotate(360deg); }
  }
  @keyframes cosmetic-avatar-custom-breathe {
    0%, 100% { transform: scale(1); filter: drop-shadow(0 0 4px var(--avatar-frame-color, #d4a017)); }
    50% { transform: scale(1.03); filter: drop-shadow(0 0 14px var(--avatar-frame-color, #d4a017)); }
  }

  @media (prefers-reduced-motion: reduce) {
    .cosmetic-avatar__fallback-frame.avatar-frame-glow,
    .cosmetic-avatar__fallback-frame.avatar-frame-custom.avatar-frame-custom-pulse,
    .cosmetic-avatar__fallback-frame.avatar-frame-custom.avatar-frame-custom-spin,
    .cosmetic-avatar__fallback-frame.avatar-frame-custom.avatar-frame-custom-ripple,
    .cosmetic-avatar__fallback-frame.avatar-frame-custom.avatar-frame-custom-float,
    .cosmetic-avatar__fallback-frame.avatar-frame-custom.avatar-frame-custom-glitch,
    .cosmetic-avatar__fallback-frame.avatar-frame-custom.avatar-frame-custom-rainbow,
    .cosmetic-avatar__fallback-frame.avatar-frame-custom.avatar-frame-custom-breathe { animation: none; }
  }
</style>
