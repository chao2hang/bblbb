<script lang="ts">
  // M03-UI-07：管理标签页——后端裁决状态渲染 + 新建标签表单。
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { adminStateLabel } from '$lib/admin';
  import type { AdminTagsPageData } from './+page.server';

  let { data, form }: { data: AdminTagsPageData; form?: AdminTagsPageData } = $props();

  const state = $derived(form?.loadState ?? data.loadState);
  const created = $derived(form?.created === true);
  const message = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );
</script>

<svelte:head>
  <title>标签管理 — BBLBB</title>
</svelte:head>

<div class="card">
  <div class="card-header"><span class="card-title">标签管理</span></div>
  <div class="card-body">
    {#if state.state === 'forbidden'}
      <p class="input-hint is-error" role="alert"><Icon name="lock" size={14} /> {adminStateLabel('forbidden')}</p>
    {:else if state.state === 'not_implemented'}
      <p class="input-hint" role="note">标签列表接口开发中（M13-ADMIN）。创建表单已可用。</p>
    {:else if state.state === 'error'}
      <p class="input-hint is-error" role="alert">{state.message || adminStateLabel('error')}</p>
    {:else if state.state === 'ok' && state.items.length === 0}
      <p class="input-hint">暂无标签数据。</p>
    {:else if state.state === 'ok'}
      <ul style="list-style:none;margin:0;padding:0;display:flex;flex-wrap:wrap;gap:var(--space-2);">
        {#each state.items as item (item.id)}
          <li class="tag-chip" title={item.description ?? item.name}>
            <Icon name="tag" size={12} />
            <span>{item.name}</span>
          </li>
        {/each}
      </ul>
    {/if}

    {#if created}
      <p class="input-hint" role="status">标签已创建。</p>
    {/if}
    {#if message}
      <p class="input-hint is-error" role="alert">{message}</p>
    {/if}

    <form method="POST" action="?/create" use:enhance class="card" style="margin-top:var(--space-4);">
      <div class="card-header"><span class="card-title">新建标签</span></div>
      <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
        <div class="input-wrapper">
          <label class="input-label" for="admin-tag-name">名称</label>
          <input type="text" class="input-field" id="admin-tag-name" name="name" maxlength="40" required />
        </div>
        <div class="input-wrapper">
          <label class="input-label" for="admin-tag-reason">操作原因（审计）</label>
          <input type="text" class="input-field" id="admin-tag-reason" name="reason" required placeholder="记录到审计日志" />
        </div>
        <div><Button text="创建标签" variant="primary" size="sm" type="submit" /></div>
      </div>
    </form>
  </div>
</div>
