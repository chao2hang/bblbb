<script lang="ts">
  import { onMount } from 'svelte';
  import type { CosmeticDefStyle, PublicPresentationTokens } from '$lib/api/types';
  import { NICKNAME_COLORS, nicknameEffectClass } from './tokens';

  let {
    name,
    presentation = null,
    class: klass = ''
  }: {
    name: string;
    presentation?: PublicPresentationTokens | { presentation_tokens?: PublicPresentationTokens | null } | null;
    class?: string;
  } = $props();

  let spanEl: HTMLSpanElement | null = $state(null);

  function resolveTokens(
    input: PublicPresentationTokens | { presentation_tokens?: PublicPresentationTokens | null } | null
  ): PublicPresentationTokens {
    if (input && 'presentation_tokens' in input) return input.presentation_tokens ?? {};
    return (input as PublicPresentationTokens) ?? {};
  }

  /** 客户端二次防线：只放行 #rrggbb 色值（服务端已校验，双保险）。 */
  function safeColor(v: unknown): string | null {
    return typeof v === 'string' && /^#[0-9a-fA-F]{6}$/.test(v) ? v : null;
  }

  const tokens = $derived(resolveTokens(presentation));
  // 自定义样式库样式（M07-SHOP-UI-10）：结构化参数经客户端色值校验后渲染，
  // 白名单之外（非法色/未知 mode）回退到注册枚举渲染路径。
  const custom = $derived(tokens.nickname_color_style ?? null);
  const customValid = $derived(Boolean(custom && validateCustom(custom)));
  function validateCustom(s: CosmeticDefStyle): boolean {
    if (s.mode === 'solid' || s.mode === 'glow') return Boolean(safeColor(s.color));
    if (s.mode === 'gradient')
      return Array.isArray(s.colors) && s.colors.length >= 2 && s.colors.length <= 5 && s.colors.every((c) => safeColor(c));
    return false;
  }

  const customStyle = $derived.by(() => {
    if (!customValid || !custom) return null;
    const dur = Math.min(Math.max(custom.durationMs ?? 2800, 800), 30000);
    const shadow = typeof custom.shadowPx === 'number' && custom.shadowPx > 0 ? `text-shadow:0 0 ${custom.shadowPx}px ${safeColor(custom.color) || '#8b5cf6'};` : '';
    const spacing = typeof custom.letterSpacing === 'number' && custom.letterSpacing > 0 ? `letter-spacing:${custom.letterSpacing}px;` : '';

    if (custom.mode === 'solid') {
      const color = safeColor(custom.color);
      if (custom.animate === 'shimmer') {
        return `color:transparent;-webkit-text-fill-color:transparent;background-image:linear-gradient(90deg,${color},#ffffff,${color});background-clip:text;-webkit-background-clip:text;background-size:200% 100%;${shadow}${spacing}animation:cosmetic-name-shimmer ${dur}ms ease-in-out infinite;`;
      }
      let animStyle = '';
      if (custom.animate === 'breathe') animStyle = `animation:cosmetic-name-breathe ${dur}ms ease-in-out infinite;`;
      else if (custom.animate === 'pulse') animStyle = `animation:cosmetic-name-pulse ${dur}ms ease-in-out infinite;`;
      else if (custom.animate === 'bounce') animStyle = `animation:cosmetic-name-bounce ${dur}ms ease-in-out infinite;`;
      else if (custom.animate === 'glitch') animStyle = `animation:cosmetic-name-glitch ${dur}ms steps(2, start) infinite;`;
      return `color:${color};${shadow}${spacing}${animStyle}`;
    }

    if (custom.mode === 'glow') {
      const color = safeColor(custom.color);
      let animStyle = '';
      if (custom.animate === 'flicker') animStyle = `animation:cosmetic-name-flicker ${dur}ms infinite;`;
      else if (custom.animate === 'fire') animStyle = `animation:cosmetic-name-fire ${dur}ms ease-in-out infinite;`;
      else if (custom.animate === 'pulse') animStyle = `animation:cosmetic-name-pulse ${dur}ms ease-in-out infinite;`;
      else if (custom.animate === 'glitch') animStyle = `animation:cosmetic-name-glitch ${dur}ms steps(2, start) infinite;`;
      else if (custom.animate !== 'none') animStyle = `animation:cosmetic-name-breathe ${dur}ms ease-in-out infinite;`;
      return `color:${color};${shadow}${spacing}${animStyle}`;
    }

    // gradient：渐变文字（可选流动动画）
    const rawStops = (custom.colors ?? []).map((c) => safeColor(c)).filter((c): c is string => Boolean(c));
    const flowStops =
      custom.animate === 'flow' && rawStops.length >= 2 && rawStops[0] !== rawStops[rawStops.length - 1]
        ? [...rawStops, rawStops[0]]
        : rawStops;
    const stops = flowStops.join(',');
    const angle = typeof custom.angle === 'number' ? `${custom.angle}deg` : '90deg';
    let anim = '';
    if (custom.animate === 'flow') anim = `animation:cosmetic-name-shift ${dur}ms linear infinite;`;
    else if (custom.animate === 'wave') anim = `animation:cosmetic-name-wave ${dur}ms ease-in-out infinite;`;
    else if (custom.animate === 'shimmer') anim = `animation:cosmetic-name-shimmer ${dur}ms ease-in-out infinite;`;
    else if (custom.animate === 'glitch') anim = `animation:cosmetic-name-glitch ${dur}ms steps(2, start) infinite;`;
    else if (custom.animate === 'rainbow') anim = `animation:cosmetic-name-rainbow-cycle ${dur}ms linear infinite;`;
    else if (custom.animate === 'pulse') anim = `animation:cosmetic-name-pulse ${dur}ms ease-in-out infinite;`;

    return `color:transparent;-webkit-text-fill-color:transparent;background-image:linear-gradient(${angle},${stops});background-clip:text;-webkit-background-clip:text;background-size:200% 100%;${shadow}${spacing}${anim}`;
  });

  const colorClass = $derived(customStyle ? null : nicknameEffectClass(tokens.nickname_color));
  const solidColor = $derived(
    !customStyle && typeof tokens.nickname_color === 'string' ? NICKNAME_COLORS[tokens.nickname_color] ?? null : null
  );
  const customClass = $derived(
    customStyle && custom?.mode === 'glow' && (custom.animate === 'breathe' || !custom.animate) ? 'nickname-custom-glow' : null
  );
  const classes = $derived(['cosmetic-name', customClass, colorClass, klass].filter(Boolean).join(' '));
  const renderedName = $derived(tokens.title_prefix_name ? `${tokens.title_prefix_name} ${name}` : name);

  // 开放 JS 动画与交互 Hook
  onMount(() => {
    if (custom?.js && spanEl) {
      try {
        const fn = new Function('el', 'style', custom.js);
        fn(spanEl, custom);
      } catch (err) {
        console.warn('Cosmetic custom JS execution error:', err);
      }
    }
  });
