<script lang="ts">
  // M03-UI-04：资料卡 portal/fixed 边界 + 窄屏底部卡。
  //
  // 在 M03-UI-03 交互基础上：
  // - portal：浮层打开时用 mount() 把卡渲染到 document.body 的宿主节点，
  //   彻底避免被 transform/overflow 祖先裁剪（fixed 定位恒视口可见）；
  // - 窄屏（≤640px，触摸为主）：hover/focus 不适用 → 点击触发改为底部卡
  //   （.user-card-sheet），自带关闭按钮，Escape/滚动/再次点击关闭；
  // - 不阻挡原导航：无全屏遮罩，触发元素始终是 <a>（无 JS 直接跳主页），
  //   底部卡内自带「查看个人主页」链接；
  // - 桌面浮层与 M03-UI-03 一致：mouseenter/focus 打开、离开延迟、Escape/
  //   scroll/resize 关闭、视口边缘夹紧。
  import { onMount, mount, unmount } from 'svelte';
  import type { Snippet } from 'svelte';
  import Avatar from './ui/Avatar.svelte';
  import CosmeticAvatar from './wardrobe/CosmeticAvatar.svelte';
  import UserHoverCard, { type HoverCardPresentation } from './UserHoverCard.svelte';
  import type { PublicProfile } from '$lib/api/client';
  import { sheetDrag } from '$lib/utils/sheet-drag';

  /** 触发卡只允许公开投影字段（严格 allowlist，杜绝私有字段流入浮层）。
   *  level/signature 可缺省：列表/搜索行只有部分公开投影。 */
  export type UserCardUser = Pick<PublicProfile, 'username' | 'display_name'> &
    Partial<Pick<PublicProfile, 'level' | 'signature'>>;

  /** 窄屏断点，与 prototype/app.css 及 components.css 一致。 */
  export const NARROW_QUERY = '(max-width: 640px)';

  let {
    user,
    children,
    href,
    label,
    class: klass = '',
    closeDelay = 250,
    presentation = null
  }: {
    user: UserCardUser;
    /** 触发内容（如 Avatar + 名字）。缺省渲染头像。 */
    children?: Snippet;
    /** 覆盖跳转地址（默认 /users/{username}）。 */
    href?: string;
    /** 覆盖可访问标签（默认「查看 … 的个人资料」）。 */
    label?: string;
    /** 追加到触发链接的额外 class（如复用既有链接样式）。 */
    class?: string;
    /** 离开触发/浮层后的关闭延迟（毫秒）。 */
    closeDelay?: number;
    /** 装扮安全投影（presentation_tokens）；缺省不渲染任何装扮。 */
    presentation?: HoverCardPresentation;
  } = $props();

  const profileUrl = $derived(href ?? `/users/${user.username}`);
  const displayName = $derived(user.display_name || user.username);
  const accessibleLabel = $derived(label ?? `查看 ${displayName} 的个人资料`);
  const triggerClass = $derived(
    klass ? `author-hover-trigger ${klass}` : 'author-hover-trigger'
  );

  let open = $state(false);
  let narrow = $state(false);
  let closeTimer: ReturnType<typeof setTimeout> | undefined;
  let trigger = $state<HTMLAnchorElement | undefined>(undefined);
  let portalInstance: ReturnType<typeof mount> | undefined;
  let portalHost: HTMLElement | undefined;
  let portalBackdrop: HTMLElement | undefined;
  let grabAction: ReturnType<typeof sheetDrag> | undefined;

  function clearCloseTimer() {
    if (closeTimer) {
      clearTimeout(closeTimer);
      closeTimer = undefined;
    }
  }

  function openCard() {
    clearCloseTimer();
    open = true;
  }

  function scheduleClose() {
    // 窄屏是显式点击/关闭交互，鼠标离开不自动关闭。
    if (narrow) return;
    clearCloseTimer();
    closeTimer = setTimeout(() => {
      open = false;
      closeTimer = undefined;
    }, closeDelay);
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      clearCloseTimer();
      open = false;
    }
  }

  function onScrollOrResize() {
    if (open) {
      clearCloseTimer();
      open = false;
    }
  }

  /** 窄屏：点击切换底部卡（阻止默认跳转）；桌面保持原导航，hover 出卡。 */
  function onTriggerClick(event: MouseEvent) {
    if (!narrow) return;
    event.preventDefault();
    open = !open;
  }

  function positionPopover(el: HTMLElement) {
    const rect = trigger?.getBoundingClientRect();
    if (!rect) return;
    const measured = el.getBoundingClientRect();
    const cardW = measured?.width ?? 320;
    const cardH = measured?.height ?? 240;
    const edge = 12;
    let left = rect.left + rect.width / 2 - cardW / 2;
    left = Math.max(edge, Math.min(left, window.innerWidth - cardW - edge));
    let top = rect.top - cardH - 10;
    if (top < edge) top = rect.bottom + 10;
    top = Math.max(edge, Math.min(top, window.innerHeight - cardH - edge));
    el.style.left = `${Math.round(left)}px`;
    el.style.top = `${Math.round(top)}px`;
  }

  /** 卡片打开后异步补齐数据（统计/徽章行）会使高度变化，
   *  ResizeObserver 触发重定位，避免卡片长大压住触发元素或溢出视口。 */
  let popoverRO: ResizeObserver | undefined;

  function destroyPortal() {
    popoverRO?.disconnect();
    popoverRO = undefined;
    if (portalInstance) {
      unmount(portalInstance);
      portalInstance = undefined;
    }
    portalBackdrop?.remove();
    portalBackdrop = undefined;
    grabAction?.destroy?.();
    grabAction = undefined;
    portalHost?.remove();
    portalHost = undefined;
  }

  /** 在 body 下创建宿主并把 UserHoverCard 挂进去（仅公开投影，见其隐私契约）。 */
  function createPortal() {
    destroyPortal();
    const el = document.createElement('div');
    if (narrow) {
      // 手机端 Bottom Sheet：scrim + 抓手 + 弹层内滚动（docs/MOBILE-SHEET.md）。
      const backdrop = document.createElement('button');
      backdrop.type = 'button';
      backdrop.className = 'app-sheet-backdrop';
      backdrop.setAttribute('aria-label', '关闭弹层');
      backdrop.addEventListener('click', () => {
        open = false;
      });
      document.body.appendChild(backdrop);
      portalBackdrop = backdrop;
      el.className = 'user-card-sheet';
      const grab = document.createElement('div');
      grab.className = 'app-sheet-grab';
      grab.setAttribute('aria-hidden', 'true');
      el.appendChild(grab);
      const body = document.createElement('div');
      body.className = 'user-card-sheet-body';
      const closeBtn = document.createElement('button');
      closeBtn.type = 'button';
      closeBtn.className = 'user-card-sheet-close';
      closeBtn.setAttribute('aria-label', '关闭');
      closeBtn.textContent = '×';
      closeBtn.addEventListener('click', () => {
        open = false;
      });
      body.appendChild(closeBtn);
      el.appendChild(body);
      document.body.appendChild(el);
      portalHost = el;
      el.setAttribute('aria-modal', 'true');
      portalInstance = mount(UserHoverCard, { target: body, props: { user, presentation } });
      grabAction = sheetDrag(grab, { onClose: () => (open = false) });
    } else {
      el.className = 'user-card-popover';
      el.setAttribute('tabindex', '0');
      el.addEventListener('mouseenter', clearCloseTimer);
      el.addEventListener('mouseleave', scheduleClose);
      // 焦点进入浮层（如 Tab 到卡内链接）不关闭，移出后延迟关闭。
      el.addEventListener('focusin', clearCloseTimer);
      el.addEventListener('focusout', scheduleClose);
      document.body.appendChild(el);
      portalHost = el;
      portalInstance = mount(UserHoverCard, { target: el, props: { user, presentation } });
      positionPopover(el);
      // 高度随数据补齐变化 → 重定位（jsdom 无 ResizeObserver，测试跳过）。
      if (typeof ResizeObserver !== 'undefined') {
        popoverRO = new ResizeObserver(() => positionPopover(el));
        popoverRO.observe(el);
      }
    }
    el.setAttribute('role', 'dialog');
    el.setAttribute('aria-label', `${displayName} 的个人资料`);
  }

  $effect(() => {
    destroyPortal();
    if (open) createPortal();
  });

  let mqCleanup: (() => void) | undefined;
  onMount(() => {
    // jsdom 无 matchMedia 时保持桌面模式（窄屏测试自行 mock）。
    if (typeof window.matchMedia === 'function') {
      const mq = window.matchMedia(NARROW_QUERY);
      narrow = mq.matches;
      const onChange = (e: MediaQueryListEvent) => {
        narrow = e.matches;
        if (e.matches) {
          clearCloseTimer();
          open = false;
        }
      };
      mq.addEventListener('change', onChange);
      mqCleanup = () => mq.removeEventListener('change', onChange);
    }
    document.addEventListener('keydown', onKeydown);
    document.addEventListener('scroll', onScrollOrResize, true);
    window.addEventListener('resize', onScrollOrResize);
    return () => {
      mqCleanup?.();
      destroyPortal();
      document.removeEventListener('keydown', onKeydown);
      document.removeEventListener('scroll', onScrollOrResize, true);
      window.removeEventListener('resize', onScrollOrResize);
    };
  });
</script>

<a
  bind:this={trigger}
  href={profileUrl}
  class={triggerClass}
  aria-label={accessibleLabel}
  onmouseenter={openCard}
  onmouseleave={scheduleClose}
  onfocus={openCard}
  onblur={scheduleClose}
  onclick={onTriggerClick}
>
  {#if children}
    {@render children()}
  {:else}
    <CosmeticAvatar name={displayName} size="xs" {presentation} avatarAttachmentId={'avatar_attachment_id' in user ? (user as { avatar_attachment_id?: string | null }).avatar_attachment_id : null} seed={user?.username ?? displayName} />
  {/if}
</a>
