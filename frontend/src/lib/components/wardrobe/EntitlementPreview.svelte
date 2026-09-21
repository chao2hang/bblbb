<!-- M07-UI-05/06：权益行内预览——把后端注册 Token 投影为固定白名单视觉：
  头像框/挂件预览（CosmeticAvatar）、昵称颜色/装饰/前缀预览（CosmeticName）、
  徽章章面、主页装饰色板、帖子装饰标签。
  安全模型与 tokens.ts 一致：只有 projectEntitlementTokens 投影出的白名单
  Token 会渲染，未知/未授权 Token 一律回退到内置图标，绝不解释任意资源。 -->
<script lang="ts">
  import Icon from '$lib/components/ui/Icon.svelte';
  import ProfileCover from '$lib/components/ui/ProfileCover.svelte';
  import type { CosmeticDefStyle } from '$lib/api/types';
  import CosmeticAvatar from './CosmeticAvatar.svelte';
  import CosmeticName from './CosmeticName.svelte';
  import { BADGES, type EntitlementTokenProjection } from './tokens';
  import steamBackgrounds from '$lib/data/steam-profile-backgrounds.json';

  const STEAM_CDN = 'https://shared.fastly.steamstatic.com/community_assets/images/items';
  type ProfileMedia = {
    image: string | null;
    fallbackSrc: string | null;
    webm: string | null;
    mp4: string | null;
  };
  const EMPTY_PROFILE_MEDIA: ProfileMedia = {
    image: null,
    fallbackSrc: null,
    webm: null,
    mp4: null
  };

  function basename(value: string): string {
    return value.split(/[/?#]/).pop() ?? value;
  }

  function isHttpUrl(value: string): boolean {
    return /^https?:\/\//i.test(value);
  }

  function safeMediaUrl(value: string): string | null {
    const trimmed = value.trim();
    if (!trimmed || trimmed.startsWith('//') || trimmed.includes('..')) return null;
    if (isHttpUrl(trimmed)) {
      try {
        const url = new URL(trimmed);
        return url.hostname === 'shared.fastly.steamstatic.com'
          && url.pathname.startsWith('/community_assets/images/items/')
          ? trimmed
          : null;
      } catch {
        return null;
      }
    }
    return trimmed.startsWith('/api/v1/steam-assets/backgrounds/') ? trimmed : null;
  }

  function localAsset(value: string | null | undefined): string | null {
    if (!value || isHttpUrl(value) || value.startsWith('//') || value.includes('..')) return null;
    if (/^[a-z][a-z\d+.-]*:/i.test(value)) return null;
    const file = basename(value);
    return /^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(file)
      ? `/api/v1/steam-assets/backgrounds/${file}`
      : null;
  }

  function steamAppId(value: string | number | null | undefined): string | null {
    if (typeof value === 'number' && Number.isSafeInteger(value)) return String(value);
    if (typeof value !== 'string') return null;
    if (/^\d+$/.test(value)) return value;
    return value.match(/\/items\/(\d+)(?:\/|$)/)?.[1] ?? null;
  }

  function steamCdnUrl(value: string | null | undefined, sibling?: string | number | null): string | null {
    if (!value) return null;
    const safe = safeMediaUrl(value);
    if (safe && isHttpUrl(safe)) return safe;
    if (value.startsWith('/') || isHttpUrl(value) || value.startsWith('//')) return null;
    if (value.includes('..') || /^[a-z][a-z\d+.-]*:/i.test(value)) return null;
    const file = basename(value);
    const appId = steamAppId(sibling);
    return file && appId ? `${STEAM_CDN}/${appId}/${file}` : null;
  }

  function steamAssetUrl(
    value: string | null | undefined,
    sibling?: string | number | null,
    allowLocal = true
  ): string | null {
    if (!value) return null;
    const safe = safeMediaUrl(value);
    if (safe) return safe;
    if (value.startsWith('/') || isHttpUrl(value)) return null;
    return steamCdnUrl(value, sibling) ?? (allowLocal ? localAsset(value) : null);
  }

  function steamMedia(item: { appid: number; image: string; webm?: string | null; mp4?: string | null }): ProfileMedia {
    return {
      image: localAsset(item.image) ?? steamCdnUrl(item.image, item.appid),
      fallbackSrc: steamCdnUrl(item.image, item.appid),
      webm: steamCdnUrl(item.webm, item.appid),
      mp4: steamCdnUrl(item.mp4, item.appid)
    };
  }

  function findSteamBackground(title: string) {
    const normalizedTitle = title.trim().toLowerCase();
    if (!normalizedTitle) return null;
    const matches = steamBackgrounds
      .filter((item) => {
        const name = item.name.trim().toLowerCase();
        // Very short names (for example, the catalog entry "2") are only
        // safe as exact matches; otherwise they match unrelated product titles.
        return normalizedTitle === name || (name.length >= 3 && normalizedTitle.includes(name));
      })
      .sort((a, b) => b.name.length - a.name.length);
    return matches[0] ?? null;
  }

  function findSteamBackgroundById(id: string | null | undefined) {
    if (!id) return null;
    return steamBackgrounds.find((item) => item.id === id || String(item.defid) === id) ?? null;
  }

  function findSteamBackgroundByAsset(value: string | null | undefined) {
    if (!value) return null;
    const file = basename(value).toLowerCase();
    if (!file) return null;
    return steamBackgrounds.find((item) =>
      [item.image, item.webm, item.mp4].some((asset) => !!asset && basename(asset).toLowerCase() === file)
    ) ?? null;
  }

  function resolveProfileMedia(
    style: CosmeticDefStyle | undefined,
    title: string,
    profileEffectId?: string | null
  ): ProfileMedia {
    const catalog = findSteamBackgroundById(profileEffectId)
      ?? findSteamBackgroundByAsset(style?.image ?? style?.webm ?? style?.mp4 ?? style?.url)
      ?? findSteamBackground(title);
    const catalogImage = catalog?.image ?? null;
    const catalogWebm = catalog?.webm ?? null;
    const catalogMp4 = catalog?.mp4 ?? null;

    if (style || catalog) {
      // Any full Steam URL in the style can provide the app id for sibling
      // bare filenames; catalog metadata supplies it when style fields are bare.
      const sibling = [style?.url, style?.image, style?.webm, style?.mp4].find(
        (value) => value && (isHttpUrl(value) || /\/items\/\d+(?:\/|$)/.test(value))
      ) ?? catalog?.appid ?? style?.url ?? style?.image ?? style?.webm ?? style?.mp4;
      const imageSource = style?.url && isHttpUrl(style.url)
        ? style.url
        : style?.image ?? catalogImage ?? style?.url;
      const image = imageSource && !imageSource.startsWith('/') && !isHttpUrl(imageSource)
        ? localAsset(imageSource) ?? steamCdnUrl(imageSource, sibling)
        : steamAssetUrl(imageSource, sibling);
      const fallbackSrc = imageSource && !imageSource.startsWith('/') && !isHttpUrl(imageSource)
        ? steamCdnUrl(imageSource, sibling)
        : null;
      const webm = steamAssetUrl(style?.webm ?? catalogWebm, sibling, false);
      const mp4 = steamAssetUrl(style?.mp4 ?? catalogMp4, sibling, false);
      if (image || webm || mp4) return { image, fallbackSrc, webm, mp4 };
    }
    return EMPTY_PROFILE_MEDIA;
  }

  let {
    projection,
    iconToken = null,
    name = '装扮预览',
    title = '',
    entitlementSlot = null,
    compact = false
  }: {
    projection: EntitlementTokenProjection;
    /** 商品 icon_token（仅内置图标名白名单，未知回退 palette）。 */
    iconToken?: string | null;
    /** 头像/昵称预览文案（传当前用户昵称更直观）。 */
    name?: string;
    /** 商品标题，用于 Steam 背景资源回退匹配。 */
    title?: string;
    /** 商品槽位；仅 profile_effect 允许按标题回退到 Steam 背景。 */
    entitlementSlot?: string | null;
    /** 是否为紧凑插槽预览模式（如左侧装备槽 36x36 容器）。 */
    compact?: boolean;
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
  const steamTitleMatch = $derived(entitlementSlot === 'profile_effect' && Boolean(findSteamBackground(title)));
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
    if (visual.profile_effect || steamTitleMatch) return 'profile_effect' as const;
    if (visual.post_effect) return 'post_effect' as const;
    return 'icon' as const;
  });
  const badgeChips = $derived(
    kind === 'badges'
      ? (visual.profile_badges ?? [])
          .map((code, index) => {
            const fixed = BADGES[code];
            if (fixed) return { ...fixed, label: fixed.label };
            const customName = visual.profile_badge_names?.[index];
            return customName ? { icon: '✦', label: customName } : null;
          })
          .filter((b): b is { icon: string; label: string } => Boolean(b))
      : []
  );
  const swatchClass = $derived(
    kind === 'profile_effect' ? EFFECT_SWATCHES[visual.profile_effect ?? ''] ?? null : null
  );
  const fallbackIcon = $derived(
    typeof iconToken === 'string' && ICON_FALLBACKS.has(iconToken) ? iconToken : 'palette'
  );
  const previewTitle = $derived(projection.labels.join('、'));
  const profileMedia = $derived(kind === 'profile_effect'
    ? resolveProfileMedia(visual.profile_effect_style, title || previewTitle, visual.profile_effect)
    : EMPTY_PROFILE_MEDIA);
</script>

<span class="entitlement-preview {compact ? 'is-compact' : ''}" aria-hidden="true" title={previewTitle || title || undefined}>
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
  {:else if kind === 'profile_effect' && (profileMedia.image || profileMedia.webm || profileMedia.mp4)}
    <span class="profile-preview-plate">
      <ProfileCover
        src={profileMedia.image}
        fallbackSrc={profileMedia.fallbackSrc}
        videoWebm={profileMedia.webm}
        videoMp4={profileMedia.mp4}
        label={title || '个人资料背景'}
      />
      <span class="profile-preview-label">全景背景</span>
    </span>
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
    /* 头像装扮预览底板与圆形头像同形，避免方角底板从圆形头像后露出。 */
    border-radius: 50%;
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
  .profile-preview-plate {
    position: relative;
    display: block;
    width: 100%;
    min-width: 180px;
    height: 72px;
    overflow: hidden;
    border: 1px solid var(--color-border);
    background: #090514;
  }
  .entitlement-preview.is-compact .profile-preview-plate {
    min-width: 0;
    width: 36px;
    height: 36px;
    border-radius: var(--radius-sm);
  }
  .entitlement-preview.is-compact .profile-preview-label {
    display: none;
  }
  .entitlement-preview.is-compact .swatch {
    width: 36px;
    height: 36px;
    border-radius: var(--radius-sm);
  }
  .entitlement-preview.is-compact .icon-plate {
    width: 36px;
    height: 36px;
  }
  .profile-preview-plate :global(.profile-cover) {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
  .profile-preview-label {
    position: absolute;
    right: 6px;
    bottom: 6px;
    z-index: 3;
    padding: 3px 6px;
    color: #fff;
    background: rgb(0 0 0 / 58%);
    font-size: var(--text-xs);
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
