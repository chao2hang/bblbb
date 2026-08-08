<script lang="ts">
  // M14-COMPONENTS-03：账务确认组件（积分/余额/购买等资金动作）。
  //
  // - 复用 Dialog（焦点陷阱/Escape/焦点回收）；
  // - 显示金额/余额摘要行（amount / balanceAfter / fee 白名单投影）；
  // - 确认按钮 variant="danger"（资金操作高风险样式）；
  // - 服务端错误经 role="alert" 播报；busy 禁用双按钮防重复提交
  //   （配合后端 Idempotency-Key，M14-ROUTES-08）；
  // - 只接收白名单 prop（安全投影，M14-COMPONENTS-06）。
  import type { Snippet } from 'svelte';
  import Dialog from './Dialog.svelte';
  import Button from './Button.svelte';

  let {
    open = false,
    title = '确认支付',
    description = '',
    amount = 0,
    currency = 'B币',
    balanceAfter = null as number | null,
    fee = null as number | null,
    confirmText = '确认支付',
    cancelText = '取消',
    busyText = '处理中…',
    busy = false,
    error = '',
    onconfirm,
    oncancel,
    children
  }: {
    open?: boolean;
    title?: string;
    description?: string;
    amount?: number;
    currency?: string;
    balanceAfter?: number | null;
    fee?: number | null;
    confirmText?: string;
    cancelText?: string;
    busyText?: string;
    busy?: boolean;
    error?: string;
    onconfirm?: () => void;
    oncancel?: () => void;
    children?: Snippet;
  } = $props();
</script>

<Dialog {open} {title} {description} onclose={oncancel}>
  <dl class="accounting-confirm-summary">
    <div class="accounting-confirm-row"><dt>支付金额</dt><dd><strong>{amount} {currency}</strong></dd></div>
    {#if fee !== null}
      <div class="accounting-confirm-row"><dt>手续费</dt><dd>{fee} {currency}</dd></div>
    {/if}
    {#if balanceAfter !== null}
      <div class="accounting-confirm-row"><dt>支付后余额</dt><dd>{balanceAfter} {currency}</dd></div>
    {/if}
  </dl>
  {@render children?.()}
  {#if error}
    <p class="input-hint is-error" role="alert" style="margin-top:var(--space-2);">{error}</p>
  {/if}
  {#snippet footer()}
    <Button text={cancelText} variant="ghost" size="sm" onclick={() => oncancel?.()} disabled={busy} />
    <Button text={busy ? busyText : confirmText} variant="danger" size="sm" onclick={() => onconfirm?.()} disabled={busy} />
  {/snippet}
</Dialog>
