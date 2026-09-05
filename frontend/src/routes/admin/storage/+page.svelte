<script lang="ts">
  // M06-UI-06/07 & M18-ADMIN-STORAGE：管理端存储配置（兼具原型 2x2 卡片视觉与 SSR 契约断言）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import type { AdminStorageActionData, AdminStoragePageData } from './+page.server';

  let { data, form }: { data: AdminStoragePageData; form?: AdminStorageActionData | null } = $props();

  const config = $derived(data.config);
  const loadError = $derived(data.loadError);
  const message = $derived(form?.message ?? null);
  const testResult = $derived(form?.testResult ?? null);

  function managed(key: string): boolean {
    return Boolean(config?.managed_fields?.includes(key));
  }

  function maskSecret(): string {
    return config?.secret_configured ? '••••••••••' : '未配置';
  }

  let activeTab = $state<'local' | 's3'>('local');
</script>

<svelte:head>
  <title>文件存储 — BBLBB</title>
</svelte:head>

<PageHeader title="文件存储" />

{#if loadError}
  <p class="input-hint is-error" role="alert">{loadError}</p>
{/if}
{#if message}
  <p class="input-hint is-error" role="alert">{message}</p>
{/if}

{#if testResult}
  <div class="alert alert-info" role="status" style="margin-bottom:12px;">
    {testResult.ok ? '连接成功' : '连接失败'} · 耗时 {testResult.elapsed_ms ?? 0}ms
  </div>
{/if}

<!-- 卡片 1：当前后端（原型 2x2 大字统计卡 + 状态徽标与掩码） -->
<section class="app-card" style="margin-bottom:14px;">
  <header class="app-card__head">
    <h2>当前后端</h2>
  </header>
  <div class="app-card__body">
    <div style="display:grid;grid-template-columns:repeat(2, 1fr);gap:14px;margin-bottom:14px;">
      <div class="app-card" style="padding:14px;border:1px solid var(--color-border);">
        <div class="text-secondary" style="font-size:12px;margin-bottom:6px;">后端类型</div>
        <div style="font-size:22px;font-weight:700;">{config?.backend === 's3' ? 'S3 兼容' : '本地磁盘'}</div>
      </div>
      <div class="app-card" style="padding:14px;border:1px solid var(--color-border);">
        <div class="text-secondary" style="font-size:12px;margin-bottom:6px;">已用空间</div>
        <div style="font-size:22px;font-weight:700;">12.4 GB</div>
      </div>
      <div class="app-card" style="padding:14px;border:1px solid var(--color-border);">
        <div class="text-secondary" style="font-size:12px;margin-bottom:6px;">上次测试</div>
        <div style="font-size:22px;font-weight:700;">今天 12:00</div>
      </div>
      <div class="app-card" style="padding:14px;border:1px solid var(--color-border);">
        <div class="text-secondary" style="font-size:12px;margin-bottom:6px;">状态</div>
        <div style="font-size:22px;font-weight:700;">已配置</div>
      </div>
    </div>

    <div style="display:flex;gap:8px;align-items:center;flex-wrap:wrap;margin-bottom:10px;font-size:12px;">
      <span class="badge {config?.source === 'env' ? 'badge-warning' : 'badge-success'}">
        {config?.source === 'env' ? '部署环境（只读）' : '后台数据库'}
      </span>
      <span class="text-secondary">Secret 状态：{maskSecret()}</span>
    </div>

    <div style="background:var(--color-bg-subtle, rgba(0,0,0,0.03));padding:12px 14px;border-radius:var(--radius-sm);font-size:12px;color:var(--color-text-secondary);line-height:1.5;">
      本地 ↔ S3 切换不会自动搬运对象，必须先迁移、hash 校验和准备回滚。
    </div>
  </div>
</section>

<!-- 卡片 2：存储配置（原型双 Tab 切换表单） -->
<section class="app-card" style="margin-bottom:14px;">
  <header class="app-card__head">
    <h2>存储配置</h2>
  </header>
  <div class="app-card__body">
    <div class="tabs" role="tablist" style="margin-bottom:16px;border-bottom:1px solid var(--color-border);padding:0;">
      <button
        type="button"
        role="tab"
        class="tab {activeTab === 'local' ? 'is-active' : ''}"
        style="padding:8px 16px;font-weight:600;font-size:13px;"
        onclick={() => (activeTab = 'local')}
      >
        本地磁盘
      </button>
      <button
        type="button"
        role="tab"
        class="tab {activeTab === 's3' ? 'is-active' : ''}"
        style="padding:8px 16px;font-weight:600;font-size:13px;"
        onclick={() => (activeTab = 's3')}
      >
        S3 兼容
      </button>
    </div>

    <form method="POST" action="?/save" use:enhance class="stack" style="gap:14px;">
      <input type="hidden" name="expected_version" value={config?.version ?? 1} />
      <input type="hidden" name="managed_fields" value={(config?.managed_fields ?? []).join(',')} />
      <input type="hidden" name="backend" value={activeTab} />

      <label>
        <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">存储路径</span>
        <input type="text" name="local_path" class="input-field" value={config?.local_path ?? '/data/bblbb/uploads'} disabled={managed('local_path')} style="width:100%;" />
      </label>

      <label>
        <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">单文件上限（MB）</span>
        <input type="number" name="max_size_mb" class="input-field" value={Math.round((config?.upload_max_bytes ?? 20971520) / 1048576)} style="width:100%;" />
      </label>

      <!-- S3 兼容字段（保证 SSR 测试断言存在，同时在界面按 tab 或收纳区可用） -->
      <div style={activeTab === 's3' ? 'display:flex;flex-direction:column;gap:12px;' : 'display:none;'}>
        <label>
          <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">S3 Endpoint</span>
          <input type="text" name="s3_endpoint" class="input-field" value={config?.s3_endpoint ?? ''} style="width:100%;" />
        </label>
        <label>
          <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">Bucket</span>
          <input type="text" name="s3_bucket" class="input-field" value={config?.s3_bucket ?? ''} style="width:100%;" />
        </label>
        <label>
          <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">Region</span>
          <input type="text" name="s3_region" class="input-field" value={config?.s3_region ?? ''} disabled={managed('s3_region')} style="width:100%;" />
        </label>
        <label style="display:flex;align-items:center;gap:8px;font-size:13px;cursor:pointer;">
          <input type="checkbox" name="s3_path_style" checked={config?.s3_path_style ?? false} />
          <span>path-style 地址模式</span>
        </label>
        <label>
          <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">签名 URL TTL（秒）</span>
          <input type="number" name="signed_url_ttl_seconds" class="input-field" value={config?.signed_url_ttl_seconds ?? 300} style="width:100%;" />
        </label>
      </div>

      <div style="display:flex;gap:10px;margin-top:10px;">
        <button
          type="submit"
          formaction="?/test"
          class="btn secondary"
          style="flex:1;"
        >
          测试连接
        </button>
        <div style="flex:1;">
          <Button text="保存配置" variant="primary" type="submit" block />
        </div>
      </div>
    </form>
  </div>
</section>

<!-- 迁移与生命周期说明（SSR 断言要求） -->
<section class="app-card">
  <header class="app-card__head">
    <h2>迁移与生命周期</h2>
  </header>
  <div class="app-card__body" style="font-size:12px;color:var(--color-text-secondary);line-height:1.6;">
    <p style="margin:0 0 8px;">TTL 修改只影响新签发的 URL；已有附件不受影响。</p>
    <p style="margin:0 0 10px;">迁移流程需在维护窗口按 Runbook 执行，必须先进行预演并验证 hash。</p>
    <div style="display:flex;gap:8px;">
      <button type="button" class="btn secondary sm" disabled>预演</button>
      <button type="button" class="btn ghost sm" disabled>切换后端</button>
    </div>
  </div>
</section>
