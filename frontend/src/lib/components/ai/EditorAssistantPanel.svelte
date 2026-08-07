<script lang="ts">
  // M09-UI-02/03/04/07：编辑器 AI 辅助面板。
  //
  // - 能力声明（/ai/capabilities）未启用/未实现/Feature Flag 关闭 → 展示
  //   「AI 辅助未开放，发布与编辑不受影响」（M09-UI-07 降级）；
  // - 正文外发前展示完整同意并确认（ConsentPanel），同意版本变更需重新同意；
  // - 任务排队/处理中可取消（M09-UI-03）；
  // - 结果只以 diff 预览呈现，字段级采纳（M09-UI-04），采纳用
  //   expected_base_version/If-Match，409 版本冲突提示重载；
  // - AI 故障/关闭/撤回永不阻塞普通发帖（错误只影响本面板）。
  import { onDestroy, onMount } from 'svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import ConsentPanel from './ConsentPanel.svelte';
  import {
    getAiCapabilities,
    grantAiConsent,
    revokeAiConsent,
    requestDraftFormat,
    getAiTask,
    cancelAiTask,
    getAiSuggestion,
    acceptAiSuggestion,
    newClientRequestId,
    aiPurposeLabel
  } from '$lib/api/client';
  import { buildDisclosureText, disclosureHash, AI_DISCLOSURE_VERSION } from '$lib/ai/consent';
  import { renderTextDiff, hasDiff } from '$lib/ai/diff';
  import type {
    AiCapabilities,
    AiConsentInput,
    AiSuggestion,
    AiSuggestionField,
    AiTask
  } from '$lib/api/types';

  let {
    draftId,
    title,
    markdown,
    onApplyField,
    onEnsureDraft
  }: {
    draftId: string | null;
    title: string;
    markdown: string;
    onApplyField: (field: string, value: string) => void;
    onEnsureDraft: () => Promise<string | null>;
  } = $props();

  type FeatureState = 'loading' | 'disabled' | 'enabled';
  type Phase = 'idle' | 'consenting' | 'granting' | 'processing' | 'result' | 'error';

  const PURPOSE = 'formatting';
  const POLL_INTERVAL_MS = 2500;

  let capabilities = $state<AiCapabilities | null>(null);
  let feature = $state<FeatureState>('loading');
  let phase = $state<Phase>('idle');
  let task = $state<AiTask | null>(null);
  let suggestion = $state<AiSuggestion | null>(null);
  let notice = $state<string | null>(null);
  let error = $state<string | null>(null);
  let conflict = $state(false);
  let acceptedFields = $state<string[]>([]);
  let cancelled = $state(false);
  let disposed = false;
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  const currentConsent = $derived(
    capabilities?.consents?.find((c) => c.purpose === PURPOSE && !c.revoked_at) ?? null
  );
  const provider = $derived(
    capabilities?.providers?.find((p) => p.available !== false) ?? capabilities?.providers?.[0] ?? null
  );
  const disclosureText = $derived.by(() =>
    buildDisclosureText({
      providerName: provider?.name,
      purpose: PURPOSE,
      dataMode: capabilities?.data_mode,
      retention: provider?.retention,
      training: provider?.training,
      region: provider?.region
    })
  );
  const disclosureVersion = $derived(AI_DISCLOSURE_VERSION);
  const disclosureHashValue = $derived(disclosureHash(disclosureText));

  const canFormat = $derived(feature === 'enabled' && phase === 'idle' && markdown.trim().length > 0);

  onMount(async () => {
    const caps = await getAiCapabilities(fetch);
    if (disposed) return;
    capabilities = caps;
    feature = caps?.enabled ? 'enabled' : 'disabled';
    if (!caps?.enabled) {
      notice = 'AI 辅助未开放。发布与编辑不受影响。';
    }
  });

  onDestroy(() => {
    disposed = true;
    stopPolling();
  });

  function stopPolling() {
    if (pollTimer) {
      clearInterval(pollTimer);
      pollTimer = null;
    }
  }

  function findConsent(): boolean {
    return Boolean(currentConsent);
  }

  async function runFormat() {
    error = null;
    conflict = false;
    notice = null;
    if (feature !== 'enabled' || !capabilities) return;
    if (!markdown.trim()) {
      notice = '请先输入正文内容';
      return;
    }
    if (!findConsent()) {
      phase = 'consenting';
      return;
    }
    await startFormat();
  }

  async function onConsentConfirm(input: AiConsentInput) {
    phase = 'granting';
    error = null;
    try {
      await grantAiConsent(fetch, input);
    } catch (err) {
      const code = (err as { code?: string })?.code;
      if (code === 'feature_disabled') {
        feature = 'disabled';
        notice = 'AI 辅助未开放。发布与编辑不受影响。';
        phase = 'idle';
        return;
      }
      phase = 'error';
      error = '同意保存失败，请稍后重试';
      return;
    }
    await startFormat();
  }

  async function onConsentRevoke(input: AiConsentInput) {
    try {
      await revokeAiConsent(fetch, input);
      notice = '已撤回 AI 辅助同意；不再发起新的 AI 任务。';
    } catch {
      notice = '撤回失败，请稍后重试';
    }
    phase = 'idle';
  }

  async function startFormat() {
    phase = 'processing';
    error = null;
    notice = null;
    try {
      const id = await onEnsureDraft();
      if (!id) {
        phase = 'error';
        error = '请先保存草稿后再使用 AI 格式化';
        return;
      }
      const accepted = await requestDraftFormat(fetch, id, newClientRequestId());
      if (accepted.suggestion) {
        showSuggestion(accepted.suggestion);
        return;
      }
      await pollTask(accepted.task_id);
    } catch (err) {
      const code = (err as { code?: string })?.code;
      if (code === 'feature_disabled') {
        feature = 'disabled';
        notice = 'AI 辅助未开放。发布与编辑不受影响。';
      } else if (code === 'ai_consent_required') {
        phase = 'consenting';
        return;
      } else if (code === 'ai_budget_exceeded') {
        phase = 'error';
        error = 'AI 用量已超限，请稍后再试';
      } else {
        phase = 'error';
        error = 'AI 格式化暂不可用，请稍后重试（不影响正常发布）';
      }
    }
  }

  async function pollTask(taskId: string) {
    task = { id: taskId, task_type: PURPOSE, status: 'queued', created_at: Date.now() };
    stopPolling();
    pollTimer = setInterval(async () => {
      try {
        const t = await getAiTask(fetch, taskId);
        task = t;
        if (t.status === 'succeeded' && t.suggestion_id) {
          stopPolling();
          const s = await getAiSuggestion(fetch, t.suggestion_id);
          showSuggestion(s);
        } else if (t.status === 'dead') {
          stopPolling();
          phase = 'error';
          error = t.error_message ?? 'AI 任务失败，请稍后重试';
        } else if (t.status === 'cancelled') {
          stopPolling();
          cancelled = true;
          phase = 'idle';
          notice = '任务已取消';
        }
      } catch {
        // 轮询失败不阻塞发布；继续尝试直到手动取消。
      }
    }, POLL_INTERVAL_MS);
  }

  async function cancelTask() {
    if (!task) return;
    try {
      await cancelAiTask(fetch, task.id, newClientRequestId());
      notice = '已发送取消请求';
    } catch {
      notice = '取消失败，请稍后重试';
    }
  }

  function showSuggestion(s: AiSuggestion) {
    suggestion = s;
    task = null;
    stopPolling();
    phase = 'result';
    notice = '建议仅以 diff 预览呈现，请手动逐字段采纳';
  }

  async function adoptField(field: AiSuggestionField) {
    if (!suggestion) return;
    conflict = false;
    error = null;
    try {
      await acceptAiSuggestion(fetch, suggestion.id, {
        expected_base_version: suggestion.base_version,
        selected_fields: [field.field]
      });
      acceptedFields = [...acceptedFields, field.field];
      onApplyField(field.field, field.proposed);
      notice = `已采纳「${fieldLabel(field.field)}」`;
    } catch (err) {
      const code = (err as { code?: string })?.code;
      if (code === 'version_conflict' || code === 'ai_suggestion_stale') {
        conflict = true;
      } else {
        error = '采纳失败，请稍后重试';
      }
    }
  }

  async function reloadSuggestion() {
    if (!suggestion) return;
    conflict = false;
    try {
      const s = await getAiSuggestion(fetch, suggestion.id);
      if (s.base_version !== suggestion.base_version) {
        notice = '建议已基于新版本刷新';
        suggestion = s;
      } else {
        notice = '建议未变化';
      }
    } catch {
      error = '建议加载失败，请刷新页面';
    }
  }

  function dismissResult() {
    suggestion = null;
    acceptedFields = [];
    phase = 'idle';
    notice = null;
  }

  function fieldLabel(field: string): string {
    const map: Record<string, string> = {
      title: '标题',
      content: '正文',
      markdown: '正文',
      summary: '摘要',
      tags: '标签'
    };
    return map[field] ?? field;
  }

  function isAccepted(field: string): boolean {
    return acceptedFields.includes(field);
  }

  function diffLines(field: AiSuggestionField) {
    return renderTextDiff(field.current ?? '', field.proposed);
  }