</script>

{#if custom?.css}
  <svelte:element this={'style'}>
    {custom.css}
  </svelte:element>
{/if}

<span
  bind:this={spanEl}
  class={classes}
  style={customStyle ?? (solidColor ? `color:${solidColor};` : undefined)}
  aria-label={renderedName}
  title={tokens.nickname_color_name ?? tokens.title_prefix_name ?? name}
>
  {renderedName}
</span>

<style>
  .cosmetic-name {
    display: inline-block;
    vertical-align: baseline;
  }
  .nickname-solid-blue { color: #0969da; }
  .nickname-solid-purple { color: #8250df; }
  .nickname-solid-green { color: #1a7f37; }
  .nickname-solid-gold { color: #8a6500; }
  .nickname-solid-red { color: #cf222e; }
  .nickname-solid-teal { color: #0e8a16; }
  .nickname-solid-pink { color: #b51d64; }
  .nickname-rainbow,
  .nickname-gradient-sunset,
  .nickname-gradient-ocean,
  .nickname-gradient-aurora {
    color: transparent;
    background-clip: text;
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    background-size: 200% 100%;
  }
  .nickname-rainbow {
    background-image: linear-gradient(90deg, #e53935, #fb8c00, #fdd835, #43a047, #1e88e5, #8e24aa, #e53935);
    animation: cosmetic-name-shift 5s linear infinite;
  }
  .nickname-gradient-sunset { background-image: linear-gradient(90deg, #c2410c, #db2777, #7c3aed); }
  .nickname-gradient-ocean { background-image: linear-gradient(90deg, #0369a1, #0d9488, #22c55e); }
  .nickname-gradient-aurora { background-image: linear-gradient(90deg, #0f766e, #2563eb, #9333ea); }
  /* 呼吸微光：紫色基底 + 缓慢明暗的光晕（区别于彩虹的色相流动）。 */
  .nickname-breathing {
    color: #8b5cf6;
    animation: cosmetic-name-breathe 2.8s ease-in-out infinite;
  }
  /* 自定义样式库呼吸光（M07-SHOP-UI-10）：颜色/时长经内联样式注入。 */
  .nickname-custom-glow {
    animation: cosmetic-name-breathe 2.8s ease-in-out infinite;
  }

  @keyframes -global-cosmetic-name-shift {
    0% { background-position: 0% 50%; }
    100% { background-position: 200% 50%; }
  }
  @keyframes -global-cosmetic-name-breathe {
    0%, 100% { text-shadow: 0 0 2px currentColor; opacity: 0.82; }
    50% { text-shadow: 0 0 12px currentColor, 0 0 22px currentColor; opacity: 1; }
  }
  @keyframes -global-cosmetic-name-pulse {
    0%, 100% { transform: scale(1); opacity: 0.9; }
    50% { transform: scale(1.04); opacity: 1; }
  }
  @keyframes -global-cosmetic-name-bounce {
    0%, 100% { transform: translateY(0); }
    50% { transform: translateY(-2px); }
  }
  @keyframes -global-cosmetic-name-wave {
    0% { background-position: 0% 50%; }
    50% { background-position: 100% 50%; }
    100% { background-position: 0% 50%; }
  }
  @keyframes -global-cosmetic-name-shimmer {
    0% { background-position: -200% 0; }
    100% { background-position: 200% 0; }
  }
  @keyframes -global-cosmetic-name-glitch {
    0%, 100% { transform: translate(0); filter: drop-shadow(0 0 0 transparent); }
    20% { transform: translate(-1px, 1px); filter: drop-shadow(-1px 0 #00ffff); }
    40% { transform: translate(1px, -1px); filter: drop-shadow(1px 0 #ff0055); }
    60% { transform: translate(-0.5px, -0.5px); filter: drop-shadow(0 0 4px #a855f7); }
    80% { transform: translate(0.5px, 0.5px); }
  }
  @keyframes -global-cosmetic-name-rainbow-cycle {
    0% { filter: hue-rotate(0deg); }
    100% { filter: hue-rotate(360deg); }
  }
  @keyframes -global-cosmetic-name-flicker {
    0%, 19.999%, 22%, 62.999%, 64%, 64.999%, 70%, 100% {
      opacity: 1;
      text-shadow: 0 0 8px currentColor, 0 0 16px currentColor;
    }
    20%, 21.999%, 63%, 63.999%, 65%, 69.999% {
      opacity: 0.4;
      text-shadow: none;
    }
  }
  @keyframes -global-cosmetic-name-fire {
    0%, 100% { text-shadow: 0 0 4px #ff4500, 0 -2px 8px #ff8c00; transform: scale(1); }
    50% { text-shadow: 0 0 8px #ff6347, 0 -4px 14px #ffd700; transform: scale(1.02); }
  }

  @media (prefers-reduced-motion: reduce) {
    .cosmetic-name { animation: none !important; }
    .nickname-rainbow { animation: none; }
    .nickname-breathing { animation: none; opacity: 1; text-shadow: 0 0 6px currentColor; }
    .nickname-custom-glow { animation: none; opacity: 1; }
  }
</style>
