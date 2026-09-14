<!-- M07-UI-05/06：权益行内预览——把后端注册 Token 投影为固定白名单视觉：
  头像框/挂件预览（CosmeticAvatar）、昵称颜色/装饰/前缀预览（CosmeticName）、
  徽章章面、主页装饰色板、帖子装饰标签。
  安全模型与 tokens.ts 一致：只有 projectEntitlementTokens 投影出的白名单
  Token 会渲染，未知/未授权 Token 一律回退到内置图标，绝不解释任意资源。 -->
<script lang="ts">
  import Icon from '$lib/components/ui/Icon.svelte';
  import CosmeticAvatar from './CosmeticAvatar.svelte';
  import CosmeticName from './CosmeticName.svelte';
  import { BADGES, type EntitlementTokenProjection } from './tokens';

  let {
    projection,
    iconToken = null,
    name = '装扮预览'
  }: {
    projection: EntitlementTokenProjection;
    /** 商品 icon_token（仅内置图标名白名单，未知回退 palette）。 */
    iconToken?: string | null;
    /** 头像/昵称预览文案（传当前用户昵称更直观）。 */
    name?: string;
  } = $props();

  const ICON_FALLBACKS = new Set([
    'shopping-bag',
    'star',
    'sparkles',
    'heart',
    'award',
    'palette',
    'trophy',
    'wand-2'
  ]);

  /** 主页装饰 Token → 色板样式（组件内固定实现，值来自白名单枚举）。 */
  const EFFECT_SWATCHES: Record<string, string> = {
    sparkle: 'swatch-sparkle',
    dark_stars: 'swatch-dark-stars'
  };

  const visual = $derived(projection.visual);
  const kind = $derived.by(() => {
    if (
      visual.avatar_frame ||
      visual.avatar_frame_attachment_id
    ) {
      return 'avatar' as const;
    }
    if (visual.nickname_color) {
      return 'name' as const;
    }
    if (visual.profile_badges && visual.profile_badges.length > 0) return 'badges' as const;
    if (visual.profile_effect) return 'profile_effect' as const;
    if (visual.post_effect) return 'post_effect' as const;
    return 'icon' as const;
  });
  const badgeChips = $derived(
    kind === 'badges'
      ? (visual.profile_badges ?? []).map((b) => BADGES[b]).filter((b) => Boolean(b))
      : []
  );
  const swatchClass = $derived(
    kind === 'profile_effect' ? EFFECT_SWATCHES[visual.profile_effect ?? ''] ?? null : null
  );
  const fallbackIcon = $derived(
    typeof iconToken === 'string' && ICON_FALLBACKS.has(iconToken) ? iconToken : 'palette'
  );
  const previewTitle = $derived(projection.labels.join('、'));
</script>

<span class="entitlement-preview" aria-hidden="true" title={previewTitle || null}>
  {#if kind === 'avatar'}
    <span class="avatar-plate">
      <CosmeticAvatar {name} size="lg" presentation={visual} />
    </span>
  {:else if kind === 'name'}
    <span class="name-plate"><CosmeticName {name} presentation={visual} /></span>
  {:else if kind === 'badges'}
    {#each badgeChips as chip}
      <span class="badge-chip">{chip.icon} {chip.label}</span>
    {/each}
  {:else if kind === 'profile_effect' && swatchClass}
    <span class="swatch {swatchClass}"></span>
  {:else if kind === 'post_effect'}
    <span class="badge-chip">
      <Icon name={visual.post_effect === 'thanks' ? 'heart' : 'sparkles'} size={14} />
      {projection.labels[0] ?? '帖子装饰'}
    </span>
  {:else}
    <span class="icon-plate"><Icon name={fallbackIcon} size={20} /></span>
  {/if}
</span>

<style>
  .entitlement-preview {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    flex: 0 0 auto;
    max-width: 220px;
    overflow: hidden;
  }
  .avatar-plate {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 48px;
    height: 48px;
    background: var(--color-bg-subtle);
    border: 1px solid var(--color-border);
  }
  .name-plate {
    display: inline-flex;
    align-items: center;
    max-width: 220px;
    overflow: hidden;
    white-space: nowrap;
    font-size: var(--text-sm);
    font-weight: 600;
  }
  .badge-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--color-text-primary);
    background: var(--color-bg-subtle);
    border: 1px solid var(--color-border);
    white-space: nowrap;
  }
  .icon-plate {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 48px;
    height: 48px;
    color: var(--color-text-secondary);
    background: var(--color-bg-subtle);
    border: 1px solid var(--color-border);
  }
  .swatch {
    display: inline-block;
    width: 48px;
    height: 48px;
    border: 1px solid var(--color-border);
  }
  .swatch-sparkle {
    background:
      radial-gradient(circle at 28% 30%, rgba(255, 215, 0, 0.5), transparent 55%),
      radial-gradient(circle at 72% 72%, rgba(9, 105, 218, 0.38), transparent 60%),
      var(--color-bg-subtle);
  }
  .swatch-dark-stars {
    background:
      radial-gradient(circle at 30% 40%, rgba(130, 80, 223, 0.55), transparent 55%),
      radial-gradient(circle at 70% 62%, rgba(9, 105, 218, 0.4), transparent 58%),
      #0b1020;
  }
  @media (prefers-reduced-motion: reduce) {
    .entitlement-preview * {
      animation: none !important;
    }
  }
</style>
