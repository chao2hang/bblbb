<!-- M06-UI-06/07：管理端存储配置——local/S3、path-style、TTL、测试连接、
  脱敏状态（Secret 用 ••• 掩码、不进 DOM）。迁移按钮禁用并提示需预演/hash/
  回滚（OPERATIONS.md）。
-->
<script lang="ts">
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
</script>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <a href="/admin" class="breadcrumb-link">管理后台</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">存储管理</span>
  </nav>

  {#if loadError}
    <p class="input-hint is-error" role="alert">{loadError}</p>
  {/if}
  {#if message}
    <p class="input-hint is-error" role="alert">{message}</p>
  {/if}

  {#if config}
    <div class="card" style="margin-bottom:var(--space-4);">
      <div class="card-body" style="display:flex;flex-wrap:wrap;gap:var(--space-4);align-items:center;">
        <span class="badge badge-neutral">后端：{config.backend === 's3' ? 'S3 兼容' : '本地磁盘'}</span>
        <span class="badge {config.source === 'env' ? 'badge-warning' : 'badge-success'}">
          配置来源：{config.source === 'env' ? '部署环境（只读）' : '后台数据库'}
        </span>
        <span class="text-secondary" style="font-size:var(--text-sm);">
          Secret 状态：<span aria-label="已配置">{maskSecret()}</span>
        </span>
      </div>
    </div>

    <div class="card" style="margin-bottom:var(--space-4);">
      <div class="card-header"><span class="card-title">存储配置（v{config.version}）</span></div>
      <div class="card-body">
        <form method="POST" action="?/save" use:enhance>
          <input type="hidden" name="expected_version" value={config.version} />
          <input type="hidden" name="managed_fields" value={(config.managed_fields ?? []).join(',')} />
          <div class="admin-form-grid" style="display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:var(--space-2);">
            <div class="input-wrapper">
              <label class="input-label" for="cfg-backend">后端类型</label>
              <select id="cfg-backend" name="backend" class="input-field" disabled={managed('backend')}>
                <option value="local" selected={config.backend === 'local'}>本地磁盘</option>
                <option value="s3" selected={config.backend === 's3'}>S3 兼容（S3/MinIO/R2）</option>
              </select>
              {#if managed('backend')}
                <p class="input-hint">由部署配置管理，后台只读</p>
              {/if}
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="cfg-path">本地存储路径</label>
              <input id="cfg-path" name="local_path" class="input-field" value={config.local_path ?? ''} disabled={managed('local_path')} />
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="cfg-endpoint">S3 Endpoint</label>
              <input id="cfg-endpoint" name="s3_endpoint" class="input-field" value={config.s3_endpoint ?? ''} disabled={managed('s3_endpoint')} />
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="cfg-region">Region（可 auto）</label>
              <input id="cfg-region" name="s3_region" class="input-field" value={config.s3_region ?? ''} disabled={managed('s3_region')} />
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="cfg-bucket">Bucket</label>
              <input id="cfg-bucket" name="s3_bucket" class="input-field" value={config.s3_bucket ?? ''} disabled={managed('s3_bucket')} />
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="cfg-ttl">签名 URL TTL（秒，建议 60–3600）</label>
              <input id="cfg-ttl" name="signed_url_ttl_seconds" type="number" min="60" max="86400" class="input-field" value={config.signed_url_ttl_seconds ?? ''} disabled={managed('signed_url_ttl_seconds')} />
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="cfg-max">站点上传硬上限（字节）</label>
              <input id="cfg-max" name="upload_max_bytes" type="number" min="0" class="input-field" value={config.upload_max_bytes ?? ''} disabled={managed('upload_max_bytes')} />
            </div>
            <div class="input-wrapper">
              <span class="input-label">Secret（只写不回显）</span>
              <input name="s3_secret_access_key" type="password" class="input-field" placeholder={config.secret_configured ? '••••••••••（留空保持不变）' : '输入新的 Secret'} autocomplete="new-password" />
            </div>
          </div>
          <div style="display:flex;gap:var(--space-4);margin-top:var(--space-2);">
            <label class="input-label" style="display:flex;align-items:center;gap:var(--space-1);">
              <input type="checkbox" name="s3_path_style" checked={config.s3_path_style === true} disabled={managed('s3_path_style')} />
              path-style 地址模式（MinIO 等）
            </label>
            <label class="input-label" style="display:flex;align-items:center;gap:var(--space-1);">
              <input type="checkbox" name="s3_presigned_uploads" checked={config.s3_presigned_uploads !== false} disabled={managed('s3_presigned_uploads')} />
              浏览器预签名直传
            </label>
          </div>
          <div class="input-wrapper" style="margin-top:var(--space-2);">
            <label class="input-label" for="cfg-reason">操作原因</label>
            <input id="cfg-reason" name="reason" class="input-field" required placeholder="必填（写审计）" />
          </div>
          <div style="display:flex;gap:var(--space-2);margin-top:var(--space-2);">
            <Button text="保存配置" variant="primary" size="sm" type="submit" />
            <Button text="测试连接（当前表单值）" variant="secondary" size="sm" type="submit" formaction="?/test" />
          </div>
        </form>
      </div>
    </div>

    {#if testResult}
      <div class="card" style="margin-bottom:var(--space-4);">
        <div class="card-body">
          <p class="input-hint {testResult.ok ? '' : 'is-error'}" role="status">
            测试结果：{testResult.ok ? '连接成功' : `连接失败（${testResult.code ?? '未知'}）`} —— {testResult.message}
            {#if typeof testResult.elapsed_ms === 'number'}
              （{testResult.elapsed_ms} ms）
            {/if}
          </p>
        </div>
      </div>
    {/if}

    <div class="card">
      <div class="card-header"><span class="card-title">迁移与生命周期（只读说明）</span></div>
      <div class="card-body">
        <ul class="auth-hint" style="margin:0;padding-left:var(--space-4);display:flex;flex-direction:column;gap:var(--space-2);">
          <li><b>TTL 修改只影响新签发的 URL</b>：已有附件对象与旧链接不受影响；URL 到期只使链接失效，不删除附件、不释放容量。</li>
          <li>切换存储后端只保存候选配置，<b>不会自动迁移已有对象</b>。正式切换必须执行预演（只读校验）→ 复制 + hash 校验 → 切换 → 回滚演练（OPERATIONS.md）。</li>
        </ul>
        <div style="margin-top:var(--space-3);display:flex;gap:var(--space-2);">
          <Button text="预演（迁移前置步骤）" variant="secondary" size="sm" type="button" disabled />
          <Button text="切换后端" variant="ghost" size="sm" type="button" disabled />
          <span class="text-secondary" style="font-size:var(--text-sm);align-self:center;">迁移流程需在维护窗口按 Runbook 执行</span>
        </div>
      </div>
    </div>
  {:else if !loadError}
    <p class="input-hint" role="status">加载中…</p>
  {/if}
</div>
