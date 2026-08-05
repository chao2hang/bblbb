<script lang="ts">
  // M03-UI-03：用户资料卡触发器——鼠标 Hover 与键盘 Focus 共用同一浮层，
  // 支持离开延迟、Escape 关闭、滚动/缩放自动关闭。
  //
  // 交互契约（与 prototype/js/app.js initUserHoverCards 一致）：
  // - 触发元素是普通链接（无 JS 直接跳主页），浮层为渐进增强；
  // - mouseenter/focus → 打开；mouseleave/blur → 离开延迟后关闭；
  // - 浮层 mouseenter → 取消关闭计时；mouseleave → 延迟关闭；
  // - Escape / scroll / resize → 立即关闭；
  // - 浮层 fixed 定位（.user-hover-card），视口边缘夹紧；
  //   portal-to-body 与窄屏底部卡见 M03-UI-04；
  // - 隐私：props 只接受 PublicProfile 公开字段 allowlist，浮层内容由
  //   UserHoverCard（M03-PROFILE-09）渲染，SSR 守卫见 privacy.test.ts。
  import { onMount } from 'svelte';
  import type { Snippet } from 'svelte';
  import Avatar from './ui/Avatar.svelte';
  import UserHoverCard from './UserHoverCard.svelte';
  import type { PublicProfile } from '$lib/api/client';

  /** 触发卡只允许公开投影字段（严格 allowlist，杜绝私有字段流入浮层）。 */
  export type UserCardUser = Pick<
    PublicProfile,
    'username' | 'display_name' | 'level' | 'bio' | 'signature'
  >;

  let {
    user,
    children,
    href,
    label,
    closeDelay = 250
  }: {
    user: UserCardUser;
    /** 触发内容（如 Avatar + 名字）。缺省渲染头像。 */
    children?: Snippet;
    /** 覆盖跳转地址（默认 /users/{username}）。 */
    href?: string;
    /** 覆盖可访问标签（默认「查看 … 的个人资料」）。 */
    label?: string;
    /** 离开触发/浮层后的关闭延迟（毫秒）。 */
    closeDelay?: number;
  } = $props();

  const profileUrl = $derived(href ?? `/users/${user.username}`);
  const displayName = $derived(user.display_name || user.username);
  const accessibleLabel = $derived(label ?? `查看 ${displayName} 的个人资料`);

  let open = $state(false);
  let closeTimer: ReturnType<typeof setTimeout> | undefined;
  let trigger = $state<HTMLAnchorElement | undefined>(undefined);
  let card = $state<HTMLElement | undefined>(undefined);
  let position = $state({ left: 0, top: 0 });

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

  // 打开后把浮层定位在触发元素附近（fixed 坐标，视口边缘夹紧）。
  $effect(() => {
    if (!open || !trigger) return;
    const rect = trigger.getBoundingClientRect();
    const measured = card?.getBoundingClientRect();
    const cardW = measured?.width ?? 320;
    const cardH = measured?.height ?? 240;
    const edge = 12;
    let left = rect.left + rect.width / 2 - cardW / 2;
    left = Math.max(edge, Math.min(left, window.innerWidth - cardW - edge));
    let top = rect.top - cardH - 10;
    if (top < edge) top = rect.bottom + 10;
    top = Math.max(edge, Math.min(top, window.innerHeight - cardH - edge));
    position = { left, top };
  });

  onMount(() => {
    document.addEventListener('keydown', onKeydown);
    document.addEventListener('scroll', onScrollOrResize, true);
    window.addEventListener('resize', onScrollOrResize);
    return () => {
      document.removeEventListener('keydown', onKeydown);
      document.removeEventListener('scroll', onScrollOrResize, true);
      window.removeEventListener('resize', onScrollOrResize);
    };
  });
</script>

<a
  bind:this={trigger}
  href={profileUrl}
  class="author-hover-trigger"
  aria-label={accessibleLabel}
  onmouseenter={openCard}
  onmouseleave={scheduleClose}
  onfocus={openCard}
  onblur={scheduleClose}
>
  {#if children}
    {@render children()}
  {:else}
    <Avatar name={displayName} size="xs" />
  {/if}
</a>

{#if open}
  <div
    bind:this={card}
    class="user-card-popover"
    style="left:{position.left}px;top:{position.top}px;"
    role="dialog"
    tabindex="0"
    aria-label="{displayName} 的个人资料"
    onmouseenter={clearCloseTimer}
    onmouseleave={scheduleClose}
    onfocusin={clearCloseTimer}
    onfocusout={scheduleClose}
  >
    <UserHoverCard {user} />
  </div>
{/if}
