<script lang="ts">
  // M02-UX-05：/me 个人主页——概览枢纽。2026-09 功能拆分后本页只保留：
  // - 个人资料卡（封面/头像/状态/签名/概览信息条，服务端安全投影，仅渲染
  //   自身账号可见字段，不输出任何会话 token）；
  // - 账号与安全状态卡（两步验证状态 + 设备数，管理入口 → /me/security；
  //   会话撤销/退出全部设备与 TOTP/Passkey 管理已拆至 /me/security 与 /mfa）；
  // - 侧栏：账户卡（B币/签到，GET /activity/summary 失败时整卡隐藏）、
  //   M20-TRUST 信任等级进度卡、快捷入口网格（含全部子页入口）；
  // - 我的处罚区块（GET /me/sanctions，后端端点落地前恒空；有记录时显示
  //   类型/原因/时间 + 去申诉入口）。
  import CosmeticAvatar from '$lib/components/wardrobe/CosmeticAvatar.svelte';
  import CosmeticName from '$lib/components/wardrobe/CosmeticName.svelte';
  import ProfileCover from '$lib/components/ui/ProfileCover.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { formatRelative } from '$lib/utils';
  import type { MePageData } from './+page.server';
  import type { CosmeticDefStyle, PublicPresentationTokens } from '$lib/api/types';
  import PageTitle from '$lib/components/PageTitle.svelte';
  import steamBackgrounds from '$lib/data/steam-profile-backgrounds.json';
  import { normalizeSlot, projectEntitlementTokens } from '$lib/components/wardrobe/tokens';
  import { profileEffectClass, profileEffectStyle } from '$lib/components/wardrobe/profile-effect';

  let { data }: { data: MePageData } = $props();

  const user = $derived(data.user);
  const presentation = $derived(data.presentation ?? null);
  const sessions = $derived(data.sessions);
  const error = $derived(data.error);
  // GAP-FIX 账户卡 / 我的处罚（load 增强数据；缺失时安全降级不渲染）。
  const activity = $derived(data.activity ?? null);
  const sanctions = $derived(data.sanctions ?? []);
  // M20-TRUST 信任等级进度（缺失时安全降级不渲染）。
  const trust = $derived(data.trust ?? null);
  const coinBalance = $derived((activity?.balances ?? []).find((b) => b.currency === 'coin'));

  /** 侧栏图标化快捷入口。 */
  const quickLinks = [
    { href: '/me/wardrobe', icon: 'sparkles', label: '我的装扮' },
    { href: '/me/security', icon: 'shield', label: '安全中心' },
    { href: '/settings', icon: 'settings', label: '账号设置' },
    { href: '/favorites', icon: 'star', label: '我的收藏' },
    { href: '/me/level', icon: 'award', label: '我的等级' },
    { href: '/me/attachments', icon: 'paperclip', label: '我的附件' },
    { href: '/me/balance', icon: 'coins', label: '积分明细' },
    { href: '/messages', icon: 'mail', label: '私信' },
    { href: '/apikeys', icon: 'key', label: 'API 密钥' },
    { href: '/me/billing', icon: 'download', label: '下载账单' }
  ] as const;

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
          <div class="me-actions" style="display:flex;align-items:center;gap:var(--space-2);flex-wrap:wrap;">
            <Button text="编辑资料" variant="secondary" size="sm" icon="edit-3" href="/settings" />
            <Button text="我的装扮" variant="ghost" size="sm" icon="sparkles" href="/me/wardrobe" />
          </div>
        </div>

        {#if user.signature}
          <p class="me-bio">{user.signature}</p>
        {:else}
          <p class="me-bio is-empty">暂无个性签名</p>
        {/if}

        <div class="me-info-strip">
          <div class="me-info-item">
            <span class="me-info-label">用户名</span>
            <span class="me-info-value">@{user.username}</span>
          </div>
          <div class="me-info-item">
            <span class="me-info-label">账号状态</span>
            <span class="me-info-value">
              <span class="badge {statusBadge(user.status)}">{statusLabel[user.status] ?? user.status}</span>
            </span>
          </div>
          <div class="me-info-item">
            <span class="me-info-label">角色</span>
            <span class="me-info-value">
              {#if user.roles.length > 0}
                <span class="badge badge-role-admin">{roleLabel(user.roles[0])}</span>
              {:else}
                <span class="badge badge-neutral">成员</span>
              {/if}
            </span>
          </div>
          {#if activity}
            <div class="me-info-item">
              <span class="me-info-label">B币</span>
              <span class="me-info-value"><strong>{coinBalance ? coinBalance.amount : 0}</strong></span>
            </div>
            <div class="me-info-item">
              <span class="me-info-label">签到</span>
              <span class="me-info-value"><strong>{activity.streak_days}</strong> 天</span>
            </div>
          {/if}
          <div class="me-info-item">
            <span class="me-info-label">登录设备</span>
            <span class="me-info-value"><strong>{sessions.length}</strong> 台</span>
          </div>
        </div>
      </div>
    </section>

    <!-- 快捷导航（紧贴资料卡下方，简洁行内链接；设备与两步验证归入安全中心） -->
    <nav class="me-nav" aria-label="快捷导航">
      <a href="/me/security"><Icon name="shield" size={14} />安全中心</a>
      <a href="/settings"><Icon name="settings" size={14} />账号设置</a>
      <a href="/settings#settings-notifications"><Icon name="bell" size={14} />通知设置</a>
      <a href="/settings#settings-oauth"><Icon name="key" size={14} />OAuth 授权</a>
    </nav>

    <div class="content-grid" style="margin-top:var(--space-4);">
      <div class="main-col">
        <!-- 账号与安全状态：概览计数 + 唯一管理入口（/me/security） -->
        <div class="card" id="security">
          <div class="card-header">
            <span class="card-title">账号与安全</span>
            <a class="me-sec-link" href="/me/security">安全中心<Icon name="chevron-right" size={14} /></a>
          </div>
          <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-2);">
            <div class="me-sec-strip">
              <span class="me-sec-item">
                <Icon name="shield" size={14} />
                <span>两步验证</span>
                <span class="badge {user.mfa_enabled ? 'badge-success' : 'badge-neutral'}">
                  {user.mfa_enabled ? '已启用' : '未启用'}
                </span>
              </span>
              <span class="me-sec-item">
                <Icon name="smartphone" size={14} />
                <span>登录设备 <strong>{sessions.length}</strong> 台</span>
              </span>
            </div>
            <p class="me-sec-hint">登录密码、两步验证（TOTP/Passkey）与在线设备管理都在安全中心。</p>
          </div>
        </div>
      </div>
      <div class="side-col">
        <!-- 账户卡：资产 / 签到 + 快捷操作（等级体系已统一为信任等级 TL0–TL4） -->
        {#if activity}
          <div class="card">
            <div class="card-header">
              <span class="card-title">账户与资产</span>
              <span class="badge badge-level">TL{trust?.level ?? user.level ?? 0}</span>
            </div>
            <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
              <div style="display:flex;justify-content:space-between;align-items:baseline;">
                <span class="text-secondary" style="font-size:var(--text-sm);">B币余额</span>
                <strong style="font-variant-numeric:tabular-nums;">{coinBalance ? coinBalance.amount : '—'}</strong>
              </div>
              <div style="display:flex;justify-content:space-between;align-items:baseline;">
                <span class="text-secondary" style="font-size:var(--text-sm);">连续签到</span>
                <span style="font-variant-numeric:tabular-nums;">{activity.streak_days} 天{activity.checked_in_today ? '（今日已签）' : ''}</span>
              </div>
              <div style="display:flex;flex-direction:column;gap:var(--space-2);margin-top:var(--space-1);padding-top:var(--space-3);border-top:var(--border-default);">
                <Button text="发布新帖" variant="primary" size="sm" icon="pen-line" href="/editor" />
                <a class="btn btn-secondary btn-sm" href="/me/balance" style="text-align:center;">签到 / 积分明细</a>
                <a class="btn btn-secondary btn-sm" href="/me/level" style="text-align:center;">社区信任等级中心</a>
              </div>
            </div>
          </div>
        {/if}

        <!-- M20-TRUST 信任等级卡：当前等级 + 下一级逐项进度（LinuxDo 式 TL0–TL4） -->
        {#if trust}
          <div class="card">
            <div class="card-header">
              <span class="card-title">信任等级</span>
              <span class="badge badge-level">TL{trust.level} · {trust.name}</span>
            </div>
            <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
              {#if trust.grace_until}
                <p class="input-hint" style="margin:0;">
                  TL3 考核宽限期至 {new Date(trust.grace_until).toLocaleDateString()}，期间不降级。
                </p>
              {/if}
              {#if trust.next_level}
                {@const next = trust.next_level}
                {#if next.manual_only}
                  <p class="input-hint" style="margin:0;">
                    TL{next.level}（{next.name}）仅可由工作人员手动授予。
                  </p>
                {:else}
                  <p class="input-hint" style="margin:0;">
                    距 TL{next.level}（{next.name}）
                    {next.eligible ? '条件已全部满足，待系统晋升。' : '：'}
                  </p>
                  <ul style="list-style:none;margin:0;padding:0;display:flex;flex-direction:column;gap:var(--space-1);">
                    {#each next.requirements as req (req.key)}
                      <li style="display:flex;justify-content:space-between;gap:var(--space-2);font-size:var(--text-sm);">
                        <span style={req.met ? 'color:var(--color-success);' : 'color:var(--color-danger);'}>
                          {req.met ? '✓' : '·'} {req.label}
                        </span>
                        <span style="font-variant-numeric:tabular-nums;white-space:nowrap;" class="text-secondary">
                          {req.current} / {req.required}
                        </span>
                      </li>
                    {/each}
                  </ul>
                {/if}
              {/if}
              {#if trust.summary}
                <p class="input-hint" style="margin:0;">{trust.summary}</p>
              {/if}
            </div>
          </div>
        {/if}

        <!-- 快捷入口 -->
        <div class="card">
          <div class="card-header"><span class="card-title">快捷入口</span></div>
          <div class="card-body">
            <div class="quick-grid">
              {#each quickLinks as link (link.href)}
                <a href={link.href} class="quick-link">
                  <Icon name={link.icon} size={15} />
                  <span>{link.label}</span>
                </a>
              {/each}
              <a href="/users/{encodeURIComponent(user.username)}?tab=posts" class="quick-link">
                <Icon name="list" size={15} />
                <span>我的帖子</span>
              </a>
            </div>
          </div>
        </div>
      </div>
    </div>

    {#if sanctions.length > 0}
      <!-- GAP-FIX 我的处罚：load 取 GET /me/sanctions；空列表时不渲染此卡。 -->
      <div class="card" style="margin-top:var(--space-5);border-color:var(--color-warning);">
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
  /* 侧栏图标化快捷入口 */
  .quick-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-2);
  }
  .quick-link {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 10px 11px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg-card);
    color: var(--color-text-secondary);
    font-size: var(--text-sm);
    text-decoration: none;
    transition: border-color 0.15s ease, color 0.15s ease, background 0.15s ease;
  }
  .quick-link:hover {
    border-color: var(--color-brand);
    color: var(--color-brand);
    background: var(--color-bg-subtle);
  }
  /* 快捷导航条：紧贴资料卡下方，行内图标链接 */
  .me-nav {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    margin-top: var(--space-3);
    padding: var(--space-2) 0;
    flex-wrap: wrap;
  }
  .me-nav a {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 6px 12px;
    border-radius: var(--radius-sm);
    color: var(--color-text-secondary);
    font-size: var(--text-sm);
    text-decoration: none;
    transition: color 0.15s, background 0.15s;
  }
  .me-nav a:hover {
    color: var(--color-brand);
    background: var(--color-bg-subtle);
  }
  /* 账号与安全状态卡 */
  .me-sec-link {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    font-size: var(--text-sm);
    color: var(--color-brand);
    text-decoration: none;
  }
  .me-sec-link:hover {
    text-decoration: underline;
  }
  .me-sec-strip {
    display: flex;
    align-items: center;
    gap: var(--space-2) var(--space-4);
    flex-wrap: wrap;
  }
  .me-sec-item {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
    color: var(--color-text-primary);
  }
  .me-sec-item strong {
    font-variant-numeric: tabular-nums;
  }
  .me-sec-hint {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--color-text-tertiary);
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
    text-shadow: 0 1px 4px rgba(0, 0, 0, 0.8);
  }
  .me-handle {
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
    line-height: 1.2;
  }
  :global(html.dark) .me-handle {
    text-shadow: 0 1px 3px rgba(0, 0, 0, 0.8);
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
  .me-info-strip {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-4);
    padding: var(--space-2) var(--space-3);
    background: var(--color-bg-subtle);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    font-size: var(--text-xs);
  }
  .me-info-item {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
  }
  .me-info-label {
    color: var(--color-text-tertiary);
  }
  .me-info-value {
    color: var(--color-text-primary);
  }
  .me-info-value strong {
    font-variant-numeric: tabular-nums;
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
    .me-info-strip {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: var(--space-2);
    }
  }
</style>
