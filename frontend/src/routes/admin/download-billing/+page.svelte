<script lang="ts">
  // M13-UI-04：下载计费配置（脱敏视图；M06-DOWNLOAD 已实现，默认关闭）。
  import Icon from '$lib/components/ui/Icon.svelte';
  import { adminStateLabel } from '$lib/admin';
  import type { AdminDownloadBillingPageData } from './+page.server';

  let { data }: { data: AdminDownloadBillingPageData } = $props();

  const fields: { key: keyof NonNullable<AdminDownloadBillingPageData['config']>; label: string }[] = [
    { key: 'configured', label: '已配置' },
    { key: 'mode', label: '计费模式' },
    { key: 'amount', label: '单价（币）' },
    { key: 'authorization_ttl_seconds', label: '授权 TTL（秒）' },
    { key: 'daily_user_limit', label: '每日上限' },
    { key: 'grace_on_disable', label: '停用宽限' },
    { key: 'is_enabled', label: '启用' },
    { key: 'version', label: '版本' }
  ];
</script>

<svelte:head>
  <title>下载计费 — BBLBB</title>
</svelte:head>

<div class="card">
  <div class="card-header"><span class="card-title">Download Billing 配置</span></div>
  <div class="card-body">
    {#if data.state === 'forbidden'}
      <p class="input-hint is-error" role="alert"><Icon name="lock" size={14} /> {adminStateLabel('forbidden')}</p>
    {:else if data.state === 'not_implemented'}
      <p class="input-hint" role="note">下载计费接口开发中。</p>
    {:else if data.state === 'error'}
      <p class="input-hint is-error" role="alert">{data.error || adminStateLabel('error')}</p>
    {:else if data.state === 'ok'}
      <p class="input-hint" role="note">下载抵扣默认关闭（Feature Flag），开启需专项门槛；本页只读展示脱敏策略，不含 Secret/签名 URL。</p>
      <dl style="display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:var(--space-3);">
        {#each fields as field}
          <div style="padding:var(--space-3);border:1px solid var(--color-border);border-radius:var(--radius-md);">
            <dt class="text-secondary" style="font-size:var(--text-sm);">{field.label}</dt>
            <dd style="margin:0;"><code>{String(data.config?.[field.key] ?? '—')}</code></dd>
          </div>
        {/each}
      </dl>
      <p style="margin-top:var(--space-3);">
        <a class="btn btn-secondary btn-sm" href="/admin/storage">存储配置</a>
      </p>
    {/if}
  </div>
</div>