</script>

<div class="card" aria-label="AI 辅助">
  <div class="card-header"><span class="card-title">AI 辅助</span></div>
  <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
    {#if feature === 'loading'}
      <p class="input-hint" role="status">正在检查 AI 能力…</p>
    {:else if feature === 'disabled'}
      <!-- M09-UI-07：AI 关闭/故障/撤回时普通发帖与编辑不受影响。 -->
      <p class="input-hint" role="status">{notice ?? 'AI 辅助未开放。发布与编辑不受影响。'}</p>
    {:else if phase === 'consenting' || phase === 'granting'}
      <ConsentPanel
        purpose={PURPOSE}
        providers={capabilities?.providers ?? null}
        dataMode={capabilities?.data_mode}
        disclosureText={disclosureText}
        disclosureVersion={disclosureVersion}
        disclosureHashValue={disclosureHashValue}
        processing={phase === 'granting'}
        onConfirm={onConsentConfirm}
        onCancel={() => (phase = 'idle')}
      />
    {:else if phase === 'processing' && task}
      <div style="display:flex;flex-wrap:wrap;gap:var(--space-2);align-items:center;" role="status">
        <span class="badge badge-neutral">任务 {task.status}</span>
        <span class="text-secondary" style="font-size:var(--text-sm);">
          {aiPurposeLabel(PURPOSE)}处理中…结果仅以 diff 预览呈现
        </span>
        <Button text="取消任务" variant="ghost" size="sm" onclick={cancelTask} />
      </div>
    {:else if phase === 'result' && suggestion}
      <div style="display:flex;flex-wrap:wrap;gap:var(--space-2);align-items:center;">
        <span class="badge badge-success">建议</span>
        <span class="text-secondary" style="font-size:var(--text-sm);">
          生成于 v{suggestion.base_version} · 手动采纳，不自动改写
        </span>
        <Button text="忽略建议" variant="ghost" size="sm" onclick={dismissResult} />
      </div>
      {#if conflict}
        <div class="alert alert-warning" role="alert" style="padding:var(--space-2);border:1px solid var(--color-warning);border-radius:var(--radius-md);">
          <p style="margin:0 0 var(--space-2);">内容已更新，建议已过期。加载最新建议后再采纳，避免覆盖新编辑。</p>
          <Button text="加载最新建议" variant="secondary" size="sm" onclick={reloadSuggestion} />
        </div>
      {/if}
      {#each suggestion.fields as field (field.field)}
        {#if hasDiff(field.current, field.proposed)}
          <div style="border:var(--border-default);border-radius:var(--radius-md);padding:var(--space-2);">
            <div style="display:flex;flex-wrap:wrap;gap:var(--space-2);align-items:center;margin-bottom:var(--space-1);">
              <strong style="font-size:var(--text-sm);">{fieldLabel(field.field)}</strong>
              {#if field.reason}
                <span class="text-secondary" style="font-size:var(--text-xs);">{field.reason}</span>
              {/if}
              <span style="margin-left:auto;">
                <Button
                  text={isAccepted(field.field) ? '已采纳' : '采纳此字段'}
                  variant={isAccepted(field.field) ? 'secondary' : 'primary'}
                  size="sm"
                  onclick={() => adoptField(field)}
                  disabled={isAccepted(field.field)}
                />
              </span>
            </div>
            {#if field.field === 'content' || field.field === 'markdown'}
              <pre class="ai-diff" style="margin:0;max-height:240px;overflow:auto;font-size:var(--text-sm);white-space:pre-wrap;background:var(--color-bg-subtle, rgba(0,0,0,0.04));border-radius:var(--radius-md);padding:var(--space-2);">
{#each diffLines(field) as line (line.text + line.type)}
{line.type === 'removed' ? '-' : line.type === 'added' ? '+' : ' '} {line.text}{/each}</pre>
            {:else}
              <p class="input-hint" style="margin:0;">{field.current ?? '（无当前值）'} → <strong>{field.proposed}</strong></p>
            {/if}
          </div>
        {/if}
      {/each}
    {:else}
      <div style="display:flex;flex-direction:column;gap:var(--space-2);">
        <Button text="AI 格式化（diff 预览）" variant="secondary" size="sm" onclick={runFormat} disabled={!canFormat} />
        <p class="input-hint" style="margin:0;">发送前会展示完整同意信息并需要你确认；结果只生成建议，需手动逐字段采纳，不自动改写正文。</p>
      </div>
    {/if}

    {#if notice}
      <p class="input-hint" role="status" style="margin:0;">{notice}</p>
    {/if}
    {#if error}
      <p class="input-hint is-error" role="alert" style="margin:0;">{error}</p>
    {/if}
  </div>
</div>
