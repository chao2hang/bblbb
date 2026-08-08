<script lang="ts">
  // M13-UI-01：后台概览页（权限门由服务端裁决）。
  import Icon from '$lib/components/ui/Icon.svelte';
  import { adminStateLabel } from '$lib/admin';
  import type { AdminOverviewData } from '../_overview';

  let { data }: { data: AdminOverviewData } = $props();
</script>

<svelte:head>
  <title>{data.title} — BBLBB</title>
</svelte:head>

<div class="card">
  <div class="card-header"><span class="card-title">{data.title}</span></div>
  <div class="card-body">
    {#if data.state === 'forbidden'}
      <p class="input-hint is-error" role="alert"><Icon name="lock" size={14} /> {adminStateLabel('forbidden')}</p>
    {:else if data.state === 'not_implemented'}
      <p class="input-hint" role="note">{data.error || '接口开发中'}</p>
    {:else if data.state === 'error'}
      <p class="input-hint is-error" role="alert">{data.error || adminStateLabel('error')}</p>
    {:else if data.state === 'ok'}
      <p class="input-hint" role="note">菜单隐藏不是安全边界：本页与所有管理页均由服务端权限门强制（401→登录、403→无权限）。</p>
      <ul style="list-style:none;margin:0;padding:0;display:grid;grid-template-columns:repeat(auto-fill,minmax(220px,1fr));gap:var(--space-3);">
        {#each data.links as link (link.href)}
          <li>
            <a class="card" href={link.href} style="display:block;text-decoration:none;color:inherit;padding:var(--space-3);">
              <strong>{link.label}</strong>
              <span class="text-secondary" style="display:block;font-size:var(--text-sm);margin-top:var(--space-1);">{link.desc}</span>
            </a>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>
