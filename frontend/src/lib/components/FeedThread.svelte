<script lang="ts">
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { formatCount, formatRelative } from '$lib/utils';

  let {
    id,
    title,
    summary = null,
    author,
    createdAt,
    boardLabel = null,
    likeCount = 0,
    replyCount = 0,
    featured = false
  }: {
    id: string;
    title: string;
    summary?: string | null;
    author: string;
    createdAt: number;
    boardLabel?: string | null;
    likeCount?: number;
    replyCount?: number;
    featured?: boolean;
  } = $props();

  const href = $derived(`/posts/${encodeURIComponent(id)}`);
</script>

<article class="thread" class:featured>
  <Avatar name={author} size="lg" seed={author} />
  <div class="thread-body">
    <div class="thread-meta">
      <b>{author}</b>
      <span>· {formatRelative(createdAt)}</span>
      {#if featured}<i>精华</i>{/if}
    </div>
    <a class="thread-detail-link" {href}>
      <h2>{title}</h2>
    </a>
    <div class="thread-footer">
      {#if boardLabel}<span>{boardLabel}</span>{/if}
      <span class="thread-likes" aria-label="{formatCount(likeCount)} 人点赞">
        <Icon name="heart" size={13} />
        {formatCount(likeCount)}
      </span>
      <a class="thread-comment-link" {href} aria-label="查看回复">
        <Icon name="message-square" size={13} />
        {formatCount(replyCount)}
      </a>
    </div>
  </div>
</article>

<style>
  .thread {
    display: flex;
    gap: 13px;
    padding: 14px 16px;
    border-bottom: var(--border-thin);
    background: transparent;
    border-radius: 0;
    transition: background 0.12s;
  }

  .thread:hover {
    background: var(--color-surface-hover);
  }

  .thread:last-child {
    border-bottom: 0;
  }

  .thread-body {
    flex: 1;
    min-width: 0;
  }

  .thread-detail-link {
    display: block;
    color: inherit;
    text-decoration: none;
  }

  .thread-detail-link h2 {
    margin: 0 0 6px;
    color: var(--color-text-primary);
    font-size: 16px;
    font-weight: var(--weight-semibold);
    line-height: 1.6;
  }

  .thread-detail-link:hover h2 {
    color: var(--color-brand);
  }

  .thread-meta {
    color: var(--color-text-secondary);
    font-size: var(--text-sm);
  }

  .thread-meta span {
    margin-left: 5px;
    color: var(--color-text-tertiary);
  }

  .thread-meta i {
    margin-left: 8px;
    padding: 2px 5px;
    border-radius: 4px;
    background: var(--color-brand-soft);
    color: var(--color-brand);
    font-size: 11px;
    font-style: normal;
  }

  .thread-footer {
    display: flex;
    align-items: center;
    gap: 15px;
    margin-top: 13px;
    overflow: hidden;
    color: var(--color-text-tertiary);
    font-size: 13px;
    white-space: nowrap;
  }

  .thread-footer > * {
    padding: 5px 0;
  }

  .thread-footer > span:first-child {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .thread-likes,
  .thread-comment-link {
    display: inline-flex;
    align-items: center;
    flex: 0 0 auto;
    gap: 3px;
    color: var(--color-text-secondary);
  }

  .thread-comment-link {
    gap: 4px;
    text-decoration: none;
  }

  .thread-comment-link:hover {
    color: var(--color-brand);
  }

  @media (max-width: 767px) {
    .thread {
      padding: 14px 12px;
    }

    .thread-detail-link h2 {
      margin: 4px 0 6px;
      line-height: 1.4;
    }

    .thread-footer {
      gap: 12px;
      margin-top: 10px;
    }

    .thread-footer > span:first-child {
      max-width: 45vw;
    }
  }
</style>
