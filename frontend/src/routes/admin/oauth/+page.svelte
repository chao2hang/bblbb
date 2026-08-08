<script lang="ts">
  // M13-UI-04：OIDC 管理页——OAuth Client 列表（secret 永不回显）。
  import Icon from '$lib/components/ui/Icon.svelte';
  import { adminStateLabel } from '$lib/admin';
  import type { AdminOAuthPageData } from './+page.server';

  let { data }: { data: AdminOAuthPageData } = $props();
</script>

<svelte:head>
  <title>OIDC 管理 — BBLBB</title>
</svelte:head>

<div class="card">
  <div class="card-header"><span class="card-title">OIDC / OAuth Client 管理</span></div>
  <div class="card-body">
    {#if data.state === 'forbidden'}
      <p class="input-hint is-error" role="alert"><Icon name="lock" size={14} /> {adminStateLabel('forbidden')}</p>
    {:else if data.state === 'not_implemented'}
      <p class="input-hint" role="note">OIDC 管理接口开发中。</p>
    {:else if data.state === 'error'}
      <p class="input-hint is-error" role="alert">{data.error || adminStateLabel('error')}</p>
    {:else if data.state === 'ok'}
      <p class="input-hint" role="note">Client Secret 只存哈希、创建/轮换时仅显示一次；本页不回显任何 Secret。</p>
      {#if !data.clients || data.clients.length === 0}
        <p class="input-hint">暂无 OAuth Client。</p>
      {:else}
        <div style="overflow-x:auto;">
          <table class="table" aria-label="OAuth Client 列表">
            <thead>
              <tr><th>名称</th><th>类型</th><th>client_id</th><th>状态</th><th>版本</th></tr>
            </thead>
            <tbody>
              {#each data.clients as client (client.id)}
                <tr>
                  <td>{String(client.name)}</td>
                  <td><span class="badge">{String(client.client_type)}</span></td>
                  <td><code>{String(client.client_id)}</code></td>
                  <td><span class="badge">{String(client.status)}</span></td>
                  <td>v{String(client.version)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
      <p style="margin-top:var(--space-3);">
        <a class="btn btn-secondary btn-sm" href="/admin/marketplace">Marketplace Client</a>
      </p>
    {/if}
  </div>
</div>
