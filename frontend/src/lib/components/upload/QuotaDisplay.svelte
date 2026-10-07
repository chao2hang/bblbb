<script lang="ts">
  // M06-UI-02：附件容量摘要组件——展示当前等级单文件上限、总容量、已用/预留
  // 与剩余额度（数据来自 GET /attachments 的 quota 摘要，字段缺失时降级）。
  import type { AttachmentQuota } from '$lib/api/types';
  import { formatBytes } from './formatBytes';

  let {
    quota,
    loading = false,
    error = ''
  }: {
    quota: AttachmentQuota | null;
    loading?: boolean;
    error?: string;
  } = $props();

  function pct(used: number, total: number): number {
    if (!total || used <= 0) return 0;
    return Math.min(100, Math.round((used / total) * 100));
  }
</script>

<div class="quota-card">
  <h3 class="quota-title">存储额度</h3>
  {#if loading}
    <p class="input-hint" role="status">加载容量信息…</p>
  {:else if error}
    <p class="input-hint is-error" role="alert">{error}</p>
  {:else if quota}
    <dl class="quota-grid">
      <div class="quota-item">
        <dt>单文件上限</dt>
        <dd>{formatBytes(quota.max_file_bytes)}</dd>
      </div>
      <div class="quota-item">
        <dt>总容量</dt>
        <dd>{formatBytes(quota.total_bytes)}</dd>
      </div>
      <div class="quota-item">
        <dt>已用</dt>
        <dd>{formatBytes(quota.used_bytes)}</dd>
      </div>
      <div class="quota-item">
        <dt>剩余</dt>
        <dd class={quota.remaining_bytes <= 0 ? 'is-warning' : ''}>{formatBytes(quota.remaining_bytes)}</dd>
      </div>
      {#if typeof quota.reserved_bytes === 'number' && quota.reserved_bytes > 0}
        <div class="quota-item">
          <dt>预留</dt>
          <dd>{formatBytes(quota.reserved_bytes)}</dd>
        </div>
      {/if}
      {#if typeof quota.charged_bytes === 'number' && quota.charged_bytes !== quota.used_bytes}
        <div class="quota-item">
          <dt>计费</dt>
          <dd>{formatBytes(quota.charged_bytes)}</dd>
        </div>
      {/if}
    </dl>
    <div
      class="quota-bar"
      role="progressbar"
      aria-label="容量使用率"
      aria-valuenow={pct(quota.used_bytes, quota.total_bytes)}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <div class="quota-bar-fill" style="width:{pct(quota.used_bytes, quota.total_bytes)}%;"></div>
    </div>
    <p class="quota-hint">
      {#if typeof quota.daily_upload_bytes === 'number' && typeof quota.daily_used_bytes === 'number'}
        今日上传 {formatBytes(quota.daily_used_bytes)} / {formatBytes(quota.daily_upload_bytes)}
        <span class="quota-hint-sep">·</span>
      {/if}
      附件在物理删除后才释放容量
    </p>
  {:else}
    <p class="input-hint">容量信息暂不可用</p>
  {/if}
</div>

<style>
  .quota-card {
    display: flex;
    flex-direction: column;
  }
  .quota-title {
    margin: 0 0 var(--space-3, 12px);
    font-size: var(--text-base, 15px);
    font-weight: 600;
    color: var(--color-text-primary);
  }
  .quota-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(130px, 1fr));
    gap: var(--space-3, 12px) var(--space-5, 20px);
    margin: 0;
  }
  .quota-item dt {
    font-size: var(--text-xs, 12px);
    color: var(--color-text-secondary);
    margin-bottom: 4px;
  }
  .quota-item dd {
    margin: 0;
    font-size: var(--text-base, 15px);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--color-text-primary);
    line-height: 1.25;
  }
  .quota-item .is-warning {
    color: var(--color-danger);
  }
  .quota-bar {
    margin-top: var(--space-4, 16px);
    height: 7px;
    border-radius: 999px;
    background: var(--color-bg-subtle);
    overflow: hidden;
  }
  .quota-bar-fill {
    height: 100%;
    border-radius: 999px;
    background: var(--color-brand);
    transition: width 0.3s ease;
  }
  .quota-hint {
    margin: var(--space-2, 8px) 0 0;
    font-size: var(--text-xs, 12px);
    color: var(--color-text-secondary);
    line-height: 1.5;
  }
  .quota-hint-sep {
    display: inline-block;
    margin: 0 4px;
    color: var(--color-text-tertiary);
  }
</style>
