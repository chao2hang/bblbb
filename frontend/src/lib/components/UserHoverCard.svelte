<script lang="ts">
  // M03-PROFILE-09 / M03-UI-03：用户 Hover Card —— 只接受并渲染公开投影字段。
  //
  // 隐私契约：本组件是用户信息的浮层卡片，props 类型只允许
  // `PublicProfile` 的公开字段（username/display_name/level/signature），
  // 严禁传入邮箱、状态、凭据等私有字段；组件实现也只渲染这些公开字段。
  // level/signature 允许缺省：列表/搜索行只有部分公开投影
  // （author.username + display_name），缺省字段不渲染而非伪造。
  // （产品约定：只保留签名，不再展示简介字段。）
  // SSR 泄漏测试见 frontend/src/lib/testing/ssr/privacy.test.ts。
  //
  // 布局（对齐「论坛用户悬浮卡片」参考稿，全部复用已有样式）：
  //   cover（等级 chip 左上）→ 头像压缝线 + 名字（头像右侧）+ 关注/私信动作 →
  //   签名 → 佩戴的成就徽章 tags → 统计（帖子/粉丝/关注）→ 页脚（加入时间）。
  //
  // 数据：触发点只带最小公开投影；卡片打开后客户端拉取
  // GET /users/{username}（公开端点）补齐 created_at / 社交统计
  // （post_count/followers/following/is_following，后端 PublicProfile DTO
  // 的 GAP-FIX 公开字段）/ presentation_tokens / equipped_achievements。
  // 拉取失败时回退为触发点数据渲染（缺省行不渲染而非伪造）。
  //
  // 佩戴徽章行 = 用户在成就墙「正在装备」槽的真实成就（社交域·成就，
  // user_achievements.equipped = 1，≤3，服务端裁决 code/name 公开字段），
  // 不再展示商城装扮徽章 Token；运行时仍逐项校验类型，异常数据不渲染。
  //
  // 装扮（M07-SHOP）：`presentation` 只接受服务端编译的 `presentation_tokens`
  // 安全投影；渲染前逐槽位过 wardrobe tokens 白名单，白名单之外一律不渲染。
  // 槽位 → 卡片位置：
  //   profile_effect   → cover 纹理（::before 叠加层）
  //   avatar_frame     → 头像光环（box-shadow 环，不影响布局）
  //   nickname_color   → 名字颜色
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import CosmeticAvatar from '$lib/components/wardrobe/CosmeticAvatar.svelte';
  import CosmeticName from '$lib/components/wardrobe/CosmeticName.svelte';
  import ProfileCover from '$lib/components/ui/ProfileCover.svelte';
  import {
    createConversation,
    followUser,
    getMe,
    getUser,
    newClientRequestId,
    unfollowUser,
    type PublicProfile
  } from '$lib/api/client';
  import { formatCount, formatRelative } from '$lib/utils';
  import { PROFILE_EFFECTS, BADGES, POST_EFFECT_LABELS } from '$lib/components/wardrobe/tokens';
  import { profileEffectClass as liveProfileEffectClass, profileEffectStyle as liveProfileEffectStyle, resolveSteamPanoramaMedia, type ProfileEffectMediaStyle } from '$lib/components/wardrobe/profile-effect';
  import type { CosmeticDefStyle, PublicPresentationTokens } from '$lib/api/types';

  /** 触发卡公开字段（严格 allowlist；level/signature 可缺省）。 */
  export type HoverCardUser = Pick<PublicProfile, 'username' | 'display_name'> &
    Partial<Pick<PublicProfile, 'level' | 'signature' | 'avatar_attachment_id' | 'cover_attachment_id'>>;

  /** 装扮安全投影：只消费服务端编译的 presentation_tokens（白名单渲染）。 */
  export type HoverCardPresentation =
    | PublicPresentationTokens
    | {
        presentation_tokens?: PublicPresentationTokens | Record<string, string | string[] | null> | null;
      }
    | null;

  /** 打开后经公开端点补齐的字段（后端 DTO GAP-FIX 公开社交统计）。 */
  type HoverCardProfile = PublicProfile & {
    post_count?: number | null;
    followers?: number | null;
    following?: number | null;
    is_following?: boolean;
  };

  let {
    user,
    presentation = null,
    overrideTokens = null,
    preview = false
  }: {
    user: HoverCardUser;
    presentation?: HoverCardPresentation;
    overrideTokens?: PublicPresentationTokens | null;
    preview?: boolean;
  } = $props();

  const displayName = $derived(user.display_name || user.username);
  const profileUrl = $derived(`/users/${user.username}`);

  // ── 打开时补齐数据（公开端点；失败回退触发点数据） ──
  let profile = $state<HoverCardProfile | null>(null);
  /** 仅保留 username 用于本人判定；不渲染任何 Me 字段。 */
  let meUsername = $state<string | null>(null);
  let meKnown = $state(false);
  let followBusy = $state(false);
  let followNeedLogin = $state(false);
  let messageBusy = $state(false);

  /** /me 结果按会话缓存（一次页面生命周期最多请求一次）。 */
  let mePromise: Promise<{ username: string } | null> | undefined;

  const level = $derived(user.level ?? profile?.level ?? null);
  const isOwner = $derived(preview || (meKnown && meUsername !== null && meUsername === user.username));
  const isFollowing = $derived(profile?.is_following === true);
  const joinedText = $derived.by(() => {
    const ts = profile?.created_at;
    if (typeof ts !== 'number' || !Number.isFinite(ts) || ts <= 0) return null;
    // 契约 created_at 为 Unix 毫秒；容错秒级时间戳。
    return formatRelative(ts > 1e12 ? Math.round(ts / 1000) : ts);
  });

  // ── 白名单 Token 解析（未知值一律不渲染）；优先服务端最新拉取的投影，并可被 overrideTokens（如商城试穿）覆盖 ──
  const tokens = $derived.by(() => {
    const out: Record<string, string | string[] | null> = {};
    const fetched = profile?.presentation_tokens;
    if (fetched && typeof fetched === 'object') {
      for (const [key, value] of Object.entries(fetched)) {
        if (value !== null && value !== undefined) out[key] = value as string | string[];
      }
    } else if (presentation && 'presentation_tokens' in presentation) {
      const pt = presentation.presentation_tokens;
      if (pt && typeof pt === 'object') {
        for (const [key, value] of Object.entries(pt)) {
          if (value !== null && value !== undefined) out[key] = value as string | string[];
        }
      }
    } else if (presentation) {
      for (const [key, value] of Object.entries(presentation as Record<string, string | string[] | null>)) {
        if (value !== null && value !== undefined) out[key] = value as string | string[];
      }
    }
    if (overrideTokens) {
      for (const [key, value] of Object.entries(overrideTokens)) {
        if (value !== null && value !== undefined) {
          out[key] = value as string | string[];
        }
      }
      if (overrideTokens.profile_effect_style) {
        out.profile_effect_style = overrideTokens.profile_effect_style as unknown as string;
      }
      if (overrideTokens.profile_effect_name) {
        out.profile_effect_name = overrideTokens.profile_effect_name;
      }
    }
    return out;
  });
  const hoverTokens = $derived.by(() => tokens);
  const profileEffectClass = $derived.by(() => {
    const v = tokens['profile_effect'];
    return typeof v === 'string' && v in PROFILE_EFFECTS ? PROFILE_EFFECTS[v] : null;
  });

  const effectDefStyle = $derived((overrideTokens?.profile_effect_style ?? tokens.profile_effect_style) as ProfileEffectMediaStyle | undefined);
  const profileEffectToken = $derived(typeof tokens.profile_effect === 'string' ? tokens.profile_effect : null);
  const profileEffectName = $derived(typeof tokens.profile_effect_name === 'string' ? tokens.profile_effect_name : null);
  const coverMedia = $derived(resolveSteamPanoramaMedia(effectDefStyle, profileEffectToken, profileEffectName));

  const bgVideoWebm = $derived(coverMedia?.webm ?? null);

  const bgVideoMp4 = $derived(coverMedia?.mp4 ?? null);

  const bgImageSrc = $derived(coverMedia?.image ?? null);

  const bgFallbackSrc = $derived(coverMedia?.fallbackSrc ?? null);

  /** 自定义主页装饰只接受 DTO 中的结构化颜色/纹理枚举，不解释任意 CSS。 */
  const profileEffectStyle = $derived.by(() => {
    const style = tokens.profile_effect_style as CosmeticDefStyle | undefined;
    if (!style || style.mode !== 'profile') return '';
    const color = (value: unknown): string | null =>
      typeof value === 'string' && /^#[0-9a-fA-F]{6}$/.test(value) ? value : null;
    const base = color(style.baseColor);
    const accent = color(style.accentColor);
    if (!base || !accent) return '';
    const texture = style.texture === 'dark_stars'
      ? `radial-gradient(circle at 30% 40%, ${accent}99, transparent 55%), radial-gradient(circle at 70% 65%, ${accent}66, transparent 58%)`
      : style.texture === 'grid'
        ? `linear-gradient(${accent}33 1px, transparent 1px), linear-gradient(90deg, ${accent}33 1px, transparent 1px)`
        : style.texture === 'dots'
          ? `radial-gradient(${accent}66 1px, transparent 1px)`
          : `radial-gradient(circle at 28% 30%, ${accent}88, transparent 55%), radial-gradient(circle at 72% 72%, ${accent}55, transparent 60%)`;
    const size = style.texture === 'grid' ? '24px 24px' : style.texture === 'dots' ? '18px 18px' : 'auto';
    return `background:${texture},${base};background-size:${size};`;
  });
  // 佩戴徽章 = 成就墙装备槽的真实成就（服务端已裁决 ≤3；运行时仍逐项
  // 校验 code/name 为非空字符串，异常条目不渲染，恒截断到 3 枚）。
  const coverMotionClass = $derived(liveProfileEffectClass(effectDefStyle, typeof tokens.profile_effect === 'string' ? tokens.profile_effect : null));
  const coverMotionStyle = $derived(liveProfileEffectStyle(effectDefStyle));

  const equippedAchievements = $derived.by(() => {
    const items = profile?.equipped_achievements;
    if (!Array.isArray(items)) return [];
    return items
      .filter(
        (a): a is { code: string; name: string } =>
          !!a &&
          typeof a === 'object' &&
          typeof a.code === 'string' &&
          a.code.length > 0 &&
          typeof a.name === 'string' &&
          a.name.length > 0
      )
      .slice(0, 3);
  });

  const previewBadges = $derived.by(() => {
    const raw = tokens['profile_badges'];
    if (!Array.isArray(raw)) return [];
    const names = Array.isArray(tokens.profile_badge_names) ? tokens.profile_badge_names : [];
    return raw
      .filter((b): b is string => typeof b === 'string')
      .map((b, index) => ({
        code: b,
        name: `${BADGES[b]?.icon ?? '✦'} ${names[index] ?? BADGES[b]?.label ?? b}`,
        isBadge: true
      }));
  });

  const displayBadges = $derived.by(() => {
    const combined: Array<{ code: string; name: string; isBadge?: boolean }> = [...equippedAchievements];
    const seen = new Set(combined.map((a) => a.code));
    for (const b of previewBadges) {
      if (!seen.has(b.code)) {
        combined.push(b);
        seen.add(b.code);
      }
    }
    return combined.slice(0, 3);
  });

  $effect(() => {
    if (!browser) return;
    const ac = new AbortController();
    getUser(fetch, user.username)
      .then((p) => {
        if (!ac.signal.aborted) profile = p as HoverCardProfile;
      })
      .catch(() => {
        /* 公开资料拉取失败 → 保持触发点数据渲染（缺省行不渲染） */
      });
    mePromise ??= getMe(fetch).then((m) => (m ? { username: m.username } : null));
    mePromise
      .then((m) => {
        if (!ac.signal.aborted) {
          meUsername = m?.username ?? null;
          meKnown = true;
        }
      })
      .catch(() => {
        if (!ac.signal.aborted) meKnown = true;
      });
    return () => ac.abort();
  });

  async function toggleFollow() {
    if (followBusy || !profile) return;
    followBusy = true;
    try {
      const res = isFollowing
        ? await unfollowUser(fetch, user.username)
        : await followUser(fetch, user.username);
      profile = { ...profile, is_following: res.following, followers: res.followers };
    } catch (e) {
      // 401（未登录/会话过期）→ 按钮退化为登录引导链接（与用户页一致）。
      if ((e as { status?: number } | null)?.status === 401) followNeedLogin = true;
    } finally {
      followBusy = false;
    }
  }

  async function startMessage() {
    if (messageBusy) return;
    messageBusy = true;
    try {
      const conv = await createConversation(fetch, user.username, newClientRequestId());
      await goto(`/messages?c=${encodeURIComponent(conv.id)}`);
    } catch {
      // 开会话失败（登录过期等）→ 退回会话列表页兜底。
      await goto('/messages');
    } finally {
      messageBusy = false;
    }
  }

  /** 统计 → 对应页面：帖子 → 主页内容 tab；粉丝/关注 → 关系列表页。
   *  preview（商城试穿）与名字一致保持非链接，不做导航。 */
  const postsStatUrl = $derived(`${profileUrl}?tab=posts`);
  const followersStatUrl = $derived(`${profileUrl}/followers`);
  const followingStatUrl = $derived(`${profileUrl}/following`);
