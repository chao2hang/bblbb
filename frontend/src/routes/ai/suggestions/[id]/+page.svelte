<script lang="ts">
  // M09-UI-04/05：AI 建议详情——diff 预览、字段级采纳、版本冲突恢复、
  // moderation 内部信息边界。
  // - formatting/seo/tagging：字段级采纳表单（原生 POST ?/accept），
  //   409 → 冲突提示 + 重新加载；仅显示后端返回的安全纯文本字段；
  // - moderation：只展示公开合规摘要与边界说明，内部 Prompt/举报信号不展示。
  import { aiPurposeLabel } from '$lib/api/client';
  import { renderTextDiff, hasDiff } from '$lib/ai/diff';
  import Button from '$lib/components/ui/Button.svelte';
  import type { AiSuggestionActionData, AiSuggestionPageData } from './+page.server';

  let { data, form }: { data: AiSuggestionPageData; form?: AiSuggestionActionData | null } = $props();

  const suggestion = $derived(data.suggestion);
  const forbidden = $derived(data.forbidden);
  const notFound = $derived(data.notFound);
  const error = $derived(data.error);
  const message = $derived(form?.message ?? null);
  const conflict = $derived(form?.conflict === true);

  function fieldLabel(field: string): string {
    const map: Record<string, string> = {
      title: '标题',
      content: '正文',
      markdown: '正文',
      summary: '摘要',
      tags: '标签'
    };
    return map[field] ?? field;
  }

  function isModeration(): boolean {
    return suggestion?.type === 'moderation';
  }
</script>

<svelte:head>
  <title>AI 建议 — BBLBB</title>
  <meta name="robots" content="noindex,follow" />
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <a href="/ai" class="breadcrumb-link">AI 能力</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">建议</span>
  </nav>

  {#if notFound}
    <div class="card">
      <div class="card-body"><p class="input-hint" style="margin:0;">建议不存在或已被移除。</p></div>
    </div>
  {:else if forbidden}
    <div class="card">
      <div class="card-body"><p class="input-hint is-error" role="alert" style="margin:0;">你没有权限查看该建议。</p></div>
    </div>
  {:else if error}
    <div class="card">
      <div class="card-body">
        <p class="input-hint is-error" role="alert" style="margin:0;">{error}</p>
        <a class="btn btn-secondary btn-sm" style="margin-top:var(--space-2);" href="/ai">返回 AI 能力</a>
      </div>
    </div>
  {:else if suggestion}
    {#if message}
      <p class="input-hint {conflict ? 'is-error' : ''}" role="alert">{message}</p>
    {/if}
    {#if conflict}
      <div class="alert alert-warning" role="alert" style="padding:var(--space-3);border:1px solid var(--color-warning);border-radius:var(--radius-md);margin-bottom:var(--space-3);">
        <p style="margin:0 0 var(--space-2);">内容已更新，建议已过期（版本冲突）。加载最新建议后再采纳，避免覆盖新编辑。</p>
        <a class="btn btn-primary btn-sm" href="/ai/suggestions/{suggestion.id}">重新加载</a>
      </div>
    {/if}

    <div class="card">
      <div class="card-header"><span class="card-title">AI 建议 {suggestion.id.slice(0, 8)}</span></div>
      <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
        <div style="display:flex;flex-wrap:wrap;gap:var(--space-2);align-items:center;">
          <span class="badge {suggestion.status === 'accepted' ? 'badge-success' : 'badge-neutral'}">
            {suggestion.status === 'accepted' ? '已采纳' : suggestion.status === 'expired' || suggestion.status === 'superseded' ? '已过期' : '待处理'}
          </span>
          <span class="text-secondary" style="font-size:var(--text-sm);">{aiPurposeLabel(suggestion.type)}</span>
          <span class="text-secondary" style="font-size:var(--text-sm);">基于内容版本 v{suggestion.base_version}</span>
        </div>

        {#if isModeration()}
          <!-- M09-UI-05：moderation 建议信息边界。 -->
          <div class="alert" style="padding:var(--space-3);border:1px solid var(--color-info, #4a90d9);border-radius:var(--radius-md);">
            <p style="margin:0 0 var(--space-2);"><strong>审核建议（仅审核人员可见）</strong></p>
            <p style="margin:0 0 var(--space-2);">目标类型：{suggestion.moderation?.target_type ?? 'post'}</p>
            {#if suggestion.moderation?.summary}
              <p style="margin:0;">公开合规摘要：{suggestion.moderation.summary}</p>
            {:else}
              <p style="margin:0;">（无公开合规摘要）</p>
            {/if}
          </div>
          <p class="input-hint" role="note" style="margin:0;">
            内部 Prompt、模型原始输出与举报证据属于审核内部信息，不会在本页面展示，也不对作者可见。
            采纳审核建议只创建人工审核动作草稿，不自动处罚或改变权限。
          </p>
        {:else}
          {#each suggestion.fields as field (field.field)}
            {#if hasDiff(field.current, field.proposed)}
              <div style="border:var(--border-default);border-radius:var(--radius-md);padding:var(--space-3);">
                <div style="display:flex;flex-wrap:wrap;gap:var(--space-2);align-items:center;margin-bottom:var(--space-1);">
                  <strong>{fieldLabel(field.field)}</strong>
                  {#if field.reason}
                    <span class="text-secondary" style="font-size:var(--text-xs);">{field.reason}</span>
                  {/if}
                </div>
                {#if field.field === 'content' || field.field === 'markdown'}
                  <pre class="ai-diff" style="margin:0 0 var(--space-2);max-height:280px;overflow:auto;font-size:var(--text-sm);white-space:pre-wrap;background:var(--color-bg-subtle, rgba(0,0,0,0.04));border-radius:var(--radius-md);padding:var(--space-2);">
{#each renderTextDiff(field.current ?? '', field.proposed) as line (line.text + line.type)}
{line.type === 'removed' ? '-' : line.type === 'added' ? '+' : ' '} {line.text}{/each}</pre>
                {:else}
                  <p class="input-hint" style="margin:0 0 var(--space-2);">{field.current ?? '（无当前值）'} → <strong>{field.proposed}</strong></p>
                {/if}
                <form method="POST" action="?/accept">
                  <input type="hidden" name="expected_base_version" value={suggestion.base_version} />
                  <input type="hidden" name="selected_field" value={field.field} />
                  <Button text="采纳此字段" variant="primary" size="sm" type="submit" />
                </form>
              </div>
            {/if}
          {/each}
          <p class="input-hint" style="margin:0;">建议只做 diff 预览，需要你手动逐字段采纳；采纳会写入 revision 并重新校验内容策略与可见性。</p>
        {/if}
      </div>
    </div>
  {/if}
</div>
