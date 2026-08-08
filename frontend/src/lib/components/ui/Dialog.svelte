<script lang="ts">
  // M14-COMPONENTS-01/04：可访问 Dialog 基础组件（模态弹窗）。
  //
  // 可访问性契约（M14-A11Y-07 键盘/焦点验收）：
  // - role="dialog" + aria-modal + aria-labelledby/aria-describedby；
  // - 打开时焦点移入首个可聚焦元素；Tab/Shift+Tab 焦点陷阱循环；
  // - Escape 关闭（onclose）；关闭后焦点回到触发元素（focus return）；
  // - 打开期间锁定 body 滚动（滚动穿透防护）；
  // - 遮罩点击关闭；关闭按钮带 aria-label；
  // - 只接收白名单 prop（安全投影，无任意属性穿透，M14-COMPONENTS-06）。
  import { onDestroy, tick } from 'svelte';
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';

  const FOCUSABLE_SELECTOR = [
    'a[href]',
    'button:not([disabled])',
    'input:not([disabled])',
    'select:not([disabled])',
    'textarea:not([disabled])',
    '[tabindex]:not([tabindex="-1"])'
  ].join(',');

  let {
    open = false,
    title = '',
    description = '',
    closeLabel = '关闭',
    size = 'md',
    onclose,
    children,
    footer
  }: {
    open?: boolean;
    title?: string;
    description?: string;
    closeLabel?: string;
    size?: 'sm' | 'md' | 'lg';
    onclose?: () => void;
    children?: Snippet;
    footer?: Snippet;
  } = $props();

  const titleId = $derived(`bblbb-dialog-title-${Math.random().toString(36).slice(2, 8)}`);
  const descId = $derived(`bblbb-dialog-desc-${Math.random().toString(36).slice(2, 8)}`);

  let dialogEl: HTMLElement | undefined = $state();
  let previousFocus: HTMLElement | null = null;
  let removeScrollLock: (() => void) | null = null;

  function trapFocus(event: KeyboardEvent): void {
    if (event.key !== 'Tab' || !dialogEl) return;
    const focusable = Array.from(dialogEl.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR));
    if (focusable.length === 0) {
      event.preventDefault();
      dialogEl.focus();
      return;
    }
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    const active = document.activeElement;
    if (event.shiftKey && (active === first || active === dialogEl)) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && active === last) {
      event.preventDefault();
      first.focus();
    }
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      onclose?.();
    }
  }

  // 打开/关闭生命周期：焦点管理 + body 滚动锁 + 全局 Escape（$effect 清理即还原）。
  let cleanup: (() => void) | null = null;
  $effect(() => {
    if (!open) return;
    previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    document.addEventListener('keydown', handleKeydown);
    const scrollY = window.scrollY;
    document.body.style.overflow = 'hidden';
    removeScrollLock = () => {
      document.body.style.overflow = '';
      window.scrollTo(0, scrollY);
    };
    void tick().then(() => {
      if (!dialogEl) return;
      const first = dialogEl.querySelector<HTMLElement>(FOCUSABLE_SELECTOR);
      (first ?? dialogEl).focus();
    });
    cleanup = () => {
      document.removeEventListener('keydown', handleKeydown);
      removeScrollLock?.();
      if (previousFocus) previousFocus.focus();
    };
    return cleanup;
  });

  onDestroy(() => {
    cleanup?.();
  });
</script>

{#if open}
  <div class="modal-overlay" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose?.()}>
    <!-- svelte-ignore a11y_no_static_element_interactions：焦点陷阱与 Escape 由
         dialog 焦点内 keydown 处理（a11y_no_noninteractive_tabindex 场景在此需要）。 -->
    <div
      bind:this={dialogEl}
      class="modal modal--{size}"
      role="dialog"
      aria-modal="true"
      aria-labelledby={title ? titleId : undefined}
      aria-describedby={description ? descId : undefined}
      tabindex="-1"
      onkeydown={trapFocus}
    >
      {#if title}
        <div class="modal-header">
          <div class="modal-title" id={titleId}>{title}</div>
          <button type="button" class="modal-close" aria-label={closeLabel} onclick={() => onclose?.()}>
            <Icon name="x" size={18} />
          </button>
        </div>
      {:else}
        <button
          type="button"
          class="modal-close"
          aria-label={closeLabel}
          style="position:absolute;top:var(--space-3);right:var(--space-3);"
          onclick={() => onclose?.()}
        >
          <Icon name="x" size={18} />
        </button>
      {/if}
      {#if description}
        <p class="text-secondary" id={descId} style="font-size:var(--text-sm);margin:var(--space-2) var(--space-4) 0;">{description}</p>
      {/if}
      <div class="modal-body">
        {@render children?.()}
      </div>
      {#if footer}
        <div class="modal-footer">{@render footer()}</div>
      {/if}
    </div>
  </div>
{/if}
