<script lang="ts">
  // M13-UI-04：等级与附件配额页（M06-QUOTA 已实现，只读脱敏视图）。
  import Icon from '$lib/components/ui/Icon.svelte';
  import { adminStateLabel } from '$lib/admin';
  import type { AdminLevelsPageData } from './+page.server';

  let { data }: { data: AdminLevelsPageData } = $props();

  const rows: { key: string; label: string }[] = [
    { key: 'single_file_max_bytes', label: '单文件上限（字节）' },
    { key: 'total_bytes', label: '总量（字节）' },
    { key: 'daily_upload_bytes', label: '每日上传（字节）' },
    { key: 'retention_days', label: '保留天数' },
    { key: 'policy_version', label: '策略版本' }
  ];
</script>

<svelte:head>
  <title>等级与配额 — BBLBB</title>
</svelte:head>

<div class="card">
  <div class="card-header"><span class="card-title">等级与附件配额</span></div>
  <div class="card-body">
    {#if data.state === 'forbidden'}
      <p class="input-hint is-error" role="alert"><Icon name="lock" size={14} /> {adminStateLabel('forbidden')}</p>
    {:else if data.state === 'not_implemented'}
      <p class="input-hint" role="note">等级配额接口开发中。</p>
    {:else if data.state === 'error'}
      <p class="input-hint is-error" role="alert">{data.error || adminStateLabel('error')}</p>
    {:else if data.state === 'ok'}
      <p class="input-hint" role="note">等级 1 默认附件配额（脱敏；Secret 不在任何响应中）。</p>
      <dl style="display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:var(--space-3);">
        {#each rows as row}
          <div style="padding:var(--space-3);border:1px solid var(--color-border);border-radius:var(--radius-md);">
            <dt class="text-secondary" style="font-size:var(--text-sm);">{row.label}</dt>
            <dd style="margin:0;"><code>{String(data.quota?.policy?.[row.key as keyof typeof data.quota.policy] ?? '—')}</code></dd>
          </div>
        {/each}
      </dl>
      <p style="margin-top:var(--space-3);">
        <a class="btn btn-secondary btn-sm" href="/admin/storage">存储配置</a>
        <a class="btn btn-secondary btn-sm" href="/admin/points">积分/活跃配置</a>
      </p>
    {/if}
  </div>
</div>
