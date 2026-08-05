<script lang="ts">
  // M03-UI-07：管理角色页——后端裁决状态渲染（角色列表当前 501，M13-ADMIN）。
  import Icon from '$lib/components/ui/Icon.svelte';
  import { adminStateLabel } from '$lib/admin';
  import type { AdminRolesPageData } from './+page.server';

  let { data }: { data: AdminRolesPageData } = $props();

  const state = $derived(data.loadState);
</script>

<svelte:head>
  <title>角色管理 — BBLBB</title>
</svelte:head>

<div class="card">
  <div class="card-header"><span class="card-title">角色管理</span></div>
  <div class="card-body">
    {#if state.state === 'forbidden'}
      <p class="input-hint is-error" role="alert"><Icon name="lock" size={14} /> {adminStateLabel('forbidden')}</p>
    {:else if state.state === 'not_implemented'}
      <p class="input-hint" role="note">角色列表接口开发中（M13-ADMIN）。角色数据由种子与 M03-AUTHZ 聚合权限服务提供。</p>
    {:else if state.state === 'error'}
      <p class="input-hint is-error" role="alert">{state.message || adminStateLabel('error')}</p>
    {:else if state.state === 'ok'}
      <ul style="list-style:none;margin:0;padding:0;display:flex;flex-direction:column;gap:var(--space-2);">
        {#each state.items as item (item.id)}
          <li style="padding:var(--space-3);border:1px solid var(--color-border);border-radius:var(--radius-md);">
            <strong>{item.name}</strong>
            {#if item.scope}<span class="badge badge-neutral">{item.scope}</span>{/if}
            {#if item.permissions}<span class="text-secondary" style="font-size:var(--text-xs);margin-left:var(--space-2);">{(item.permissions as string[]).join('、')}</span>{/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>
