<script lang="ts">
  // M13-UI-04：附件管理页——存储脱敏视图（后端不回显 access/secret key）。
  import Icon from '$lib/components/ui/Icon.svelte';
  import { adminStateLabel } from '$lib/admin';
  import type { AdminAttachmentsPageData } from './+page.server';

  let { data }: { data: AdminAttachmentsPageData } = $props();

  const fields: { key: keyof NonNullable<AdminAttachmentsPageData['config']>; label: string }[] = [
    { key: 'backend', label: '后端' },
    { key: 'configured', label: '已配置' },
    { key: 'bucket', label: 'Bucket' },
    { key: 'region', label: 'Region' },
    { key: 'signed_url_ttl_seconds', label: '签名 URL TTL（秒）' }
  ];
</script>

<svelte:head>
  <title>附件管理 — BBLBB</title>
</svelte:head>

<div class="card">
  <div class="card-header"><span class="card-title">附件管理</span></div>
  <div class="card-body">
    {#if data.state === 'forbidden'}
      <p class="input-hint is-error" role="alert"><Icon name="lock" size={14} /> {adminStateLabel('forbidden')}</p>
    {:else if data.state === 'not_implemented'}
      <p class="input-hint" role="note">附件管理接口开发中。</p>
    {:else if data.state === 'error'}
      <p class="input-hint is-error" role="alert">{data.error || adminStateLabel('error')}</p>
    {:else if data.state === 'ok'}
      <p class="input-hint" role="note">存储配置为脱敏视图（Access Key / Secret 永不回显）。</p>
      <dl style="display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:var(--space-3);">
        {#each fields as field}
          <div style="padding:var(--space-3);border:1px solid var(--color-border);border-radius:var(--radius-md);">
            <dt class="text-secondary" style="font-size:var(--text-sm);">{field.label}</dt>
            <dd style="margin:0;"><code>{String(data.config?.[field.key] ?? '—')}</code></dd>
          </div>
        {/each}
        <div style="padding:var(--space-3);border:1px solid var(--color-border);border-radius:var(--radius-md);">
          <dt class="text-secondary" style="font-size:var(--text-sm);">凭据状态</dt>
          <dd style="margin:0;">
            <code>
              access: {data.config?.credentials?.access_key_id_configured ? '已配置' : '未配置'}｜
              secret: {data.config?.credentials?.secret_configured ? '已配置' : '未配置'}
            </code>
          </dd>
        </div>
      </dl>
      <p style="margin-top:var(--space-3);">
        <a class="btn btn-secondary btn-sm" href="/admin/storage">存储配置</a>
        <a class="btn btn-secondary btn-sm" href="/admin/download-billing">下载计费</a>
      </p>
    {/if}
  </div>
</div>
