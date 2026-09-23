<script lang="ts">
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import CosmeticAvatar from '$lib/components/wardrobe/CosmeticAvatar.svelte';
  import CosmeticName from '$lib/components/wardrobe/CosmeticName.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import type { PublicPresentationTokens } from '$lib/api/types';
  import UserCard from '$lib/components/UserCard.svelte';
  import { isPostRead } from '$lib/readState.svelte';
  import { formatCount, formatRelative } from '$lib/utils';
  import { boardVisuals } from '$lib/board-visuals';

  export interface ParticipantPreview {
    id?: string;
    username?: string | null;
    display_name?: string | null;
    avatar_attachment_id?: string | null;
    presentation_tokens?: PublicPresentationTokens | null;
  }

  let {
    id,
    title,
    author,
    authorUsername = null,
    authorPresentation = null,
    authorAvatarAttachmentId = null,
    boardLabel = null,
    boardSlug = null,
    boardIcon = null,
    participants = [],
    replyCount = 0,
    viewCount = 0,
    likeCount = 0,
    createdAt,
    lastReplyAt = null,
    pinned = false,
    featured = false,
    reasonBadge = null
  }: {
    id: string;
    title: string;
    /** 作者展示名（昵称优先；与 participantList 的 label 同源）。 */
    author: string;
    /** 作者账号用户名（悬浮资料卡/主页链接；缺省时作者头像不出卡）。 */
    authorUsername?: string | null;
    /** 作者装扮安全投影（服务端 presentation_tokens；已装备头像框随楼主
     *  头像渲染。缺省/未携带时不渲染任何装扮，与悬浮资料卡同源）。 */
    authorPresentation?: PublicPresentationTokens | null;
    /** 作者上传头像附件 id（公开引用；缺省回退首字母占位头像）。 */
    authorAvatarAttachmentId?: string | null;
    boardLabel?: string | null;
    /** 板块 slug（身份色/图标 slug 映射的解析键；未传回退默认视觉）。 */
    boardSlug?: string | null;
    /** 持久化板块图标（boards.icon，管理端设置；null/未知名由
     *  boardVisuals 回退 slug 映射 → 默认图标，与侧栏分类卡同源）。 */
    boardIcon?: string | null;
    /** Real participant preview; the topic author is used when omitted. */
    participants?: ParticipantPreview[];
    replyCount?: number;
    viewCount?: number;
    likeCount?: number;
    createdAt: number;
    lastReplyAt?: number | null;
    pinned?: boolean;
    featured?: boolean;
    /** 算法推荐理由（仅推荐流传入；普通列表不显示）。 */
    reasonBadge?: string | null;
  } = $props();

  const href = $derived('/posts/' + encodeURIComponent(id));
  /** 板块身份视觉（图标 + 身份色）：持久化 boards.icon → slug 映射 → 默认，
   *  与左栏分类卡（BoardNav）/列表 category-badge 同一解析函数。 */
  const boardVisual = $derived(boardVisuals(boardSlug ?? '', boardIcon));
  /** 参与者列单元格：label=展示名（昵称优先，头像与无障碍标签用）；
   *  account=账号用户名（悬浮资料卡与主页链接用；为 null 时不出卡，
   *  如已注销用户或未传用户名投影的行）；avatarAttachmentId=上传头像
   *  附件引用（缺省回退首字母占位头像）。 */
  interface ParticipantCell {
    id?: string;
    label: string;
    account: string | null;
    avatarAttachmentId: string | null;
    presentation: PublicPresentationTokens | null;
  }
  /** 参与者列：楼主 + 真实参与者（后端 participants 投影），去重后最多 5 个。
   *  展示名优先昵称（display_name），缺省回退 username；两者皆空的行剔除
   *  （如仅剩 id 的已注销用户），避免渲染「?」占位头像。
   *  若超过 5 个参与者，在最后头像后显示省略符号。 */
  const deduplicatedParticipants = $derived.by(() => {
    const source: ParticipantPreview[] = [
      {
        username: authorUsername ?? null,
        display_name: author,
        avatar_attachment_id: authorAvatarAttachmentId,
        presentation_tokens: authorPresentation
      },
      ...participants
    ];
    const seen = new Set<string>();
    return source
      .map((participant): ParticipantCell => {
        const display = participant.display_name?.trim() || '';
        const account = participant.username?.trim() || null;
        return {
          id: participant.id,
          label: display || account || '',
          account,
          avatarAttachmentId: participant.avatar_attachment_id ?? null,
          presentation: participant.presentation_tokens ?? null
        };
      })
      .filter((participant) => participant.label.length > 0)
      .filter((participant) => {
        const keys = [
          participant.id?.trim(),
          participant.account?.toLowerCase(),
          participant.label.toLowerCase()
        ].filter((key): key is string => Boolean(key));
        if (keys.some((key) => seen.has(key))) return false;
        keys.forEach((key) => seen.add(key));
        return true;
      });
  });

  const participantList = $derived(deduplicatedParticipants.slice(0, 5));
  const hasMoreParticipants = $derived(deduplicatedParticipants.length > 5);

  const participantLabel = $derived.by(() => {
    if (participants.length === 0) {
      return '作者：' + author;
    }
    const names = participantList.map((participant) => participant.label).join('、');
    return hasMoreParticipants ? `参与者：${names} 等` : `参与者：${names}`;
  });
  const activityAt = $derived(lastReplyAt ?? createdAt);
  const activityLevel = $derived(calculateActivity(replyCount, likeCount, activityAt));
  // 已读状态（本地浏览记录）：左侧色条「未读亮 / 已读暗」的数据来源
  const isRead = $derived(isPostRead(id));

  function toIsoTime(timestamp: number): string {
    const milliseconds = timestamp > 1e11 ? timestamp : timestamp * 1000;
    return new Date(milliseconds).toISOString();
  }

  /** A compact, data-backed signal for the row activity spine. */
  function calculateActivity(replies: number, likes: number, timestamp: number): number {
    const milliseconds = timestamp > 1e11 ? timestamp : timestamp * 1000;
    const ageHours = Math.max(0, (Date.now() - milliseconds) / 3_600_000);
    const ageBucket = Math.floor(ageHours / 6);
    const recency = Math.max(0, 1 - ageBucket / 28);
    const engagement = Math.min(1, (Math.max(0, replies) * 2 + Math.max(0, likes)) / 40);
    return Math.round(Math.min(1, Math.max(0.08, engagement * 0.68 + recency * 0.32)) * 100) / 100;
  }
