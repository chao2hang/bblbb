<script lang="ts">
  // M06-UI-06/07 & M18-ADMIN-STORAGE：管理端存储配置（兼具原型 2x2 卡片视觉与 SSR 契约断言）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { untrack } from 'svelte';
  import { enhance } from '$app/forms';
  import type { ActionResult } from '@sveltejs/kit';
  import Button from '$lib/components/ui/Button.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import type { AdminStorageActionData, AdminStoragePageData } from './+page.server';

  let { data, form }: { data: AdminStoragePageData; form?: AdminStorageActionData | null } = $props();

  const config = $derived(data.config);
  const loadError = $derived(data.loadError);
  const message = $derived(form?.message ?? null);
  const messageKind = $derived(form?.messageKind ?? 'error');
  const testResult = $derived(form?.testResult ?? null);

  function managed(key: string): boolean {
    return Boolean(config?.managed_fields?.includes(key));
  }

  function maskSecret(): string {
    return config?.secret_configured ? '••••••••••' : '未配置';
  }

  // 初始 Tab 跟随当前生效后端（SSR 即确定，无需 effect）。
  let activeTab = $state<'local' | 's3'>(untrack(() => data.config?.backend === 's3' ? 's3' : 'local'));

  // 表单草稿：组件挂载时从脱敏配置播种一次。提交成功后的 invalidateAll
  // 不会改动草稿——否则 load 重跑会把 S3 字段重置为环境值，表现为
  // “保存/测试后刚填的配置被清空”。
  let draft = $state(untrack(() => ({
    local_path: config?.local_path ?? '/data/bblbb/uploads',
    max_size_mb: Math.round((config?.upload_max_bytes ?? 20971520) / 1048576),
    s3_endpoint: config?.s3_endpoint ?? '',
    s3_bucket: config?.s3_bucket ?? '',
    s3_public_base_url: config?.s3_public_base_url ?? '',
    s3_region: config?.s3_region ?? '',
    s3_path_style: config?.s3_path_style ?? false,
    signed_url_ttl_seconds: config?.signed_url_ttl_seconds ?? 300
  })));

  let reauthLoading = $state(false);
  let reauthCancelled = $state(false);
  let reauthError = $state<string | null>(null);

  // JS 启用：动作结果走全局 Toast 浮窗（成功绿/失败红，测试连接附诊断详情）；
  // 顶部内联横幅仅保留为无 JS 回退（SSR HTML 仍渲染，见 hasJs）。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  // 提交后保留用户输入：默认 enhance 会在成功后 reset 表单（清掉刚填的
  // S3 凭据/路径）；这里改为 update({ reset: false })，仅应用 action 结果；
  // 结果同时用全局 Toast 提示（产品约定：提醒用浮窗，不占页面主体）。
  function storageFormEnhance() {
    return async ({
      result,
      update
    }: {
      result: ActionResult;
      update: (opts?: { reset?: boolean }) => Promise<void>;
    }) => {
      if (result.type === 'success' || result.type === 'failure') {
        const d = result.data as AdminStorageActionData | null;
        if (d?.testResult) {
          const tr = d.testResult;
          const summary = `${tr.ok ? '连接成功' : '连接失败'} · 后端 ${tr.backend ?? '—'} · 耗时 ${tr.elapsed_ms ?? 0}ms`;
          const tips =
            !tr.ok && (tr.message.includes('dispatch failure') || tr.message.includes('dns/tls'))
              ? '\n排查建议：检查 Endpoint 地址/端口可达性、本地 MinIO 是否启动、path-style 勾选、http(s) 协议。'
              : '';
          const detail = tr.message
            ? `${tr.message}${tr.error_class && tr.error_class !== 'ok' ? `（${tr.error_class}）` : ''}${tips}`
            : tips || undefined;
          showToast(summary, tr.ok ? 'success' : 'danger', 6500, detail);
        } else if (d?.message) {
          showToast(d.message, d.messageKind === 'success' ? 'success' : 'danger');
        }
      }
      await update({ reset: false });
    };
  }
</script>

<svelte:head>
  <title>文件存储 — BBLBB</title>
</svelte:head>

<PageHeader title="文件存储" />

