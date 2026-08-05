<script lang="ts">
  // M03-UI-07：Assignment 管理页——展示可授予角色集与 assignment 契约说明。
  import Icon from '$lib/components/ui/Icon.svelte';
  import { adminStateLabel } from '$lib/admin';
  import type { AdminAssignmentsPageData } from './+page.server';

  let { data }: { data: AdminAssignmentsPageData } = $props();

  const state = $derived(data.loadState);
</script>

<svelte:head>
  <title>Assignment 管理 — BBLBB</title>
</svelte:head>

<div class="card">
  <div class="card-header"><span class="card-title">Assignment 管理</span></div>
  <div class="card-body">
    <p class="auth-hint">
      板块角色授予写入 <code>board_role_assignments</code>（M03-AUTHZ-02/03：复合唯一
      (board_id, user_id, role_id)、<code>expires_at</code> 生效窗口、过期即失效；
      聚合权限仅来自服务端数据）。assignment 管理端点由 M13-ADMIN 提供。
    </p>

    {#if state.state === 'forbidden'}
      <p class="input-hint is-error" role="alert"><Icon name="lock" size={14} /> {adminStateLabel('forbidden')}</p>
    {:else if state.state === 'not_implemented'}
      <p class="input-hint" role="note">角色接口开发中（M13-ADMIN）。</p>
    {:else if state.state === 'error'}
      <p class="input-hint is-error" role="alert">{state.message || adminStateLabel('error')}</p>
    {:else if state.state === 'ok'}
      <p class="input-hint">可授予角色：</p>
      <ul style="list-style:none;margin:var(--space-2) 0;padding:0;display:flex;flex-wrap:wrap;gap:var(--space-2);">
        {#each state.items as item (item.id)}
          <li class="tag-chip"><Icon name="shield-check" size={12} /><span>{item.name}</span></li>
        {/each}
      </ul>
    {/if}
  </div>
</div>
