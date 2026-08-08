<script lang="ts">
  // M14-COMPONENTS-03：危险操作确认组件（删除/封禁/撤下等不可逆动作）。
  //
  // - 复用 Dialog（焦点陷阱/Escape/焦点回收/遮罩关闭）；
  // - 确认按钮 variant="danger"，busy 时禁用并显示进行中文案；
  // - 服务端错误经 role="alert" 播报（M14-COMPONENTS-04 aria-live）；
  // - 只接收白名单 prop（安全投影，M14-COMPONENTS-06）。
  import type { Snippet } from 'svelte';
  import Dialog from './Dialog.svelte';
  import Button from './Button.svelte';

  let {
    open = false,
    title = '危险操作确认',
    description = '',
    confirmText = '确认执行',
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
  {@render children?.()}
  {#if error}
    <p class="input-hint is-error" role="alert" style="margin-top:var(--space-2);">{error}</p>
  {/if}
  {#snippet footer()}
    <Button text={cancelText} variant="ghost" size="sm" onclick={() => oncancel?.()} disabled={busy} />
    <Button text={busy ? busyText : confirmText} variant="danger" size="sm" onclick={() => onconfirm?.()} disabled={busy} />
  {/snippet}
</Dialog>
