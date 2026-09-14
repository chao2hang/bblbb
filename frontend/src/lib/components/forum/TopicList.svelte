<script lang="ts">
  // 话题列表（首页 / 发现 / 板块详情共用）：列表框 + 表头（话题|参与者|回复|浏览|活动）
  // + TopicRow 行 + 首页同款空态（标题 + 描述 + 可选发布 CTA）。
  //
  // 此前「表头 + TopicRow」结构在首页与发现页各复制一份，板块详情页仍是旧
  // PostList 行（无参与者/浏览/活动列）。本组件把首页的列表观感沉淀为单一
  // 事实来源，供三处引用（参照首页优化板块文章列表，M18-BOARD-02）。
  //
  // - frameless：嵌入已有 L1 面板（如板块 post-panel 卡片）时去掉外层边框
  //   与上边距，由宿主面板充当唯一列表外框（避免双框）。
  // - label：传入时容器带 role="feed" + aria-label（板块页沿用旧 PostList
  //   的可访问性语义）；首页/发现不传，保持原 DOM 不变。
  import TopicRow from './TopicRow.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import type { PublicPresentationTokens } from '$lib/api/types';

  export interface TopicListRow {
    id: string;
    title: string;
    author: string;
    /** 作者账号用户名（参与者列头像悬浮资料卡用；缺省时作者头像不出卡）。 */
    authorUsername?: string | null;
    /** 作者装扮安全投影（服务端 presentation_tokens；参与者列楼主头像的
     *  已装备头像框数据源，缺省不渲染装扮）。 */
    authorPresentation?: PublicPresentationTokens | null;
    /** 作者上传头像附件 id（公开引用；缺省回退首字母占位头像）。 */
    authorAvatarAttachmentId?: string | null;
    /** 跨板块信息流传板块名；单板块页（板块详情）不传，避免每行重复同一板块名。 */
    boardLabel?: string | null;
    /** 板块 slug（身份图标/颜色解析键；与 boardLabel 同缺省语义）。 */
    boardSlug?: string | null;
    /** 持久化板块图标（boards.icon；null 由 boardVisuals 回退 slug 映射）。 */
    boardIcon?: string | null;
    /** 真实参与者预览（未传时 TopicRow 回退为作者头像）；展示名优先
     *  display_name，缺省回退 username，归一由 TopicRow 承接。 */
    participants?: Array<{
      id?: string;
      username?: string | null;
      display_name?: string | null;
      avatar_attachment_id?: string | null;
      presentation_tokens?: PublicPresentationTokens | null;
    }>;
    likeCount?: number;
    replyCount?: number;
    viewCount?: number;
    pinned?: boolean;
    featured?: boolean;
    createdAt: number;
    lastReplyAt?: number | null;
    /** 算法推荐理由（仅推荐流传入，行 meta 渲染为品牌色小徽标）。 */
    reasonBadge?: string | null;
  }

  let {
    rows,
    emptyTitle = '暂无帖子',
    emptyDesc = '成为第一个发帖的人吧！',
    emptyCta = null,
    frameless = false,
    label = null
  }: {
    rows: TopicListRow[];
    emptyTitle?: string;
    emptyDesc?: string;
    /** 空态发布引导（首页/板块/发现的发布动作各自解析 href）。 */
    emptyCta?: { href: string; label: string } | null;
    frameless?: boolean;
    label?: string | null;
  } = $props();
</script>

<div
  class="topic-list"
  class:topic-list--frameless={frameless}
  role={label ? 'feed' : undefined}
  aria-label={label ?? undefined}
>
  <div class="topic-list-head">
    <span class="topic-list-head__main">话题</span>
    <span class="topic-list-head__participants">参与者</span>
    <span>回复</span>
    <span>浏览</span>
    <span>活动</span>
  </div>
  {#each rows as row (row.id)}
    <TopicRow
      id={row.id}
      title={row.title}
      author={row.author}
      authorUsername={row.authorUsername ?? null}
      authorPresentation={row.authorPresentation ?? null}
      authorAvatarAttachmentId={row.authorAvatarAttachmentId ?? null}
      boardLabel={row.boardLabel ?? null}
      boardSlug={row.boardSlug ?? null}
      boardIcon={row.boardIcon ?? null}
      participants={row.participants ?? []}
      likeCount={row.likeCount ?? 0}
      replyCount={row.replyCount ?? 0}
      viewCount={row.viewCount ?? 0}
      pinned={row.pinned ?? false}
      featured={row.featured ?? false}
      createdAt={row.createdAt}
      lastReplyAt={row.lastReplyAt ?? null}
      reasonBadge={row.reasonBadge ?? null}
    />
  {/each}

  {#if rows.length === 0}
    <div class="thread-empty">
      <div class="empty-state-title">{emptyTitle}</div>
      <p class="empty-state-desc">{emptyDesc}</p>
      {#if emptyCta}
        <a class="empty-state-cta" href={emptyCta.href}>
          <Icon name="plus" size={15} />
          <span>{emptyCta.label}</span>
        </a>
      {/if}
    </div>
  {/if}
</div>

<style>
  /* 列表框（原型 section.feed 的 .topic-list）：与工具条之间留 12px 间距 */
  .topic-list {
    margin-top: 12px;
    background: var(--color-bg-card);
    border: var(--border-default);
    border-radius: 0;
    overflow: hidden;
  }

  /* 嵌入宿主面板（板块 post-panel）：面板本身是唯一外框，列表贴边填充 */
  .topic-list--frameless {
    margin-top: 0;
    border: 0;
    background: transparent;
  }

  .topic-list-head {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 112px 64px 72px 88px;
    gap: var(--space-3);
    align-items: center;
    min-height: 36px;
    padding: 0 var(--space-4) 0 18px;
    border-bottom: 1px solid var(--color-border-strong);
    color: var(--color-text-tertiary);
    font: 600 11px/1.2 var(--font-family-mono);
    font-variant-numeric: tabular-nums;
    letter-spacing: var(--label-letter-spacing);
    text-transform: uppercase;
  }
  .topic-list-head > span:not(.topic-list-head__main):not(.topic-list-head__participants) {
    text-align: right;
  }
  .topic-list-head__participants {
    text-align: center;
  }
  .topic-list-head__main {
    min-width: 0;
  }

  .thread-empty {
    padding: 48px 24px;
    text-align: center;
  }
  .empty-state-title {
    font-size: var(--text-base);
    font-weight: var(--weight-medium);
  }
  .empty-state-desc {
    color: var(--color-text-secondary);
    font-size: var(--text-sm);
    margin: var(--space-2) 0 0;
  }
  .empty-state-cta {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-top: 14px;
    padding: 8px 18px;
    border-radius: var(--radius-sm);
    background: var(--color-brand);
    color: var(--color-text-on-brand);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    text-decoration: none;
    transition: background var(--duration-fast);
  }
  .empty-state-cta:hover {
    background: var(--color-brand-hover);
  }

  /* 表头随 TopicRow 的列收缩同步降级：窄屏先隐藏「参与者」列，再整行隐藏
     （TopicRow 自带移动端统计行）。767px 与 TopicRow 的移动端断点一致，
     避免 640–767px 区间出现「表头 4 列 + 行已是移动端单列」的错位。 */
  @media (max-width: 900px) {
    .topic-list-head {
      grid-template-columns: minmax(0, 1fr) 64px 72px 88px;
    }
    .topic-list-head > span:nth-child(2) {
      display: none;
    }
  }

  @media (max-width: 767px) {
    .topic-list-head {
      display: none;
    }
  }
</style>
