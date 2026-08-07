<script lang="ts">
  // M09-UI-01/03：AI 能力与同意管理页。
  // - 默认关闭（Feature Flag 未启用）→ 关闭说明，不承诺功能可用；
  // - Provider 只展示脱敏状态（Secret 仅布尔，不回显）；
  // - 同意记录展示版本与撤回入口（原生表单，无 JS 可提交）；
  // - 处理中/取消状态由任务页与编辑器面板承载。
  import { aiPurposeLabel, aiDataModeLabel } from '$lib/api/client';
  import type { AiPageActionData, AiPageData } from './+page.server';

  let { data, form }: { data: AiPageData; form?: AiPageActionData | null } = $props();

  const state = $derived(data.state);
  const caps = $derived(data.capabilities);
  const disabledMessage = $derived(data.disabledMessage);
  const error = $derived(data.error);
  const message = $derived(form?.message ?? null);

  const purposes = $derived(caps?.purposes ?? []);
  const providers = $derived(caps?.providers ?? []);
  const consents = $derived((caps?.consents ?? []).filter((c) => !c.revoked_at));
  const revoked = $derived((caps?.consents ?? []).filter((c) => Boolean(c.revoked_at)));
</script>

<svelte:head>
  <title>AI 能力与同意 — BBLBB</title>
  <meta name="description" content="查看 AI 能力状态与管理你的 AI 数据发送同意" />
  <meta name="robots" content="noindex,follow" />
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">AI 能力与同意</span>
  </nav>

  {#if message}
    <p class="input-hint is-error" role="alert">{message}</p>
  {/if}
  {#if error}
    <p class="input-hint is-error" role="alert">{error}</p>
  {/if}

  {#if state === 'disabled'}
    <div class="card">
      <div class="card-header"><span class="card-title">AI 功能未开放</span></div>
      <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
        <p style="margin:0;">{disabledMessage ?? 'AI 能力当前未开放（默认关闭）。'}</p>
        <p class="input-hint" style="margin:0;">
          未启用时，你的正文不会发送给任何外部 AI 提供商；普通发帖、编辑与人工审核不受影响。
          若页面显示错误，可
          <a href="/ai" class="text-link">刷新</a>重试。
        </p>
      </div>
    </div>
  {:else if state === 'forbidden'}
    <div class="card">
      <div class="card-body">
        <p class="input-hint is-error" role="alert" style="margin:0;">你没有权限访问 AI 能力状态。</p>
      </div>
    </div>
  {:else if caps}
    <div class="card" style="margin-bottom:var(--space-4);">
      <div class="card-header"><span class="card-title">能力状态</span></div>
      <div class="card-body" style="display:flex;flex-wrap:wrap;gap:var(--space-4);align-items:center;">
        <span class="badge badge-success">已启用</span>
        <span class="text-secondary">数据模式：{aiDataModeLabel(caps.data_mode)}</span>
        {#if caps.synchronous}
          <span class="badge badge-neutral">支持同步建议</span>
        {/if}
        {#if caps.admin_forbidden}
          <span class="badge badge-warning">管理员策略限制中</span>
        {/if}
      </div>
    </div>

    <div class="card" style="margin-bottom:var(--space-4);">
      <div class="card-header"><span class="card-title">提供商（脱敏状态）</span></div>
      <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
        {#if providers.length === 0}
          <p class="input-hint" style="margin:0;">尚未配置可用的 AI 提供商。</p>
        {:else}
          {#each providers as provider (provider.id)}
            <div style="border:var(--border-default);border-radius:var(--radius-md);padding:var(--space-3);">
              <div style="display:flex;flex-wrap:wrap;gap:var(--space-2);align-items:center;">
                <strong>{provider.name ?? '未命名提供商'}</strong>
                {#if provider.model}
                  <span class="badge badge-neutral">{provider.model}</span>
                {/if}
                <span class="badge {provider.secret_configured ? 'badge-success' : 'badge-warning'}">
                  {provider.secret_configured ? '密钥已配置' : '密钥未配置'}
                </span>
                <span class="badge {provider.available === false ? 'badge-warning' : 'badge-success'}">
                  {provider.available === false ? '不可用' : '可用'}
                </span>
              </div>
              {#if provider.purposes && provider.purposes.length > 0}
                <p class="input-hint" style="margin:var(--space-1) 0 0;">用途：{provider.purposes.map(aiPurposeLabel).join('、')}</p>
              {/if}
              {#if provider.retention || provider.training || provider.region}
                <p class="input-hint" style="margin:var(--space-1) 0 0;">
                  {provider.retention ? `留存：${provider.retention}` : ''}
                  {provider.training ? ` · 训练：${provider.training}` : ''}
                  {provider.region ? ` · 区域：${provider.region}` : ''}
                </p>
              {/if}
            </div>
          {/each}
        {/if}
        <p class="input-hint" style="margin:0;">密钥只保存在受保护的 Secret Store，任何页面都不会显示明文或片段。</p>
      </div>
    </div>

    <div class="card" style="margin-bottom:var(--space-4);">
      <div class="card-header"><span class="card-title">我的同意记录</span></div>
      <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
        {#if consents.length === 0 && revoked.length === 0}
          <p class="input-hint" style="margin:0;">尚无 AI 数据发送同意记录。每次正文外发前都会展示完整披露并由你明确确认。</p>
        {:else}
          {#each consents as consent (consent.provider_id + consent.purpose)}
            <div style="display:flex;flex-wrap:wrap;gap:var(--space-2);align-items:center;border:var(--border-default);border-radius:var(--radius-md);padding:var(--space-2);">
              <span class="badge badge-success">已同意</span>
              <span class="text-secondary" style="font-size:var(--text-sm);">
                {aiPurposeLabel(consent.purpose)} · {consent.provider_name ?? consent.provider_id} · v{consent.disclosure_version}
              </span>
              {#if consent.disclosure_hash}
                <span class="text-secondary" style="font-size:var(--text-xs);">hash {consent.disclosure_hash}</span>
              {/if}
              <form method="POST" action="?/revoke" style="margin-left:auto;">
                <input type="hidden" name="provider_id" value={consent.provider_id} />
                <input type="hidden" name="purpose" value={consent.purpose} />
                <input type="hidden" name="disclosure_version" value={consent.disclosure_version} />
                <input type="hidden" name="disclosure_hash" value={consent.disclosure_hash ?? ''} />
                <button type="submit" class="btn btn-ghost btn-sm">撤回同意</button>
              </form>
            </div>
          {/each}
          {#if revoked.length > 0}
            <p class="input-hint" style="margin:0;">已撤回：{revoked.map((c) => aiPurposeLabel(c.purpose)).join('、')}。撤回后不再发起新的 AI 任务，排队任务将被取消。</p>
          {/if}
        {/if}
      </div>
    </div>

    <div class="card">
      <div class="card-header"><span class="card-title">AI 边界说明</span></div>
      <div class="card-body">
        <ul style="margin:0;padding-left:var(--space-4);display:flex;flex-direction:column;gap:var(--space-2);">
          <li>模型不能自动发布/拒绝内容、封禁用户或修改权限；结果只是版本化建议，必须由你或审核人员手动采纳。</li>
          <li>隐藏正文默认不发送外部模型；审核建议只对授权审核人员可见。</li>
          <li>AI 故障、关闭或撤回同意不影响普通发帖与人工审核。</li>
        </ul>
      </div>
    </div>
  {/if}
</div>
