<script lang="ts">
  // M09-UI-02/03：正文外发前的同意面板。
  //
  // 每次把正文发送给外部 AI Provider 前，必须展示完整披露（Provider、用途、
  // 留存、训练使用、区域、数据模式）并取得明确确认（checkbox + 确认按钮）；
  // 确认携带披露版本与文案 hash 作为同意证据（docs/AI.md §5）。
  // 已同意时展示同意版本并允许撤回；处理中/撤回后给出对应状态。
  import Button from '$lib/components/ui/Button.svelte';
  import { aiPurposeLabel, aiDataModeLabel } from '$lib/api/client';
  import type { AiConsentInput, AiConsentView, AiProviderStatus } from '$lib/api/types';

  let {
    purpose,
    providers = null,
    dataMode = null,
    disclosureText,
    disclosureVersion,
    disclosureHashValue,
    existingConsent = null,
    processing = false,
    onConfirm,
    onCancel,
    onRevoke = null
  }: {
    purpose: string;
    providers?: AiProviderStatus[] | null;
    dataMode?: string | null;
    disclosureText: string;
    disclosureVersion: number;
    disclosureHashValue: string;
    existingConsent?: AiConsentView | null;
    processing?: boolean;
    onConfirm: (input: AiConsentInput) => void;
    onCancel: () => void;
    onRevoke?: ((input: AiConsentInput) => void) | null;
  } = $props();

  let acknowledged = $state(false);

  /** 选择的 Provider（面板默认第一个已配置 Provider）。 */
  const provider = $derived(providers?.find((p) => p.available !== false) ?? providers?.[0] ?? null);
  const providerId = $derived(provider?.id ?? '');

  function consentInput(): AiConsentInput {
    return {
      provider_id: providerId,
      purpose,
      data_mode: 'full_with_consent',
      disclosure_version: disclosureVersion,
      disclosure_hash: disclosureHashValue
    };
  }

  function confirm() {
    if (!acknowledged || processing) return;
    onConfirm(consentInput());
  }

  function revoke() {
    if (!onRevoke || processing) return;
    onRevoke(consentInput());
  }
</script>

<div class="card" role="region" aria-label="AI 数据发送同意" style="border-color:var(--color-warning);margin:var(--space-3) 0;">
  <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
    {#if existingConsent && !existingConsent.revoked_at}
      <div style="display:flex;flex-wrap:wrap;gap:var(--space-2);align-items:center;">
        <span class="badge badge-success">已同意</span>
        <span class="text-secondary" style="font-size:var(--text-sm);">
          {aiPurposeLabel(existingConsent.purpose)} · v{existingConsent.disclosure_version}
          {#if existingConsent.provider_name} · {existingConsent.provider_name}{/if}
        </span>
      </div>
      <p class="input-hint" style="margin:0;">你的同意记录已保存（含当时展示的披露版本与文案 hash）。撤回后不再发起新的 AI 任务，排队任务将被取消。</p>
      {#if onRevoke}
        <div>
          <Button text="撤回同意" variant="danger" size="sm" onclick={revoke} disabled={processing} />
        </div>
      {/if}
    {:else}
      <strong>发送前请阅读并确认</strong>
      <pre class="ai-disclosure" style="white-space:pre-wrap;font-family:inherit;margin:0;padding:var(--space-2);background:var(--color-bg-subtle, rgba(0,0,0,0.04));border-radius:var(--radius-md);font-size:var(--text-sm);">{disclosureText}</pre>
      <p class="input-hint" style="margin:0;">
        数据模式：{aiDataModeLabel(dataMode)} · 披露版本 v{disclosureVersion}（hash {disclosureHashValue}）。
        发送内容仅用于本次建议，不会自动处罚或修改权限。
      </p>
      <label style="display:flex;align-items:flex-start;gap:var(--space-2);" for="ai-consent-ack">
        <input
          id="ai-consent-ack"
          type="checkbox"
          bind:checked={acknowledged}
          disabled={processing}
        />
        <span>我已知晓并同意将上述内容发送给 AI 提供商用于本次{aiPurposeLabel(purpose)}（每次正文外发均需确认）</span>
      </label>
      <div style="display:flex;gap:var(--space-2);">
        <Button
          text={processing ? '处理中…' : '同意并继续'}
          variant="primary"
          size="sm"
          onclick={confirm}
          disabled={!acknowledged || processing}
        />
        <Button text="取消" variant="ghost" size="sm" onclick={onCancel} disabled={processing} />
      </div>
    {/if}
  </div>
</div>
