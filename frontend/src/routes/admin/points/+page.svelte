<script lang="ts">
  // M13-UI-04：积分/活跃配置页——签到与奖励（后端裁决；不直接改余额/流水）。
  import Icon from '$lib/components/ui/Icon.svelte';
  import { adminStateLabel } from '$lib/admin';
  import type { AdminPointsPageData } from './+page.server';

  let { data }: { data: AdminPointsPageData } = $props();
</script>

<svelte:head>
  <title>积分/活跃配置 — BBLBB</title>
</svelte:head>

<div class="card">
  <div class="card-header"><span class="card-title">积分 / 活跃配置</span></div>
  <div class="card-body">
    {#if data.state === 'forbidden'}
      <p class="input-hint is-error" role="alert"><Icon name="lock" size={14} /> {adminStateLabel('forbidden')}</p>
    {:else if data.state === 'not_implemented'}
      <p class="input-hint" role="note">积分配置接口开发中。</p>
    {:else if data.state === 'error'}
      <p class="input-hint is-error" role="alert">{data.error || adminStateLabel('error')}</p>
    {:else if data.state === 'ok'}
      <p class="input-hint" role="note">积分/活跃奖励配置由服务端账本裁决；本页只读展示配置，禁止直接修改余额或历史流水。</p>
      <dl style="display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:var(--space-3);">
        {#each Object.entries(data.config ?? {}) as [key, value]}
          <div style="padding:var(--space-3);border:1px solid var(--color-border);border-radius:var(--radius-md);">
            <dt class="text-secondary" style="font-size:var(--text-sm);">{key}</dt>
            <dd style="margin:0;word-break:break-all;"><code>{typeof value === 'object' ? JSON.stringify(value) : String(value)}</code></dd>
          </div>
        {/each}
      </dl>
      <p style="margin-top:var(--space-3);">
        <a class="btn btn-secondary btn-sm" href="/admin/activity">打开活跃任务配置</a>
        <a class="btn btn-secondary btn-sm" href="/admin/shop">打开商城配置</a>
        <a class="btn btn-secondary btn-sm" href="/admin/levels">等级与附件配额</a>
      </p>
    {/if}
  </div>
</div>
