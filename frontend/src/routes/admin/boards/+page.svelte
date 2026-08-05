<script lang="ts">
  // M03-UI-07：管理板块页——后端裁决状态渲染 + 新建板块表单。
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { adminStateLabel } from '$lib/admin';
  import type { AdminBoardsPageData } from './+page.server';

  let { data, form }: { data: AdminBoardsPageData; form?: AdminBoardsPageData } = $props();

  const state = $derived(form?.loadState ?? data.loadState);
  const created = $derived(form?.created === true);
  const message = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );
</script>

<svelte:head>
  <title>板块管理 — BBLBB</title>
</svelte:head>

<div class="card">
  <div class="card-header"><span class="card-title">板块管理</span></div>
  <div class="card-body">
    {#if state.state === 'forbidden'}
      <p class="input-hint is-error" role="alert"><Icon name="lock" size={14} /> {adminStateLabel('forbidden')}</p>
    {:else if state.state === 'not_implemented'}
      <p class="input-hint" role="note">板块列表接口开发中（M13-ADMIN）。创建表单已可用。</p>
    {:else if state.state === 'error'}
      <p class="input-hint is-error" role="alert">{state.message || adminStateLabel('error')}</p>
    {:else if state.state === 'ok' && state.items.length === 0}
      <p class="input-hint">暂无板块数据。</p>
    {:else if state.state === 'ok'}
      <ul style="list-style:none;margin:0;padding:0;display:flex;flex-direction:column;gap:var(--space-2);">
        {#each state.items as item (item.id)}
          <li style="display:flex;justify-content:space-between;gap:var(--space-3);padding:var(--space-3);border:1px solid var(--color-border);border-radius:var(--radius-md);">
            <div>
              <strong>{item.name}</strong>
              <span class="text-secondary" style="font-size:var(--text-sm);margin-left:var(--space-2);">/{item.slug}</span>
            </div>
            <a class="btn btn-secondary btn-sm" href="/boards/{item.slug}">查看</a>
          </li>
        {/each}
      </ul>
    {/if}

    {#if created}
      <p class="input-hint" role="status">板块已创建。</p>
    {/if}
    {#if message}
      <p class="input-hint is-error" role="alert">{message}</p>
    {/if}

    <form method="POST" action="?/create" use:enhance class="card" style="margin-top:var(--space-4);">
      <div class="card-header"><span class="card-title">新建板块</span></div>
      <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
        <div class="input-wrapper">
          <label class="input-label" for="admin-board-name">名称</label>
          <input type="text" class="input-field" id="admin-board-name" name="name" maxlength="100" required />
        </div>
        <div class="input-wrapper">
          <label class="input-label" for="admin-board-slug">slug</label>
          <input type="text" class="input-field" id="admin-board-slug" name="slug" maxlength="120" pattern="[a-z0-9-]+" required />
          <p class="input-hint">小写字母/数字/连字符，唯一。</p>
        </div>
        <div class="input-wrapper">
          <label class="input-label" for="admin-board-desc">说明</label>
          <textarea class="input-field" id="admin-board-desc" name="description" rows="3" maxlength="2000"></textarea>
        </div>
        <div class="input-wrapper">
          <label class="input-label" for="admin-board-visibility">可见性</label>
          <select class="input-field" id="admin-board-visibility" name="visibility">
            <option value="public">public（公开）</option>
            <option value="members">members（登录成员）</option>
            <option value="restricted">restricted（需加入）</option>
            <option value="hidden">hidden（管理可见）</option>
          </select>
        </div>
        <div class="input-wrapper">
          <label class="input-label" for="admin-board-mode">发帖模式</label>
          <select class="input-field" id="admin-board-mode" name="posting_mode">
            <option value="normal">normal（正常）</option>
            <option value="approval">approval（审核）</option>
            <option value="readonly">readonly（只读）</option>
            <option value="closed">closed（关闭）</option>
          </select>
        </div>
        <div class="input-wrapper">
          <label class="input-label" for="admin-board-reason">操作原因（审计）</label>
          <input type="text" class="input-field" id="admin-board-reason" name="reason" required placeholder="记录到审计日志" />
        </div>
        <div><Button text="创建板块" variant="primary" size="sm" type="submit" /></div>
      </div>
    </form>
  </div>
</div>
