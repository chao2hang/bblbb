<script lang="ts">
  // M02-UX-05：/me 个人主页——个人中心（tag「账户资产」默认页）。
  // 移动端 IA 对齐成熟个人中心（淘宝/哔哩哔哩：统计条 + 分组列表；
  // linux.do/GitHub：身份头 + 独立 tab 页），结构：
  // - 资料卡（通栏 hero：封面/头像/徽章/双按钮/签名，服务端安全投影，
  //   不输出任何会话 token）；
  // - 资产统计条（三个可点统计卡：余额/签到 → /me/balance，设备 →
  //   /me/security；GET /activity/summary 失败时整条隐藏）；
  // - 分组 cell 列表（内容与互动 / 资产与凭证 / 账户与偏好，全部入口）；
  // - 我的处罚区块（GET /me/sanctions，空列表不渲染；有记录时显示
  //   类型/原因/时间 + 去申诉入口）。
  // 分区导航：MeSectionTabs tag 栏（每个 tag 独立路由页面）——
  // 账号安全 → /me/security，信任等级 → /me/level，状态/进度详情在对应页面，
  // 本页不重复。注：本页顶部已有完整资料卡，不渲染 MeIdentityBar，避免用户信息重复。
  import CosmeticAvatar from '$lib/components/wardrobe/CosmeticAvatar.svelte';
  import CosmeticName from '$lib/components/wardrobe/CosmeticName.svelte';
  import ProfileCover from '$lib/components/ui/ProfileCover.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { formatCount, formatRelative } from '$lib/utils';
  import type { MePageData } from './+page.server';
  import type { CosmeticDefStyle, PostSummary, PublicPresentationTokens, PublicProfile } from '$lib/api/types';
  import PageTitle from '$lib/components/PageTitle.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import steamBackgrounds from '$lib/data/steam-profile-backgrounds.json';
  import { normalizeSlot, projectEntitlementTokens } from '$lib/components/wardrobe/tokens';
  import { profileEffectClass, profileEffectStyle } from '$lib/components/wardrobe/profile-effect';
  import { getCurrencyNameContext } from '$lib/site/currency-context.svelte';

  type PublicProfileWithStats = PublicProfile & {
    post_count?: number | null;
    followers?: number | null;
    following?: number | null;
    is_following?: boolean | null;
  };

  let { data }: { data: MePageData } = $props();

  const user = $derived(data.user);
  const profile = $derived<PublicProfileWithStats | null>((data.profile as PublicProfileWithStats | null) ?? null);
  const presentation = $derived(data.presentation ?? null);
  const sessions = $derived(data.sessions);
  const error = $derived(data.error);
  const currencyName = $derived(getCurrencyNameContext()?.currencyName ?? '金币');
  // GAP-FIX 账户卡 / 我的处罚（load 增强数据；缺失时安全降级不渲染）。
  const activity = $derived(data.activity ?? null);
  const sanctions = $derived(data.sanctions ?? []);
  // M20-TRUST 信任等级进度（缺失时安全降级不渲染）。
  const trust = $derived(data.trust ?? null);
  const coinBalance = $derived((activity?.balances ?? []).find((b) => b.currency === 'coin'));

  // ── 我的帖子流（客户端直连获取当前用户的创作内容） ──
  let posts = $state<PostSummary[]>([]);
  let postsLoading = $state(false);
  let postsLoaded = $state(false);
  let postsError = $state<string | null>(null);

  async function fetchMyPosts(): Promise<void> {
    if (!user?.username) return;
    postsLoading = true;
    postsError = null;
    try {
      const params = new URLSearchParams({ author_username: user.username, limit: '10' });
      const response = await fetch(`/api/v1/posts?${params.toString()}`, {
        headers: { Accept: 'application/json' },
        credentials: 'same-origin'
      });
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      const result = (await response.json()) as { items?: PostSummary[] };
      posts = Array.isArray(result.items) ? result.items : [];
    } catch {
      posts = [];
      postsError = '帖子列表加载失败';
    }
    postsLoading = false;
    postsLoaded = true;
  }

  $effect(() => {
    if (user?.username) {
      void fetchMyPosts();
    }
  });

  // ── 分组功能列表（全部入口按领域分组）──────────
  interface MeCell {
    href: string;
    icon: string;
    label: string;
  }

  const contentCells: MeCell[] = [
    { href: '/favorites', icon: 'star', label: '我的收藏' },
    { href: '/me/attachments', icon: 'paperclip', label: '我的附件' },
    { href: '/messages', icon: 'mail', label: '私信' }
  ];

  const assetCells: MeCell[] = [
    { href: '/me/balance', icon: 'coins', label: '积分明细' },
    { href: '/me/billing', icon: 'download', label: '下载账单' },
    { href: '/apikeys', icon: 'key', label: 'API 密钥' }
  ];

  const accountCells: MeCell[] = [
    { href: '/me/security', icon: 'shield', label: '账号与安全' },
    { href: '/settings', icon: 'settings', label: '账号设置' },
    { href: '/settings#settings-notifications', icon: 'bell', label: '通知设置' },
    { href: '/settings#settings-oauth', icon: 'key', label: 'OAuth 授权' },
    { href: '/me/level', icon: 'award', label: '信任等级' }
  ];

  /** 处罚类型中文标签（moderation SanctionKind；未知值原样展示）。 */
  const sanctionKindLabels: Record<string, string> = {
    warning: '警告',
    rate_limit: '限流',
    mute: '禁言',
    board_mute: '板块禁言',
    ban: '封禁',
    suspend: '暂停'
  };

  /** 后端时间戳为毫秒（M01-DB-08），formatRelative 口径为秒。 */
  function toSeconds(ts: number | null | undefined): number | null {
    if (typeof ts !== 'number' || !Number.isFinite(ts) || ts <= 0) return null;
    return ts > 1e11 ? Math.floor(ts / 1000) : ts;
  }

  const statusLabel: Record<string, string> = {
    active: '正常',
    pending: '待验证',
    restricted: '受限',
    banned: '已封禁',
    deleted: '已删除'
  };

  function statusBadge(status: string): string {
    switch (status) {
      case 'active':
        return 'badge-success';
      case 'pending':
        return 'badge-warning';
      case 'restricted':
        return 'badge-warning';
      case 'banned':
      case 'deleted':
        return 'badge-danger';
      default:
        return 'badge-neutral';
    }
  }

  function roleLabel(role: string): string {
    if (!role) return '成员';
    const r = role.toLowerCase();
    const map: Record<string, string> = {
      admin: '管理员',
      administrator: '管理员',
      mod: '版主',
      moderator: '版主',
      member: '成员'
    };
    return map[r] ?? role;
  }

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
    // Only static posters are bundled locally. Video filenames must continue
    // to the Steam CDN; mapping them to a local path makes the <video> element
    // fail silently before it ever reaches the working remote source.
    return /\.(?:avif|gif|jpe?g|png|webp)$/i.test(file)
      && /^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(file)
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

  function findSteamBackground(title: string) {
    const normalizedTitle = title.trim().toLowerCase();
    if (!normalizedTitle) return null;
    return steamBackgrounds
      .filter((item) => {
        const name = item.name.trim().toLowerCase();
        return normalizedTitle === name || (name.length >= 3 && normalizedTitle.includes(name));
      })
      .sort((a, b) => b.name.length - a.name.length)[0] ?? null;
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
      [item.image, item.webm, item.mp4].some(
        (asset) => typeof asset === 'string' && basename(asset).toLowerCase() === file
      )
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

    if (!style && !catalog) return EMPTY_PROFILE_MEDIA;

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
    const webmValue = style?.webm ?? catalogWebm;
    const mp4Value = style?.mp4 ?? catalogMp4;
    // Posters can be bundled locally; the animated streams stay on the Steam CDN.
    // Mapping a video filename to /cosmetics/backgrounds first makes the browser
    // fail silently before it ever reaches the working remote source.
    const webm = catalog?.webm
      ? `/api/v1/steam-assets/backgrounds/${catalog.webm}`
      : steamAssetUrl(webmValue, sibling, false);
    const mp4 = catalog?.mp4
      ? `/api/v1/steam-assets/backgrounds/${catalog.mp4}`
      : steamAssetUrl(mp4Value, sibling, false);
    if (image || webm || mp4) return { image, fallbackSrc, webm, mp4 };
    return EMPTY_PROFILE_MEDIA;
  }

  const meEntitlements = $derived(Array.isArray(data.entitlements) ? data.entitlements : []);
  const meCosmetics = $derived(Array.isArray(data.cosmetics) ? data.cosmetics : []);
  const equippedProfileEffect = $derived(
    meEntitlements.find(
      (entitlement) => entitlement.status === 'equipped'
        && (normalizeSlot(entitlement.slot) === 'profile_effect' || entitlement.kind === 'profile_effect')
    ) ?? null
  );
  const entitlementProjection = $derived(
    equippedProfileEffect
      ? projectEntitlementTokens(
          equippedProfileEffect.presentation_tokens,
          equippedProfileEffect.asset_attachment_id,
          normalizeSlot(equippedProfileEffect.slot),
          meCosmetics
        )
      : null
  );
  const cosmeticPresentation = $derived<PublicPresentationTokens>(
    (presentation?.presentation_tokens ?? user?.presentation_tokens ?? {}) as PublicPresentationTokens
  );
  const effectId = $derived(
    entitlementProjection?.visual.profile_effect
      ?? (typeof cosmeticPresentation.profile_effect === 'string' ? cosmeticPresentation.profile_effect : null)
      ?? presentation?.profile_effect_id
      ?? equippedProfileEffect?.product_id
  );
  const matchedDef = $derived.by(() => {
    const byId = effectId
      ? meCosmetics.find((cosmetic) => cosmetic.kind === 'profile_effect' && cosmetic.id === effectId)
      : null;
    if (byId) return byId;
    const normalizedTitle = equippedProfileEffect?.product_title?.trim().toLowerCase() ?? '';
    if (!normalizedTitle) return null;
    return meCosmetics
      .filter((cosmetic) => {
        if (cosmetic.kind !== 'profile_effect') return false;
        const name = cosmetic.name.trim().toLowerCase();
        return name === normalizedTitle
          || (name.length >= 3 && normalizedTitle.length >= 3
            && (normalizedTitle.includes(name) || name.includes(normalizedTitle)));
      })
      .sort((a, b) => b.name.length - a.name.length)[0] ?? null;
  });
  const effectStyle = $derived<CosmeticDefStyle | undefined>(
    entitlementProjection?.visual.profile_effect_style
      ?? cosmeticPresentation.profile_effect_style
      ?? matchedDef?.style
  );
  const effectTitle = $derived(
    equippedProfileEffect?.product_title?.trim() || matchedDef?.name?.trim() || ''
  );
  const profileMedia = $derived(
    resolveProfileMedia(effectStyle, effectTitle, effectId)
  );
  const liveProfileEffectClass = $derived(profileEffectClass(effectStyle, effectId));
  const liveProfileEffectStyle = $derived(profileEffectStyle(effectStyle));
  const equippedCosmeticBadges = $derived(
    (cosmeticPresentation.profile_badges ?? []).map((code, index) => ({
      code,
      label: cosmeticPresentation.profile_badge_names?.[index] ?? code
    }))
  );
