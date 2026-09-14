<script lang="ts">
  // M18-ADMIN-OPS（约定 D）：行动作「⋯ 三点」菜单（横向操作条 → 竖向菜单）。
  //
  // 交互模式通名：Dropdown Menu（下拉菜单）；表格行场景即 Row Actions Menu，
  // 触发按钮俗称 kebab menu / overflow menu（溢出菜单）。
  //
  // 定位契约 v2（修复「点击后菜单出现在距触发点半屏外」）：
  // 1. 菜单经 portal 挂到 document.body（对齐 ReactionBar/UserCard 既有约定）：
  //    旧实现把菜单留在组件内用 position:fixed + 一次性测量定位——
  //    a) 任何 transform/backdrop-filter/zoom 祖先会把 fixed 包含块从视口劫持成
  //       该祖先（主题样式一旦加了毛玻璃/动画，菜单整体错位）；
  //    b) 一次测量定终身 + 依赖「滚动即关闭」兜底，任何漏掉的滚动/布局变化
  //       （HMR 半更新、程序化滚动、字体加载重排）都会留下悬在旧位置的菜单；
  //    c) right 用 window.innerWidth 计算，多算一条经典滚动条宽度（实测偏 12px）。
  //    现改为：portal 脱离裁剪/包含块祖先 + 打开时实时测量 + 滚动/resize 跟随
  //    重新定位（rAF 节流），几何决策在 row-actions-position.ts 纯函数中单测。
  // 2. 可访问性：role=menu/menuitem、aria-haspopup/expanded/controls；
  //    触发按钮 ↑/↓ 打开并聚焦末/首项；菜单内 ↑/↓/Home/End 循环移动、Tab 关闭、
  //    Escape 关闭并把焦点还给触发按钮；鼠标打开不抢焦点。
  // 3. 关闭时机：点击外部（pointerdown capture，portal 后菜单点击不算外部）、
  //    Escape、选中动作项；销毁时兜底关闭。SSR 基线：关闭态不渲染菜单 DOM。
  import { onDestroy } from 'svelte';
  import { computeMenuPosition } from './row-actions-position';

  export interface RowActionItem {
    /** 菜单文案（同时作为 menuitem 的可读标签）。 */
    label: string;
    /** 危险操作（删除/停用/紧急停用等），红色呈现。 */
    danger?: boolean;
    /** 禁用（如该行状态下不可用）。 */
    disabled?: boolean;
    /** 禁用原因 / 补充说明（title 提示）。 */
    hint?: string;
    /** 执行：先关菜单再调用；一般 `run = () => openXxx(item)`。 */
    run: () => void;
  }

  let {
    actions,
    label = '更多操作'
  }: {
    actions: RowActionItem[];
    /** 触发按钮的 aria-label（建议含行语义，如「更多操作：用户 alice」）。 */
    label?: string;
  } = $props();

  let open = $state(false);
  let rootEl: HTMLElement | undefined = $state();
  let triggerEl: HTMLButtonElement | undefined = $state();
  let menuEl: HTMLElement | undefined = $state();
  /** 键盘打开时的待聚焦方向；鼠标打开不抢焦点。 */
  let pendingFocus: 'first' | 'last' | null = $state(null);

  const menuId = $props.id();

  function show(): void {
    pendingFocus = null;
    open = true;
  }

  function close(refocus = false): void {
    if (!open) return;
    open = false;
    pendingFocus = null;
    if (refocus) triggerEl?.focus();
  }

  function toggle(): void {
    if (open) close();
    else show();
  }

  function pick(action: RowActionItem): void {
    if (action.disabled) return;
    close();
    action.run();
  }

  function enabledItems(): HTMLButtonElement[] {
    if (!menuEl) return [];
    return [...menuEl.querySelectorAll<HTMLButtonElement>('button[role="menuitem"]:not(:disabled)')];
  }

  function focusItemAt(which: 'first' | 'last'): void {
    const items = enabledItems();
    if (items.length === 0) return;
    (which === 'first' ? items[0] : items[items.length - 1]).focus();
  }

  function onTriggerKeydown(event: KeyboardEvent): void {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      pendingFocus = event.key === 'ArrowDown' ? 'first' : 'last';
      open = true;
    }
  }

  function onMenuKeydown(event: KeyboardEvent): void {
    const items = enabledItems();
    if (items.length === 0) return;
    const current = items.indexOf(document.activeElement as HTMLButtonElement);
    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault();
        items[(current + 1) % items.length]?.focus();
        break;
      case 'ArrowUp':
        event.preventDefault();
        items[(current - 1 + items.length) % items.length]?.focus();
        break;
      case 'Home':
        event.preventDefault();
        items[0]?.focus();
        break;
      case 'End':
        event.preventDefault();
        items[items.length - 1]?.focus();
        break;
      case 'Tab':
        // 焦点离开菜单即关闭（焦点自然移动，不拦截）。
        close();
        break;
    }
  }

  /** 实时按触发按钮矩形定位（portal 后菜单挂在 body，fixed 相对视口无劫持）。 */
  function positionMenu(): void {
    if (!menuEl || !triggerEl || typeof document === 'undefined') return;
    const triggerRect = triggerEl.getBoundingClientRect();
    // 布局视口（不含经典滚动条）：window.innerWidth 会偏一个滚动条宽度。
    // clientWidth 在异常嵌入（iframe 隐藏、jsdom）下可能为 0 → 回退 innerWidth。
    const vw = document.documentElement.clientWidth || window.innerWidth;
    const vh = document.documentElement.clientHeight || window.innerHeight;
    // 锚点完全滚出视口（行纵向滚走 / 操作列横向滚走）→ 菜单失去意义，直接关闭，
    // 不留「钉在视口边缘的幽灵菜单」。
    if (triggerRect.bottom < 0 || triggerRect.top > vh || triggerRect.right < 0 || triggerRect.left > vw) {
      close();
      return;
    }
    const menuRect = menuEl.getBoundingClientRect();
    const { top, left } = computeMenuPosition(
      triggerRect,
      { width: menuRect.width || menuEl.offsetWidth, height: menuRect.height || menuEl.offsetHeight },
      { width: vw, height: vh }
    );
    menuEl.style.left = `${left}px`;
    menuEl.style.top = `${top}px`;
    menuEl.style.visibility = 'visible';
  }

  /** portal action：菜单挂到 document.body，脱离 overflow/transform 祖先。 */
  function portalMenu(node: HTMLElement): { destroy: () => void } {
    document.body.appendChild(node);
    return {
      destroy() {
        node.remove();
      }
    };
  }

  // 打开/动作集变化：先量后放（初始 visibility:hidden 防跳帧），再等一帧修正
  // 字体/布局稳定后的微差（对齐 ReactionBar 弹层的二次定位约定）。
  $effect(() => {
    if (!open || !menuEl) return;
    void actions.length;
    positionMenu();
    if (pendingFocus) {
      focusItemAt(pendingFocus);
      pendingFocus = null;
    }
    const raf = requestAnimationFrame(positionMenu);
    return () => cancelAnimationFrame(raf);
  });

  let rafId = 0;
  function schedulePosition(): void {
    if (!open || rafId) return;
    rafId = requestAnimationFrame(() => {
      rafId = 0;
      positionMenu();
    });
  }

  /** 外部关闭判定：portal 后菜单点击不算外部（触发按钮走 toggle 自己处理）。 */
  function isOutside(target: Node | null): boolean {
    if (!target) return false;
    return !(rootEl?.contains(target) ?? false) && !(menuEl?.contains(target) ?? false);
  }

  function onDocPointerdown(event: PointerEvent): void {
    if (open && isOutside(event.target as Node | null)) close();
  }

  // click 兜底：无 PointerEvent 环境/合成点击也能外部关闭（与 pointerdown 幂等）。
  function onDocClick(event: MouseEvent): void {
    if (open && isOutside(event.target as Node | null)) close();
  }

  function onDocKeydown(event: KeyboardEvent): void {
    if (open && event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      close(true);
    }
  }

  $effect(() => {
    document.addEventListener('pointerdown', onDocPointerdown, true);
    document.addEventListener('click', onDocClick, true);
    document.addEventListener('keydown', onDocKeydown, true);
    // capture：内层滚动容器（.app-table-wrap、卡片体）滚动也让菜单跟随触发点。
    document.addEventListener('scroll', schedulePosition, true);
    window.addEventListener('resize', schedulePosition);
    return () => {
      document.removeEventListener('pointerdown', onDocPointerdown, true);
      document.removeEventListener('click', onDocClick, true);
      document.removeEventListener('keydown', onDocKeydown, true);
      document.removeEventListener('scroll', schedulePosition, true);
      window.removeEventListener('resize', schedulePosition);
      if (rafId) cancelAnimationFrame(rafId);
    };
  });

  onDestroy(() => close());