{#if loadError}
  <p class="input-hint has-error" role="alert">{loadError}</p>
{/if}
{#if message && !hasJs}
  <p
    class="input-hint {messageKind === 'success' ? '' : 'has-error'}"
    role={messageKind === 'success' ? 'status' : 'alert'}
    style={messageKind === 'success' ? 'color:var(--color-success);' : ''}
  >
    {message}
  </p>
{/if}

{#if testResult && !hasJs}
  <div
    class="input-hint {testResult.ok ? '' : 'has-error'}"
    role="status"
    style="margin-bottom:12px;padding:10px 12px;border:1px solid var(--color-border);border-radius:var(--radius-sm);{testResult.ok ? 'color:var(--color-success);' : ''}"
  >
    {testResult.ok ? '连接成功' : '连接失败'} · 后端 {testResult.backend ?? '—'} · 耗时 {testResult.elapsed_ms ?? 0}ms
    {#if testResult.message}
      <div class="text-secondary" style="font-size:12px;margin-top:4px;">
        {testResult.message}{#if testResult.error_class && testResult.error_class !== 'ok'}（{testResult.error_class}）{/if}
      </div>
      {#if testResult.message.includes('dispatch failure') || testResult.message.includes('dns/tls')}
        <div class="app-muted" style="font-size:11px;margin-top:6px;line-height:1.4;color:var(--color-text-secondary);">
          排查建议：无法建立到 S3 Endpoint 的网络连接。请检查：1) Endpoint 地址与端口是否正确且可达；2) 本地 MinIO 服务是否已启动；3) MinIO 或兼容对象存储请尝试勾选「path-style 地址模式」；4) 协议是否应为 http:// 或 https://。
        </div>
      {/if}
    {/if}
  </div>
{/if}

<!-- step-up 重新验证（M02-MFA-07）：save/test 命中 403 step_up_required 时展示 -->
{#if form?.stepUpRequired && !reauthCancelled}
  <section class="app-card" style="margin-bottom:14px;border-left:3px solid var(--color-warning);">
    <header class="app-card__head" style="display:flex;justify-content:space-between;align-items:center;">
      <h2>需要重新验证身份</h2>
      <button type="button" class="btn ghost sm" onclick={() => (reauthCancelled = true)}>取消</button>
    </header>
    <div class="app-card__body">
      <p class="text-secondary" style="font-size:12px;margin:0 0 10px;line-height:1.6;">
        存储配置的保存与测试属于高风险管理操作，要求近期重新认证（登录已超过有效期）。
        输入当前账号密码完成重新验证后，将保留当前表单并可继续保存。
      </p>
      {#if reauthError}
        <div class="alert alert-danger" role="alert" style="margin-bottom:10px;padding:8px 12px;font-size:12px;">
          {reauthError}
        </div>
      {/if}
      <form
        method="POST"
        action="?/reauth"
        use:enhance={() => {
          reauthLoading = true;
          reauthError = null;
          return async ({ result, update }) => {
            reauthLoading = false;
            if (result.type === 'success') {
              showToast((result.data as { message?: string } | null)?.message ?? '身份重新验证成功，请继续保存设置或测试连接', 'success');
            } else if (result.type === 'failure') {
              reauthError = (result.data as any)?.message ?? '密码验证失败，请重试';
            }
            await update({ reset: false });
          };
        }}
        class="stack"
        style="gap:10px;"
      >
        <label>
          <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">当前密码</span>
          <input
            type="password"
            name="password"
            class="input-field"
            required
            autocomplete="current-password"
            style="width:100%;"
          />
        </label>
        <div style="display:flex;gap:10px;align-items:center;">
          <Button text={reauthLoading ? '验证中…' : '重新验证'} variant="primary" type="submit" disabled={reauthLoading} />
          <button type="button" class="btn ghost sm" onclick={() => (reauthCancelled = true)}>取消</button>
        </div>
      </form>
    </div>
  </section>
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
    <div class="tabs" role="tablist" aria-label="存储后端切换" style="margin-bottom:16px;border-bottom:1px solid var(--color-border);padding:0;">
      <button
        type="button"
        role="tab"
        id="tab-storage-local"
        aria-controls="panel-storage-local"
        aria-selected={activeTab === 'local'}
        tabindex={activeTab === 'local' ? 0 : -1}
        class="tab {activeTab === 'local' ? 'is-active' : ''}"
        style="padding:8px 16px;font-weight:600;font-size:13px;"
        onclick={() => (activeTab = 'local')}
      >
        本地磁盘
      </button>
      <button
        type="button"
        role="tab"
        id="tab-storage-s3"
        aria-controls="panel-storage-s3"
        aria-selected={activeTab === 's3'}
        tabindex={activeTab === 's3' ? 0 : -1}
        class="tab {activeTab === 's3' ? 'is-active' : ''}"
        style="padding:8px 16px;font-weight:600;font-size:13px;"
        onclick={() => (activeTab = 's3')}
      >
        S3 兼容
      </button>
    </div>

    <form method="POST" action="?/save" use:enhance={storageFormEnhance()} class="stack" style="gap:14px;">
      <input type="hidden" name="expected_version" value={config?.version ?? 1} />
      <input type="hidden" name="managed_fields" value={(config?.managed_fields ?? []).join(',')} />
      <input type="hidden" name="backend" value={activeTab} />

      <!-- 本地磁盘面板（非激活 Tab 时 disabled：隐藏面板字段不参与提交，
           避免与 S3 面板同名字段（max_size_mb）相互遮蔽） -->
      <div
        id="panel-storage-local"
        role="tabpanel"
        aria-labelledby="tab-storage-local"
        hidden={activeTab !== 'local'}
        tabindex={activeTab === 'local' ? 0 : -1}
      >
        <fieldset
          disabled={activeTab !== 'local'}
          style="border:0;margin:0;padding:0;{activeTab === 'local' ? 'display:flex;flex-direction:column;gap:12px;' : 'display:none;'}"
        >
          <label>
            <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">本地存储路径</span>
            <input
              type="text"
              name="local_path"
              class="input-field"
              value={draft.local_path}
              disabled={managed('local_path')}
              style="width:100%;"
            />
            <span class="text-secondary" style="font-size:12px;margin-top:4px;display:block;">本地磁盘对象存储根目录，后端进程需具备读写权限。</span>
          </label>

          <label>
            <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">单文件上限（MB）</span>
            <input
              type="number"
              name="max_size_mb"
              class="input-field"
              value={draft.max_size_mb}
              style="width:100%;"
            />
          </label>
        </fieldset>
      </div>

      <!-- S3 兼容字段（路径、密钥与服务端点） -->
      <div
        id="panel-storage-s3"
        role="tabpanel"
        aria-labelledby="tab-storage-s3"
        hidden={activeTab !== 's3'}
        tabindex={activeTab === 's3' ? 0 : -1}
      >
        <fieldset
          disabled={activeTab !== 's3'}
          style="border:0;margin:0;padding:0;{activeTab === 's3' ? 'display:flex;flex-direction:column;gap:12px;' : 'display:none;'}"
        >
        <label>
          <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">S3 Endpoint</span>
          <input
            type="text"
            name="s3_endpoint"
            class="input-field"
            placeholder="https://s3.amazonaws.com 或 MinIO/OSS/COS 域名"
            value={draft.s3_endpoint}
            disabled={managed('s3_endpoint')}
            style="width:100%;"
          />
        </label>

        <label>
          <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">Bucket</span>
          <input
            type="text"
            name="s3_bucket"
            class="input-field"
            placeholder="例如 bblbb-attachments"
            value={draft.s3_bucket}
            disabled={managed('s3_bucket')}
            style="width:100%;"
          />
        </label>

        <label>
          <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">访问路径 / 公网基准 URL</span>
          <input
            type="text"
            name="s3_public_base_url"
            class="input-field"
            placeholder="https://cdn.example.com/uploads（留空默认使用 Endpoint 地址）"
            value={draft.s3_public_base_url}
            disabled={managed('s3_public_base_url')}
            style="width:100%;"
          />
          <span class="text-secondary" style="font-size:12px;margin-top:4px;display:block;">可配置 CDN 或反向代理的公网访问路径；留空时默认使用 Endpoint 地址。</span>
        </label>

        <label>
          <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">AccessKey ID（访问密钥）</span>
          <input
            type="text"
            name="s3_access_key_id"
            class="input-field"
            placeholder="输入 AccessKey ID（留空保持不变）"
            disabled={managed('s3_access_key_id')}
            style="width:100%;"
          />
        </label>

        <label>
          <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">Secret AccessKey（私有密钥）</span>
          <input
            type="password"
            name="s3_secret_access_key"
            class="input-field"
            placeholder={config?.secret_configured ? '••••••••••（已配置，留空表示保持不变）' : '输入 Secret AccessKey'}
            autocomplete="new-password"
            disabled={managed('s3_secret_access_key')}
            style="width:100%;"
          />
          <span class="text-secondary" style="font-size:12px;margin-top:4px;display:block;">已配置的 Secret 不回显；留空表示保持不变。</span>
        </label>

        <label>
          <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">Region</span>
          <input
            type="text"
            name="s3_region"
            class="input-field"
            placeholder="例如 ap-southeast-1 或 auto"
            value={draft.s3_region}
            disabled={managed('s3_region')}
            style="width:100%;"
          />
        </label>

        <label style="display:flex;align-items:center;gap:8px;font-size:13px;cursor:pointer;">
          <input type="checkbox" name="s3_path_style" bind:checked={draft.s3_path_style} />
          <span>path-style 地址模式</span>
        </label>

        <label>
          <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">签名 URL TTL（秒）</span>
          <input
            type="number"
            name="signed_url_ttl_seconds"
            class="input-field"
            value={draft.signed_url_ttl_seconds}
            style="width:100%;"
          />
        </label>

        <label>
          <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">单文件上限（MB）</span>
          <input
            type="number"
            name="max_size_mb"
            class="input-field"
            value={draft.max_size_mb}
            style="width:100%;"
          />
        </label>
      </fieldset>
    </div>

      <label style="margin-top:4px;">
        <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">
          操作原因 <span class="text-secondary" style="font-weight:normal;">(必填，写入管理审计日志)</span>
        </span>
        <input
          type="text"
          name="reason"
          class="input-field"
          required
          placeholder="例如：配置并验证 S3 存储桶凭据与路径"
          style="width:100%;"
        />
      </label>

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