</script>

<article
  class="topic-row"
  class:is-read={isRead}
  data-activity-level={activityLevel}
>
  <div class="topic-row__main">
    <div class="topic-row__title-line">
      {#if pinned}<span class="topic-row__status topic-row__status--pinned">置顶</span>{/if}
      {#if featured}<span class="topic-row__status topic-row__status--featured">精华</span>{/if}
      <a class="topic-row__title" {href} aria-label="查看帖子：{title}">{title}</a>
    </div>
    <div class="topic-row__meta">
      {#if boardLabel}
        <span class="topic-row__board" title="板块：{boardLabel}">
          <span class="topic-row__board-mark" style="color:{boardVisual.color};" aria-hidden="true">
            <Icon name={boardVisual.icon} size={12} />
          </span>
          <span>{boardLabel}</span>
        </span>
      {/if}
      <span class="topic-row__author">
        {#if authorPresentation}
          <CosmeticName name={author} presentation={authorPresentation} />
        {:else}
          {author}
        {/if}
      </span>
      <span class="topic-row__separator" aria-hidden="true">·</span>
      <time datetime={toIsoTime(createdAt)}>{formatRelative(createdAt)}</time>
      {#if likeCount > 0}
        <span class="topic-row__likes" aria-label="{formatCount(likeCount)} 人点赞">
          <Icon name="heart" size={12} />
          {formatCount(likeCount)}
        </span>
      {/if}
      {#if reasonBadge}
        <span class="topic-row__reason-badge" title="推荐理由：{reasonBadge}">{reasonBadge}</span>
      {/if}
    </div>
  </div>

  <div class="topic-row__participants" aria-label={participantLabel}>
    {#each participantList as participant (participant.id ?? participant.label)}
      <span class="topic-row__avatar" title={participant.label}>
        {#if participant.account}
          <!-- 头像即触发链接：hover/focus 出公开资料悬浮卡（UserCard portal），
               窄屏点击出底部卡；无账号用户名（匿名/已注销）保持普通头像。 -->
          <UserCard
            user={{ username: participant.account, display_name: participant.label }}
            label="查看 {participant.label} 的个人资料"
            presentation={participant.presentation}
          >
            <CosmeticAvatar
              name={participant.label}
              size="sm"
              presentation={participant.presentation}
              avatarAttachmentId={participant.avatarAttachmentId}
              seed={participant.account ?? participant.id ?? participant.label}
              class="topic-row__participant-avatar"
            />
          </UserCard>
        {:else}
          <CosmeticAvatar
            name={participant.label}
            size="sm"
            presentation={participant.presentation}
            avatarAttachmentId={participant.avatarAttachmentId}
            seed={participant.account ?? participant.id ?? participant.label}
            class="topic-row__participant-avatar"
          />
        {/if}
      </span>
    {/each}
    {#if hasMoreParticipants}
      <span class="topic-row__more topic-row__ellipsis" title="更多参与者" aria-hidden="true">…</span>
    {/if}
  </div>

  <div class="topic-row__metric topic-row__replies" aria-label="{formatCount(replyCount)} 回复">
    <strong class:is-hot={replyCount >= 20}>{formatCount(replyCount)}</strong>
  </div>

  <div class="topic-row__metric topic-row__views" aria-label="{formatCount(viewCount)} 浏览量">
    <strong>{formatCount(viewCount)}</strong>
  </div>

  <div class="topic-row__metric topic-row__activity" aria-label="最近活动：{formatRelative(activityAt)}">
    <time datetime={toIsoTime(activityAt)}>{formatRelative(activityAt)}</time>
  </div>

  <div class="topic-row__mobile-stats" aria-label="帖子数据">
    <span>回复 {formatCount(replyCount)}</span>
    <span>浏览 {formatCount(viewCount)}</span>
    <span>活动 {formatRelative(activityAt)}</span>
  </div>

  <!-- 原型对齐（移动端卡片页脚）：板块 chip + ♥赞 + 💬回复，替代上方旧统计行 -->
  <div class="topic-row__mobile-footer">
    {#if boardLabel}
      <span class="topic-row__footer-chip">
        <span class="topic-row__board-mark" style="color:{boardVisual.color};" aria-hidden="true">
          <Icon name={boardVisual.icon} size={11} />
        </span>
        {boardLabel}
      </span>
    {/if}
    <span class="topic-row__footer-stat" aria-label="{formatCount(likeCount)} 人点赞">
      <Icon name="heart" size={13} />
      {formatCount(likeCount)}
    </span>
    <span class="topic-row__footer-stat" aria-label="{formatCount(replyCount)} 条回复">
      <Icon name="message-circle" size={13} />
      {formatCount(replyCount)}
    </span>
  </div>
</article>

<style>
  .topic-row {
    position: relative;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 112px 64px 72px 88px;
    gap: var(--space-3);
    align-items: center;
    min-width: 0;
    min-height: 68px;
    padding: var(--space-2) var(--space-4) var(--space-2) 18px;
    border-bottom: 1px solid var(--color-border-muted);
    background: transparent;
    transition: background-color var(--duration-fast) var(--ease-out);
  }

  /* 左侧 2px 色条：浏览状态信号（未读亮 / 已读暗），替代原活跃度透明度编码 */
  .topic-row::before {
    position: absolute;
    inset: 0 auto 0 0;
    width: 2px;
    content: '';
    background: var(--color-brand);
    opacity: 0.9;
    pointer-events: none;
  }

  /* 已读：色条压暗一档（主题 token 驱动，日夜模式一致成立） */
  .topic-row.is-read::before {
    opacity: 0.22;
  }

  .topic-row:last-child {
    border-bottom: 0;
  }

  .topic-row:hover {
    background: var(--color-surface-hover);
  }

  .topic-row__main,
  .topic-row__title-line,
  .topic-row__meta {
    min-width: 0;
  }

  .topic-row__title-line {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .topic-row__title {
    display: -webkit-box;
    min-width: 0;
    overflow: hidden;
    color: var(--color-text-primary);
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
    line-height: 1.35;
    letter-spacing: -0.018em;
    text-decoration: none;
    text-overflow: ellipsis;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }

  .topic-row__title:hover {
    color: var(--color-brand);
  }

  .topic-row__title:focus-visible {
    border-radius: var(--radius-sm);
    outline: 2px solid var(--color-brand);
    outline-offset: 3px;
  }

  .topic-row__status {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    min-height: 20px;
    padding: 1px 6px;
    border: 1px solid currentColor;
    border-radius: var(--radius-sm);
    font: 600 10px/1 var(--font-family-mono);
    letter-spacing: var(--label-letter-spacing);
    white-space: nowrap;
  }

  .topic-row__status--pinned {
    color: var(--color-success);
  }

  .topic-row__status--featured {
    color: var(--color-warning);
  }

  .topic-row__meta {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-top: 5px;
    overflow: hidden;
    color: var(--color-text-tertiary);
    font: var(--text-xs)/1.35 var(--font-family-mono);
    font-variant-numeric: tabular-nums;
    letter-spacing: var(--meta-letter-spacing);
    white-space: nowrap;
  }

  .topic-row__board {
    display: inline-flex;
    min-width: 0;
    align-items: center;
    gap: 5px;
    max-width: min(260px, 42%);
    overflow: hidden;
    color: var(--color-text-secondary);
    text-overflow: ellipsis;
  }

  .topic-row__board > span:last-child {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* 板块身份图标（boards.icon → slug 映射 → 默认；颜色 = 板块身份色，
     与左栏分类卡同源）。容器自身为 flex：svg 脱离文本基线排版，与 CJK
     文字几何居中（inline span 直包 svg 会因基线降部空隙整体偏高）。 */
  .topic-row__board-mark {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    line-height: 0;
  }

  .topic-row__author {
    overflow: hidden;
    max-width: 180px;
    color: var(--color-text-secondary);
    text-overflow: ellipsis;
  }

  .topic-row__separator {
    color: var(--color-text-tertiary);
  }

  .topic-row__likes {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    flex: 0 0 auto;
    color: var(--color-text-tertiary);
  }

  /* 算法推荐理由徽标（仅推荐流传入时渲染） */
  .topic-row__reason-badge {
    flex: 0 0 auto;
    padding: 1px 6px;
    border-radius: var(--radius-sm, 2px);
    background: var(--color-brand-soft);
    color: var(--color-brand);
    font-size: 11px;
    line-height: 1.5;
    white-space: nowrap;
  }

  .topic-row__participants {
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 0;
    isolation: isolate;
  }

  .topic-row__avatar {
    position: relative;
    display: inline-flex;
    width: 24px;
    height: 24px;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    border: 1px solid var(--color-border);
    border-radius: 0;
    background: var(--color-bg-card);
  }

  :global(.topic-row__participant-avatar .avatar) {
    border-radius: 0 !important;
  }

  /* 头像触发链接的焦点环：内层 <a> 的 outline 被 overflow:hidden 裁剪，
     提到外层圆 span 上呈现（键盘 Tab / 点击聚焦时可见）。 */
  .topic-row__avatar:focus-within {
    overflow: visible;
    outline: 2px solid var(--color-focus-ring);
    outline-offset: 3px;
    z-index: 2;
  }

  .topic-row__avatar + .topic-row__avatar {
    margin-left: -7px;
  }

  .topic-row__more {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    margin-left: 3px;
    color: var(--color-text-tertiary);
    font-size: 13px;
    font-weight: 600;
    line-height: 1;
    letter-spacing: 0.05em;
    user-select: none;
  }

  /* 参与者栈纯 DOM 序层级（后者在上，-7px 叠加）：金色帧环与头像本体
     同层——被下一个头像盖住的部分自然收进环内，不做任何抬升或间隙
     （产品约定：装备框不改变栈的层级语义）。键盘焦点环例外：聚焦时
     outline 提到 z-index:2，不被相邻头像裁掉。 */

  .topic-row__metric {
    display: grid;
    min-width: 0;
    justify-items: end;
    color: var(--color-text-secondary);
    font-family: var(--font-family-mono);
    font-variant-numeric: tabular-nums;
  }

  .topic-row__metric strong,
  .topic-row__metric time {
    overflow: hidden;
    max-width: 100%;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .topic-row__metric strong {
    color: var(--color-text-secondary);
    font-size: var(--text-base);
    font-weight: 500;
    line-height: 1.3;
  }

  .topic-row__metric strong.is-hot {
    color: var(--color-brand);
    font-weight: 700;
  }

  .topic-row__activity time {
    color: var(--color-text-tertiary);
    font-size: var(--text-xs);
    line-height: 1.3;
  }

  .topic-row__mobile-stats {
    display: none;
  }

  /* 原型式卡片元素：桌面表格态默认隐藏（移动层内再启用） */
  .topic-row__mobile-footer {
    display: none;
  }

  @media (max-width: 900px) {
    .topic-row {
      grid-template-columns: minmax(0, 1fr) 64px 72px 88px;
    }

    .topic-row__participants {
      display: none;
    }
  }

  @media (max-width: 767px) {
    .topic-row {
      grid-template-columns: minmax(0, 1fr);
    }

    .topic-row__participants,
    .topic-row__metric {
      display: none;
    }
  }

  @media (max-width: 767px) {
    .topic-row {
      grid-template-columns: minmax(0, 1fr);
      gap: var(--space-2);
      min-height: 64px;
      padding: 13px 14px 12px 16px;
    }

    .topic-row__title {
      font-size: var(--text-base);
    }

    .topic-row__meta {
      gap: 6px;
      margin-top: 6px;
      font-size: 11px;
    }

    .topic-row__board {
      max-width: 54vw;
    }

    .topic-row__author {
      max-width: 28vw;
    }

    .topic-row__metric {
      display: none;
    }

    .topic-row__mobile-stats {
      display: flex;
      align-items: center;
      gap: var(--space-3);
      overflow: hidden;
      color: var(--color-text-tertiary);
      font: 11px/1.35 var(--font-family-mono);
      font-variant-numeric: tabular-nums;
      white-space: nowrap;
    }

    .topic-row__mobile-stats span {
      overflow: hidden;
      text-overflow: ellipsis;
    }

    /* 原型式卡片元素（页脚 chip/计数）：≤767px 启用 */
    .topic-row__mobile-footer {
      display: flex;
      align-items: center;
      gap: var(--space-3);
      margin-top: 9px;
    }

    .topic-row__footer-chip {
      display: inline-flex;
      align-items: center;
      gap: 5px;
      max-width: 50%;
      padding: 3px 9px;
      overflow: hidden;
      border: 1px solid var(--color-border);
      border-radius: var(--radius-sm);
      color: var(--color-text-secondary);
      font-size: 11px;
      line-height: 1.4;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .topic-row__footer-stat {
      display: inline-flex;
      align-items: center;
      gap: 4px;
      color: var(--color-text-tertiary);
      font: 500 12px/1.3 var(--font-family-mono);
      font-variant-numeric: tabular-nums;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .topic-row {
      transition: none;
    }
  }
</style>