</script>

<div
  class="user-hover-card"
  role={preview ? 'region' : 'dialog'}
  aria-label={preview ? `${displayName} 的装扮预览卡片` : `${displayName} 的个人资料`}
>
  <div class="user-hover-coverwrap">
    <ProfileCover
      attachmentId={bgImageSrc || bgVideoWebm || bgVideoMp4 ? null : (profile?.cover_attachment_id ?? (user && 'cover_attachment_id' in user ? (user.cover_attachment_id as string | null) : null))}
      src={bgImageSrc}
      fallbackSrc={bgFallbackSrc}
      videoWebm={bgVideoWebm}
      videoMp4={bgVideoMp4}
      class="user-hover-cover{profileEffectClass ? ` ${profileEffectClass}` : ''}{coverMotionClass ? ` ${coverMotionClass}` : ''}"
      style={bgImageSrc || bgVideoWebm || bgVideoMp4 ? 'background: #000;' : [profileEffectStyle, coverMotionStyle].filter(Boolean).join(';')}
    />
    {#if level !== null}
      <!-- 等级 chip：cover 左上（参考稿），绝对定位不占布局 -->
      <span class="badge badge-level user-hover-level">TL{level}</span>
    {/if}
    <div class="user-hover-head">
      <div class="user-hover-avatar">
        <CosmeticAvatar
          name={displayName}
          size="xl"
          presentation={{ presentation_tokens: hoverTokens as PublicPresentationTokens }}
          avatarAttachmentId={profile?.avatar_attachment_id ?? (user && 'avatar_attachment_id' in user ? (user.avatar_attachment_id as string | null) : null)}
          seed={user?.username ?? profile?.id ?? displayName}
        />
      </div>
      {#if preview}
        <span class="user-hover-name">
          <CosmeticName name={displayName} presentation={{ presentation_tokens: hoverTokens as PublicPresentationTokens }} />
        </span>
      {:else}
        <a class="user-hover-name" href={profileUrl}>
          <CosmeticName name={displayName} presentation={{ presentation_tokens: hoverTokens as PublicPresentationTokens }} />
        </a>
      {/if}
    </div>
  </div>
  <div class="user-hover-body">
    {#if meKnown && !isOwner}
      <div class="user-hover-head-actions">
        {#if meUsername === null || followNeedLogin}
          <!-- 匿名/会话过期：关注是登录操作，给登录引导（同用户页模式） -->
          <a class="btn primary sm" href="/login?next={encodeURIComponent(profileUrl)}"
            >登录后关注</a
          >
        {:else}
          <button
            type="button"
            class="btn {isFollowing ? 'ghost' : 'primary'} sm"
            onclick={toggleFollow}
            disabled={followBusy}
          >
            {isFollowing ? '已关注 · 取消' : '+ 关注'}
          </button>
          <button
            type="button"
            class="btn secondary sm"
            onclick={startMessage}
            disabled={messageBusy}
          >
            私信
          </button>
        {/if}
      </div>
    {/if}
    {#if user.signature || profile?.signature}
      <p class="user-hover-bio">{profile?.signature ?? user.signature}</p>
    {:else if preview}
      <p class="user-hover-bio" style="color:var(--color-text-tertiary);font-style:italic;">装扮试穿展示</p>
    {/if}
    {#if displayBadges.length > 0}
      <!-- 佩戴徽章：成就墙装备槽的真实成就（≤3，服务端裁决）或装扮徽章 -->
      <div class="user-hover-tags" aria-label="佩戴的成就徽章">
        {#each displayBadges as a (a.code)}
          <span class="tag">{#if !a.isBadge}🏅 {/if}{a.name}</span>
        {/each}
      </div>
    {/if}
    {#if tokens['post_effect']}
      <div class="user-hover-tags" style="margin-top:var(--space-2);" aria-label="帖子装饰">
        <span class="tag" style="border:1px dashed var(--color-brand);color:var(--color-brand);">
          ✨ 帖子装饰：{POST_EFFECT_LABELS[String(tokens['post_effect'])] ?? tokens['post_effect']}
        </span>
      </div>
    {/if}
    {#if profile && typeof profile.post_count === 'number'}
      <div class="user-hover-stats">
        {#if preview}
          <div class="user-hover-stat">
            <strong>{formatCount(profile.post_count)}</strong><span>帖子</span>
          </div>
        {:else}
          <a class="user-hover-stat" href={postsStatUrl} title={`查看 ${displayName} 的帖子`}>
            <strong>{formatCount(profile.post_count)}</strong><span>帖子</span>
          </a>
        {/if}
        {#if typeof profile.followers === 'number'}
          {#if preview}
            <div class="user-hover-stat">
              <strong>{formatCount(profile.followers)}</strong><span>粉丝</span>
            </div>
          {:else}
            <a class="user-hover-stat" href={followersStatUrl} title={`查看 ${displayName} 的粉丝`}>
              <strong>{formatCount(profile.followers)}</strong><span>粉丝</span>
            </a>
          {/if}
        {/if}
        {#if typeof profile.following === 'number'}
          {#if preview}
            <div class="user-hover-stat">
              <strong>{formatCount(profile.following)}</strong><span>关注</span>
            </div>
          {:else}
            <a class="user-hover-stat" href={followingStatUrl} title={`查看 ${displayName} 正在关注的人`}>
              <strong>{formatCount(profile.following)}</strong><span>关注</span>
            </a>
          {/if}
        {/if}
      </div>
    {/if}
    {#if preview}
      <div class="user-hover-footer">
        <span class="user-hover-joined" style="color:var(--color-brand);font-weight:600;">✦ 装扮试穿中</span>
      </div>
    {:else if joinedText}
      <div class="user-hover-footer">
        <span class="user-hover-joined">加入于 {joinedText}</span>
      </div>
    {/if}
  </div>
</div>
