<script lang="ts">
  import type { PublicPresentationTokens } from '$lib/api/types';
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

  function resolveTokens(
    input: PublicPresentationTokens | { presentation_tokens?: PublicPresentationTokens | null } | null
  ): PublicPresentationTokens {
    if (input && 'presentation_tokens' in input) return input.presentation_tokens ?? {};
    return (input as PublicPresentationTokens) ?? {};
  }

  const tokens = $derived(resolveTokens(presentation));
  const colorClass = $derived(nicknameEffectClass(tokens.nickname_color));
  const solidColor = $derived(
    typeof tokens.nickname_color === 'string' ? NICKNAME_COLORS[tokens.nickname_color] ?? null : null
  );
  const classes = $derived(['cosmetic-name', colorClass, klass].filter(Boolean).join(' '));
</script>

<span class={classes} style={solidColor ? `color:${solidColor};` : undefined} aria-label={name} title={name}>
  {name}
</span>

<style>
  .cosmetic-name { display: inline; }
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
    background-size: 220% 100%;
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
  @keyframes cosmetic-name-shift { to { background-position: 220% 0; } }
  @keyframes cosmetic-name-breathe {
    0%, 100% { text-shadow: 0 0 2px rgba(139, 92, 246, 0.25); opacity: 0.82; }
    50% { text-shadow: 0 0 12px rgba(139, 92, 246, 0.85), 0 0 22px rgba(139, 92, 246, 0.45); opacity: 1; }
  }
  @media (prefers-reduced-motion: reduce) {
    .nickname-rainbow { animation: none; }
    .nickname-breathing { animation: none; opacity: 1; text-shadow: 0 0 6px rgba(139, 92, 246, 0.4); }
  }
</style>