</script>

<div class="row-actions" bind:this={rootEl}>
  <button
    type="button"
    class="row-actions__trigger"
    bind:this={triggerEl}
    aria-label={label}
    aria-haspopup="menu"
    aria-expanded={open}
    aria-controls={menuId}
    onclick={toggle}
    onkeydown={onTriggerKeydown}
  >
    <!-- ⋯ 三点（横向）内联 SVG，不依赖图标集 -->
    <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor" aria-hidden="true">
      <circle cx="5" cy="12" r="1.9" />
      <circle cx="12" cy="12" r="1.9" />
      <circle cx="19" cy="12" r="1.9" />
    </svg>
  </button>

  {#if open}
    <div
      class="row-actions__menu"
      role="menu"
      tabindex="-1"
      aria-label={label}
      id={menuId}
      bind:this={menuEl}
      use:portalMenu
      style="visibility:hidden;"
      onkeydown={onMenuKeydown}
    >
      {#each actions as action (action.label)}
        <button
          type="button"
          role="menuitem"
          class="row-actions__item"
          class:is-danger={action.danger}
          disabled={action.disabled}
          title={action.hint}
          onclick={() => pick(action)}
        >
          {action.label}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .row-actions {
    position: relative;
    display: inline-flex;
  }

  .row-actions__trigger {
    display: inline-grid;
    place-items: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-bg-card);
    color: var(--color-text-secondary);
    cursor: pointer;
  }

  .row-actions__trigger:hover,
  .row-actions__trigger[aria-expanded='true'] {
    color: var(--color-text-primary);
    border-color: var(--color-brand);
  }

  .row-actions__menu {
    /* portal 到 body 后 fixed 相对视口：left/top 由 positionMenu 实时写入
       （初始 visibility:hidden，量好坐标再显示，防跳帧）；滚动/resize 跟随
       重新定位，不再依赖「一滚就关」。 */
    position: fixed;
    z-index: var(--z-dropdown, 100);
    min-width: 150px;
    max-height: calc(100vh - 2 * 8px);
    overflow-y: auto;
    padding: 4px;
    display: flex;
    flex-direction: column;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.16);
  }

  .row-actions__item {
    width: 100%;
    padding: 7px 10px;
    border: none;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--color-text-primary);
    font-size: var(--text-sm, 13px);
    text-align: left;
    cursor: pointer;
    white-space: nowrap;
  }

  .row-actions__item:hover:not(:disabled),
  .row-actions__item:focus-visible {
    background: var(--color-bg-subtle);
  }

  .row-actions__item.is-danger {
    color: var(--color-danger);
  }

  .row-actions__item:disabled {
    color: var(--color-text-tertiary);
    cursor: not-allowed;
  }
</style>