</script>

  <PageTitle title="我的" />

<div class="container page-content app-page" id="page-me">
  <h1 class="u-visually-hidden">我的</h1>

  {#if error}
    <div class="app-notice is-danger" role="alert">
      <span>{error}</span>
      <a href="/me">重新加载</a>
    </div>
  {/if}

  {#if user}
    <!-- 个人资料卡：参考信息小卡片设计，集中展示个人信息与账号状态 -->
    <section class="me-profile-card" aria-label="个人信息">
      <div class="me-coverwrap">
        <ProfileCover
          attachmentId={data.cover?.attachment_id}
          src={profileMedia.image}
          fallbackSrc={profileMedia.fallbackSrc}
          videoWebm={profileMedia.webm}
          videoMp4={profileMedia.mp4}
          label="个人资料背景"
          class="me-cover {liveProfileEffectClass}"
           style={liveProfileEffectStyle || undefined}
        />
        <span class="badge badge-level me-cover-level">TL{trust?.level ?? user.level ?? 0}{trust?.name ? ` · ${trust.name}` : ''}</span>
        <div class="me-head">
          <div class="me-avatar">
            <CosmeticAvatar
              name={user.display_name || user.username}
              size="xl"
              presentation={cosmeticPresentation}
              avatarAttachmentId={user.avatar_attachment_id}
              seed={user.username ?? user.id}
            />
          </div>
          <div class="me-identity">
            <h2 class="me-name">
              <CosmeticName name={user.display_name || user.username} presentation={cosmeticPresentation} />
            </h2>
            <span class="me-handle">@{user.username}</span>
          </div>
        </div>
      </div>

      <div class="me-body">
        <div class="me-body-header">
          <div class="me-badges">
            {#if user.roles.length > 0}
              <span class="badge badge-role-admin">{roleLabel(user.roles[0])}</span>
            {:else}
              <span class="badge badge-neutral">成员</span>
            {/if}
            <span class="badge {statusBadge(user.status)}">{statusLabel[user.status] ?? user.status}</span>
            {#if user.mfa_enabled}
              <span class="badge badge-success">2FA 已开启</span>
            {/if}
            {#each equippedCosmeticBadges as badge}
              <span class="badge badge-neutral">✦ {badge.label}</span>
            {/each}
            {#if cosmeticPresentation.post_effect}
              <span class="badge badge-brand">✨ {cosmeticPresentation.post_effect_name ?? '帖子装饰'}</span>
            {/if}
          </div>
          <div class="me-actions">
            <Button text="编辑资料" variant="secondary" size="sm" icon="edit-3" href="/settings" />
            <Button text="我的装扮" variant="secondary" size="sm" icon="sparkles" href="/me/wardrobe" />
          </div>
        </div>

        {#if user.signature}
          <p class="me-bio">{user.signature}</p>
        {:else}
          <p class="me-bio is-empty">暂无个性签名</p>
        {/if}

        <!-- 社区社交指标（帖子/关注/粉丝） -->
        {#if profile && (profile.post_count !== undefined || profile.followers !== undefined || profile.following !== undefined)}
          <div class="me-social-stats">
            <a class="me-social-stat" href="/users/{encodeURIComponent(user.username)}?tab=posts">
              <span class="me-social-num">{profile.post_count ?? 0}</span>
              <span class="me-social-lbl">帖子</span>
            </a>
            <a class="me-social-stat" href="/users/{encodeURIComponent(user.username)}/followers">
              <span class="me-social-num">{profile.followers ?? 0}</span>
              <span class="me-social-lbl">关注者</span>
            </a>
            <a class="me-social-stat" href="/users/{encodeURIComponent(user.username)}/following">
              <span class="me-social-num">{profile.following ?? 0}</span>
              <span class="me-social-lbl">正在关注</span>
            </a>
          </div>
        {/if}
      </div>
    </section>

    <!-- 社区资产与状态胶囊（余额、签到、设备安全） -->
    {#if activity}
      <div class="me-stat-row" style="margin-top:10px;" role="group" aria-label="账户资产统计">
        <a class="me-stat" href="/me/balance">
          <span class="me-stat-value">{coinBalance ? coinBalance.amount : 0}</span>
          <span class="me-stat-label">{currencyName}余额</span>
        </a>
        <a class="me-stat" href="/me/balance">
          <span class="me-stat-value">{activity.streak_days}<span class="me-stat-unit">天</span></span>
          <span class="me-stat-label">{activity.checked_in_today ? '今日已签' : '连续签到'}</span>
        </a>
        <a class="me-stat" href="/me/security">
          <span class="me-stat-value">{sessions.length}<span class="me-stat-unit">台</span></span>
          <span class="me-stat-label">登录设备</span>
        </a>
      </div>
    {/if}

    <!-- 个人内容与创作流（论坛核心心智：查看本人近期发表的内容） -->
    <section class="card me-posts-section" aria-label="我的创作">
      <div class="card-header me-posts-header">
        <span class="card-title">我的发布</span>
        <a class="me-posts-all" href="/users/{encodeURIComponent(user.username)}?tab=posts">
          全部内容 <Icon name="chevron-right" size={14} />
        </a>
      </div>
      <div class="card-body me-posts-body">
        {#if postsLoading}
          <div class="empty-state"><div class="empty-state-title">内容加载中…</div></div>
        {:else if postsError}
          <p class="input-hint is-error" role="alert" style="padding:var(--space-3);">{postsError}</p>
        {:else if postsLoaded && posts.length === 0}
          <div style="padding:var(--space-4);">
            <EmptyState icon="file-text" title="暂未发布内容" desc="在社区分享你的观点或提问吧" />
            <div style="display:flex;justify-content:center;margin-top:var(--space-2);">
              <Button text="发布新帖" variant="primary" size="sm" icon="pen-line" href="/editor" />
            </div>
          </div>
        {:else if posts.length > 0}
          <div class="app-post-list" role="feed" aria-label="我发布的帖子">
            {#each posts as post (post.id)}
              <div class="app-post-row me-post-item" role="article" aria-label={post.title}>
                <div class="app-post-row__main">
                  <h3 class="me-post-title">
                    <a href="/posts/{encodeURIComponent(post.id)}">{post.title}</a>
                  </h3>
                  <div class="app-post-row__meta me-post-meta">
                    {#if post.board_name}
                      <span class="me-post-board">{post.board_name}</span>
                      <span>·</span>
                    {/if}
                    <span>{formatCount(post.reply_count)} 回复</span>
                    <span>·</span>
                    <span>{formatCount(post.view_count)} 浏览</span>
                    {#if post.created_at}
                      <span>·</span>
                      <span>{formatRelative(toSeconds(post.created_at))}</span>
                    {/if}
                  </div>
                </div>
              </div>
            {/each}
          </div>
        {:else}
          <!-- SSR / 无 JS 降级 -->
          <div style="padding:var(--space-3) var(--space-4);">
            <a class="text-link" href="/users/{encodeURIComponent(user.username)}?tab=posts">查看我的全部文章与讨论 &rarr;</a>
          </div>
        {/if}
      </div>
    </section>

    <!-- 常用服务与偏好入口（桌面端双栏展示，移动端单列极简，不重复上方内容） -->
    <div class="content-grid" style="margin-top:10px;">
      <div class="main-col">
        <div class="card me-cell-card">
          <div class="card-header"><span class="card-title">内容与附件</span></div>
          <div class="card-body me-cell-list">
            <a class="me-cell" href="/favorites">
              <span class="me-cell-icon"><Icon name="star" size={16} /></span>
              <span class="me-cell-label">我的收藏</span>
              <Icon class="me-cell-chevron" name="chevron-right" size={14} />
            </a>
            <a class="me-cell" href="/me/attachments">
              <span class="me-cell-icon"><Icon name="paperclip" size={16} /></span>
              <span class="me-cell-label">我的附件</span>
              <Icon class="me-cell-chevron" name="chevron-right" size={14} />
            </a>
            <a class="me-cell" href="/messages">
              <span class="me-cell-icon"><Icon name="mail" size={16} /></span>
              <span class="me-cell-label">私信会话</span>
              <Icon class="me-cell-chevron" name="chevron-right" size={14} />
            </a>
          </div>
        </div>

        <div class="card me-cell-card">
          <div class="card-header"><span class="card-title">资产与凭证</span></div>
          <div class="card-body me-cell-list">
            {#each assetCells as cell (cell.href)}
              <a class="me-cell" href={cell.href}>
                <span class="me-cell-icon"><Icon name={cell.icon} size={16} /></span>
                <span class="me-cell-label">{cell.label}</span>
                {#if cell.href === '/me/balance' && activity}
                  <span class="me-cell-value">{coinBalance ? coinBalance.amount : 0}</span>
                {/if}
                <Icon class="me-cell-chevron" name="chevron-right" size={14} />
              </a>
            {/each}
          </div>
        </div>
      </div>
      <div class="side-col">
        <div class="card me-cell-card">
          <div class="card-header"><span class="card-title">系统与偏好</span></div>
          <div class="card-body me-cell-list">
            {#each accountCells as cell (cell.href)}
              <a class="me-cell" href={cell.href}>
                <span class="me-cell-icon"><Icon name={cell.icon} size={16} /></span>
                <span class="me-cell-label">{cell.label}</span>
                {#if cell.href === '/me/security'}
                  <span class="me-cell-value">{sessions.length} 台设备</span>
                {/if}
                <Icon class="me-cell-chevron" name="chevron-right" size={14} />
              </a>
            {/each}
          </div>
        </div>
      </div>
    </div>

    {#if sanctions.length > 0}
      <!-- GAP-FIX 我的处罚：load 取 GET /me/sanctions；空列表时不渲染此卡。 -->
      <div class="card" id="sanctions" style="margin-top:var(--space-5);border-color:var(--color-warning);">
        <div class="card-header">
          <span class="card-title">我的处罚记录</span>
          <span class="badge badge-warning">{sanctions.length} 条</span>
        </div>
        <div class="card-body" style="padding:0;">
          <ul style="list-style:none;margin:0;padding:0;display:flex;flex-direction:column;">
            {#each sanctions as sanction (sanction.id)}
              <li style="padding:var(--space-3) var(--space-4);border-bottom:var(--border-default);display:flex;flex-wrap:wrap;gap:var(--space-2);align-items:center;">
                <span class="badge badge-warning">{sanctionKindLabels[sanction.kind] ?? sanction.kind}</span>
                <span style="flex:1;min-width:0;">{sanction.reason}</span>
                <span class="text-secondary" style="font-size:var(--text-xs);">
                  {formatRelative(toSeconds(sanction.created_at))}
                  {#if sanction.expires_at}
                    · 至 {formatRelative(toSeconds(sanction.expires_at))}
                  {:else}
                    · 未注明期限
                  {/if}
                </span>
                <a class="btn btn-secondary btn-sm" href="/moderation/appeals?create">去申诉</a>
              </li>
            {/each}
          </ul>
          <p class="input-hint" style="padding:var(--space-2) var(--space-4);margin:0;">
            对处罚有异议可提交申诉，由管理团队复核；申诉入口会要求处罚 ID（处罚通知中的 ID）。
          </p>
        </div>
      </div>
    {/if}
  {:else if !error}
    <div class="empty-state"><div class="empty-state-title">加载中…</div></div>
  {/if}
</div>

<style>
  /* ── 资产统计条：一张卡三列可点统计（≤767px 在 mobile.css §20 通栏化） */
  .me-stat-row {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    background: var(--color-bg-card);
    border: var(--border-default);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-sm);
    overflow: hidden;
  }
  .me-stat {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    padding: 14px 8px 12px;
    text-decoration: none;
    transition: background 0.15s ease;
  }
  .me-stat + .me-stat {
    border-left: 1px solid var(--color-border);
  }
  .me-stat:hover {
    background: var(--color-bg-subtle);
  }
  .me-stat-value {
    font-size: 18px;
    font-weight: var(--weight-bold);
    color: var(--color-text-primary);
    font-variant-numeric: tabular-nums;
    line-height: 1.2;
  }
  .me-stat-unit {
    font-size: 12px;
    font-weight: var(--weight-medium);
    color: var(--color-text-tertiary);
    margin-left: 1px;
  }
  .me-stat-label {
    font-size: 12px;
    color: var(--color-text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
  }

  /* ── 常用工具金刚区（4-Grid 扁平快捷卡片） ──────────────────────── */
  .me-grid-tools {
    display: none;
  }

  /* ── 个人内容流区块 ── */
  .me-posts-section {
    margin-top: 10px;
    border: var(--border-default);
    border-radius: var(--radius-lg);
    background: var(--color-bg-card);
    overflow: hidden;
  }
  .me-posts-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 14px 8px;
  }
  .me-posts-all {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    font-size: var(--text-xs);
    color: var(--color-brand);
    text-decoration: none;
  }
  .me-posts-body {
    padding: 0 !important;
  }
  .me-post-item {
    padding: 12px 14px;
    border-bottom: 1px solid var(--color-border);
  }
  .me-post-item:last-child {
    border-bottom: none;
  }
  .me-post-title {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: var(--weight-medium, 500);
    line-height: 1.4;
  }
  .me-post-title a {
    color: var(--color-text-primary);
    text-decoration: none;
  }
  .me-post-title a:hover {
    color: var(--color-brand);
  }
  .me-post-meta {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 4px;
    font-size: var(--text-xs);
    color: var(--color-text-tertiary);
  }
  .me-post-board {
    color: var(--color-brand);
    font-weight: 500;
  }

  /* ── 分组 cell 列表：iOS 设置式行（图标 + 名称 + 值 + chevron）──────── */
  .me-cell-list {
    display: flex;
    flex-direction: column;
    padding: 0 !important;
  }
  .me-cell {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 46px;
    padding: 0 14px;
    color: var(--color-text-primary);
    font-size: var(--text-sm);
    text-decoration: none;
    transition: background 0.15s ease;
  }
  .me-cell + .me-cell {
    border-top: 1px solid var(--color-border-muted, var(--color-border));
  }
  .me-cell:hover {
    background: var(--color-bg-subtle);
  }
  .me-cell-icon {
    display: inline-flex;
    color: var(--color-text-secondary);
  }
  .me-cell-label {
    flex: 1;
    min-width: 0;
  }
  .me-cell-value {
    font-size: var(--text-xs);
    color: var(--color-text-tertiary);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .me-cell-chevron {
    color: var(--color-text-tertiary);
    flex: 0 0 auto;
  }

  /* 个人信息卡片（参考信息小卡片） */
  .me-profile-card {
    position: relative;
    overflow: hidden;
    background: var(--color-bg-card);
    border: var(--border-default);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-sm);
  }
  .me-coverwrap {
    position: relative;
  }
  :global(.me-cover) {
    height: 140px;
    min-height: 140px;
    background:
      radial-gradient(110% 150% at 90% -20%, color-mix(in srgb, var(--color-brand) 22%, transparent), transparent 55%),
      linear-gradient(135deg, var(--color-bg-inset), var(--color-brand-soft));
    background-position: center;
    background-size: cover;
    border-bottom: 1px solid var(--color-border);
  }
  .me-cover-level {
    position: absolute;
    top: var(--space-3);
    left: var(--space-3);
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    box-shadow: var(--shadow-sm);
    z-index: 2;
  }
  .me-head {
    position: absolute;
    left: var(--space-4);
    right: var(--space-4);
    bottom: -22px;
    display: flex;
    align-items: flex-end;
    gap: var(--space-3);
    margin-top: 0;
    z-index: 3;
    pointer-events: none;
  }
  .me-head > * {
    pointer-events: auto;
  }
  .me-avatar :global(.avatar) {
    border: 3px solid var(--color-bg-card);
    box-shadow: var(--shadow-sm);
  }
  .me-identity {
    flex: 1;
    min-width: 0;
    margin-bottom: 24px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .me-name {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: var(--weight-bold);
    line-height: 1.2;
    color: var(--color-text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  :global(html.dark) .me-name {
    text-shadow: 0 1px 4px var(--color-overlay);
  }
  .me-handle {
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
    line-height: 1.2;
  }
  :global(html.dark) .me-handle {
    text-shadow: 0 1px 3px var(--color-overlay);
  }
  .me-body {
    position: relative;
    padding: var(--space-4);
    padding-top: calc(var(--space-3) + 20px);
  }
  .me-body-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    flex-wrap: wrap;
    margin-bottom: var(--space-3);
  }
  .me-badges {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .me-actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-shrink: 0;
  }
  .me-bio {
    margin: 0 0 var(--space-3);
    font-size: var(--text-sm);
    line-height: var(--line-height-relaxed);
    color: var(--color-text-secondary);
    word-break: break-word;
  }
  .me-bio.is-empty {
    color: var(--color-text-tertiary);
    font-style: italic;
  }
  /* 社交统计指标栏（帖子 / 关注者 / 正在关注） */
  .me-social-stats {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    margin-top: var(--space-2);
    padding-top: var(--space-3);
    border-top: 1px solid var(--color-border);
  }
  .me-social-stat {
    display: inline-flex;
    align-items: baseline;
    gap: 4px;
    text-decoration: none;
    color: var(--color-text-secondary);
    transition: color 0.15s ease;
  }
  .me-social-stat:hover {
    color: var(--color-brand);
  }
  .me-social-num {
    font-size: var(--text-base);
    font-weight: var(--weight-bold, 700);
    color: var(--color-text-primary);
    font-variant-numeric: tabular-nums;
  }
  .me-social-lbl {
    font-size: var(--text-xs);
    color: var(--color-text-tertiary);
  }
  @media (max-width: 767px) {
    :global(.me-cover) {
      height: 110px;
      min-height: 110px;
    }
    .me-head {
      left: var(--space-3);
      right: var(--space-3);
      bottom: -18px;
      gap: var(--space-2);
    }
    .me-identity {
      margin-bottom: 20px;
    }
    .me-name {
      font-size: var(--text-lg);
    }
    .me-body {
      padding: var(--space-3);
      padding-top: calc(var(--space-3) + 16px);
    }
  }
</style>
