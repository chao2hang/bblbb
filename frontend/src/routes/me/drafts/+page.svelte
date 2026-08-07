<script lang="ts">
  // M04-UI-03：我的草稿——SSR 列表（无 JS 可读）；打开（/editor?draft=）与
  // 删除（客户端 DELETE + confirm）渐进增强。
  import { goto } from '$app/navigation';
  import { deleteDraft, type Draft } from '$lib/api/client';
  import { problemMessage, type Problem } from '$lib/errors';
  import Button from '$lib/components/ui/Button.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { formatTime, charCount } from '$lib/utils';
  import type { DraftsPageData } from './+page.server';

  let { data }: { data: DraftsPageData } = $props();

  /** 本会话内删除的草稿 id（避免改动 data 本身；列表由 derived 派生）。 */
  let deletedIds = $state<Set<string>>(new Set());
  const drafts = $derived(data.drafts.filter((d) => !deletedIds.has(d.id)));
  const loadError = $derived(data.error);
  let deletingId = $state<string | null>(null);
  let actionError = $state('');

  function typeLabel(d: Draft): string {
    return d.type === 'article' ? '文章' : '讨论';
  }

  async function handleDelete(d: Draft) {
    if (!window.confirm(`确定删除草稿「${d.title}」吗？删除后不可恢复。`)) return;
    deletingId = d.id;
    actionError = '';
    try {
      await deleteDraft(fetch, d.id);
      deletedIds = new Set([...deletedIds, d.id]);
    } catch (err: unknown) {
      actionError = problemMessage(err as Problem);
    }
    deletingId = null;
  }

  function openDraft(d: Draft) {
    goto(`/editor?draft=${encodeURIComponent(d.id)}`);
  }
</script>

<svelte:head>
  <title>我的草稿 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <a href="/me" class="breadcrumb-link">我的主页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">我的草稿</span>
  </nav>

  <div class="card">
    <div class="card-header" style="display:flex;align-items:center;justify-content:space-between;">
      <span class="card-title">草稿（{drafts.length}）</span>
      <Button text="新建" variant="primary" size="sm" icon="pen-line" href="/editor" />
    </div>
    <div class="card-body" style="padding:0;">
      {#if loadError}
        <p class="input-hint is-error" role="alert" style="padding:var(--space-4);">{loadError}</p>
      {/if}
      {#if actionError}
        <p class="input-hint is-error" role="alert" style="padding:var(--space-4);">{actionError}</p>
      {/if}
      {#if drafts.length === 0 && !loadError}
        <div style="padding:var(--space-6);">
          <EmptyState icon="save" title="暂无草稿" desc="在编辑器发布页会为你自动保存未发布的内容" />
        </div>
      {:else}
        <div style="display:flex;flex-direction:column;">
          {#each drafts as draft (draft.id)}
            <div class="post-row" style="padding:var(--space-4);border-bottom:var(--border-default);display:flex;gap:var(--space-3);align-items:center;">
              <div style="min-width:0;flex:1;">
                <div style="font-weight:var(--weight-medium);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">
                  {draft.title}
                </div>
                <div class="text-secondary" style="font-size:var(--text-sm);margin-top:2px;">
                  <span class="badge badge-neutral">{typeLabel(draft)}</span>
                  <span style="margin:0 var(--space-1);">·</span>
                  {charCount(draft.markdown)} 字
                  <span style="margin:0 var(--space-1);">·</span>
                  更新于 {formatTime(draft.updated_at)}
                  <span style="margin:0 var(--space-1);">·</span>
                  v{draft.version}
                </div>
              </div>
              <div style="display:flex;gap:var(--space-2);flex-shrink:0;">
                <Button text="打开" variant="secondary" size="sm" onclick={() => openDraft(draft)} />
                <Button
                  text={deletingId === draft.id ? '删除中…' : '删除'}
                  variant="ghost"
                  size="sm"
                  onclick={() => handleDelete(draft)}
                  disabled={deletingId === draft.id}
                />
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</div>
