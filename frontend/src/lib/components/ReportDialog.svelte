<script lang="ts">
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import { createReport } from '$lib/api/client';
  import { problemText } from '$lib/errors';

  let {
    open = $bindable(false),
    targetType = 'post',
    targetId = '',
    targetTitle = '',
    onclose,
    onsuccess
  }: {
    open?: boolean;
    targetType?: 'post' | 'comment' | 'user' | 'board' | string;
    targetId?: string;
    targetTitle?: string;
    onclose?: () => void;
    onsuccess?: (result: { id: string; status: string }) => void;
  } = $props();

  const reasons = [
    { value: 'spam', label: '垃圾广告' },
    { value: 'harassment', label: '骚扰谩骂' },
    { value: 'illegal', label: '违法违规' },
    { value: 'nsfw', label: '色情不当' },
    { value: 'misinformation', label: '不实信息' },
    { value: 'impersonation', label: '冒充他人' },
    { value: 'other', label: '其他' }
  ];

  let reason = $state('spam');
  let detail = $state('');
  let submitting = $state(false);
  let errorMessage: string | null = $state(null);

  const targetLabel = $derived(
    targetType === 'post' ? '帖子' : targetType === 'comment' ? '回复' : targetType === 'user' ? '用户' : '内容'
  );

  function handleClose() {
    errorMessage = null;
    open = false;
    onclose?.();
  }

  async function handleSubmit(event: SubmitEvent) {
    event.preventDefault();
    if (!reason || !targetId || submitting) return;

    submitting = true;
    errorMessage = null;

    try {
      const result = await createReport(fetch, {
        target_type: targetType,
        target_id: targetId,
        reason,
        detail: detail.trim() || null
      });

      showToast('举报已提交，我们会尽快核实处理', 'success');

      open = false;
      detail = '';
      reason = 'spam';
      onsuccess?.(result);
      onclose?.();
    } catch (err: unknown) {
      errorMessage = problemText(err as Parameters<typeof problemText>[0]) || (err as { message?: string })?.message || '提交失败，请稍后重试';
    } finally {
      submitting = false;
    }
  }
</script>

<Dialog
  {open}
  title={`举报${targetLabel}`}
  description="请选择举报原因并补充说明，管理员将尽快核实处理。"
  onclose={handleClose}
>
  {#if errorMessage}
    <div
      class="form-error"
      role="alert"
      data-testid="report-modal-error"
      style="margin-bottom:var(--space-3);padding:8px 12px;font-size:var(--text-sm);color:var(--color-danger);background:var(--color-danger-soft);border-radius:var(--radius-md);"
    >
      {errorMessage}
    </div>
  {/if}

  <form onsubmit={handleSubmit} class="stack" style="gap:var(--space-3);" data-testid="report-form">
    {#if targetTitle || targetId}
      <div style="background:var(--color-bg-subtle);padding:var(--space-2) var(--space-3);border-radius:var(--radius-md);font-size:var(--text-sm);display:flex;align-items:center;gap:var(--space-2);overflow:hidden;">
        <span class="badge" style="flex-shrink:0;">{targetLabel}</span>
        {#if targetTitle}
          <span style="font-weight:500;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">{targetTitle}</span>
        {/if}
        <code style="font-size:11px;color:var(--color-text-secondary);margin-left:auto;flex-shrink:0;">{targetId.slice(0, 12)}</code>
      </div>
    {/if}

    <label>
      <span class="field-label" style="display:block;margin-bottom:var(--space-1);font-weight:500;font-size:var(--text-sm);">举报原因</span>
      <select
        name="reason"
        class="input-field"
        bind:value={reason}
        required
        style="width:100%;padding:var(--space-2);border-radius:var(--radius-md);border:var(--border-default);background:var(--color-bg-card, inherit);color:inherit;"
      >
        {#each reasons as r}
          <option value={r.value}>{r.label}</option>
        {/each}
      </select>
    </label>

    <label>
      <span class="field-label" style="display:block;margin-bottom:var(--space-1);font-weight:500;font-size:var(--text-sm);">补充说明（可选，最多 2000 字）</span>
      <textarea
        name="detail"
        class="input-field"
        bind:value={detail}
        rows="4"
        maxlength="2000"
        placeholder="请描述具体违规情况或原因，便于管理员快速核查..."
        style="width:100%;padding:var(--space-2);border-radius:var(--radius-md);border:var(--border-default);resize:vertical;background:var(--color-bg-card, inherit);color:inherit;"
      ></textarea>
    </label>

    <div style="display:flex;justify-content:flex-end;gap:var(--space-2);margin-top:var(--space-2);">
      <button type="button" class="btn btn-secondary" onclick={handleClose} disabled={submitting}>
        取消
      </button>
      <button type="submit" class="btn btn-primary" disabled={submitting} data-testid="report-submit-btn">
        {submitting ? '提交中…' : '提交举报'}
      </button>
    </div>
  </form>
</Dialog>
