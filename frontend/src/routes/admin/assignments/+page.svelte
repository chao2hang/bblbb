<script lang="ts">
  // M03-UI-07：Assignment 管理页——展示可授予角色集与 assignment 契约说明。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { adminStateLabel } from '$lib/admin';
  import type { AdminAssignmentsPageData } from './+page.server';

  let { data }: { data: AdminAssignmentsPageData } = $props();

  const state = $derived(data.loadState);

  /** 角色 chip 本地化（后端返回英文角色名，未收录时原样显示）。 */
  const ROLE_LABELS: Record<string, string> = {
    administrator: '管理员',
    global_moderator: '全站版主',
    board_moderator: '板块版主',
    member: '成员'
  };
  function roleLabel(name: string): string {
    return ROLE_LABELS[name] ?? name;
  }
</script>

<svelte:head>
  <title>角色委派 — BBLBB</title>
</svelte:head>

<PageHeader title="角色委派" />

<div class="app-card">
  <div class="app-card__head"><h2>角色委派</h2></div>
  <div class="app-card__body">
    <p class="auth-hint">
      在这里把板块版主等角色授予指定用户，并可设置生效期限：到期后自动失效。
      所有授予与撤销都会写入审计日志；用户的实际权限始终以服务端裁决为准。
    </p>

    {#if state.state === 'forbidden'}
      <p class="input-hint is-error" role="alert"><Icon name="lock" size={14} /> {adminStateLabel('forbidden')}</p>
    {:else if state.state === 'not_implemented'}
      <p class="input-hint" role="note">角色委派接口开发中。</p>
    {:else if state.state === 'error'}
      <p class="input-hint is-error" role="alert">{state.message || adminStateLabel('error')}</p>
    {:else if state.state === 'ok'}
      <p class="input-hint">可授予角色：</p>
      <ul style="list-style:none;margin:var(--space-2) 0;padding:0;display:flex;flex-wrap:wrap;gap:var(--space-2);">
        {#each state.items as item (item.id)}
          <li class="tag-chip"><Icon name="shield-check" size={12} /><span>{roleLabel(item.name)}</span></li>
        {/each}
      </ul>
    {/if}
  </div>
</div>
