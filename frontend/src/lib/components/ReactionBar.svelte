<!-- M07-UI-07：Reaction 表情互动组件。
  - 左侧仅展示「收到的反应」：无反应时不渲染任何图标；有反应时只显示图标 + 反应次数（紧凑 Pill）。
  - 点击图标弹出明细弹窗（按表情分类 Tab + 反应用户列表）；再次点击同一图标收起。
  - 单激活语义：一人对同一目标只保留一个激活反应；选择器中点击不同表情即切换
    （先移除旧反应再添加新反应，后端事务内原子完成），响应含完整 counts 时以服务端回同步。
  - 右侧「+ 表情」选择器负责添加/切换/撤销反应（选择器内已激活项高亮，点击即撤销）。
  - onReactionMutated 回调把变更结果回传父级，联动行内快捷动作按钮（侧栏赞）。
  - 429 限流：显示 Retry-After 秒数并禁用按钮（冷却倒计时）。
  - 403 目标权限错误 / 401 未登录：分别给出指引。
  - 通知偏好：不渲染任何通知提示（产品移除「可在通知设置中关闭」作者提示）。
  - 键盘与无障碍：原生 <button>（Enter/Space 激活，Esc 关闭弹窗）。
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { onMount, untrack } from 'svelte';
  import {
    addCommentReaction,
    addPostReaction,
    getCommentReactions,
    getPostReactions,
    removeCommentReaction,
    removePostReaction,
    type ReactionUserItem
  } from '$lib/api/client';
  import { problemMessage, retryAfterOf, type Problem } from '$lib/errors';
  import Icon from '$lib/components/ui/Icon.svelte';
  import DogeIcon from '$lib/components/ui/DogeIcon.svelte';
  import HuajiIcon from '$lib/components/ui/HuajiIcon.svelte';
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import { AVAILABLE_REACTIONS, getReactionDef } from '$lib/reactions';
  import { SHEET_MEDIA_QUERY, sheetDrag } from '$lib/utils/sheet-drag';

  let {
    targetType,
    targetId,
    reactions = [],
    authed = true,
    currentUser = null,
    fetchFn = fetch,
    onReactionMutated,
    rightActions
  }: {
    targetType: 'post' | 'comment';
    targetId: string;
    /** 初始计数 [{reaction, count, active}]；父级随服务端结果更新时组件同步种子项。 */
    reactions?: Array<{ reaction: string; count: number; active?: boolean }>;
    authed?: boolean;
    currentUser?: { id?: string; username?: string; display_name?: string | null } | null;
    fetchFn?: typeof fetch;
    /** 本组件完成一次添加/撤销后回传结果，供父级同步行内快捷动作按钮（如侧栏赞）。 */
    onReactionMutated?: (payload: {
      reaction: string;
      active: boolean;
      count: number;
      counts?: Record<string, number>;
    }) => void;
    rightActions?: Snippet;
  } = $props();

  let items = $state<Array<{ reaction: string; count: number; active: boolean }>>([]);
  let initialized = false;
  $effect(() => {
    const seeded = reactions; // 跟踪 prop：父级更新时重新合并
    const current = untrack(() => items); // 避免对 items 形成自依赖（乐观更新不被回读覆盖）
    if (!initialized) {
      items = seeded.map((r) => ({ ...r, active: Boolean(r.active) }));
      initialized = true;
      return;
    }
    // 父级用服务端结果更新种子项（like/doge）时同步到本地，保持与行内按钮联动
    items = current.map((it) => {
      const s = seeded.find((x) => sameReaction(x.reaction, it.reaction));
      return s ? { reaction: it.reaction, count: s.count, active: Boolean(s.active) } : it;
    });
  });

  let busyReaction = $state<string | null>(null);
  let errorText = $state('');
  let cooldownUntil = $state<number>(0);
  let cooldownLeft = $state(0);
  let pickerOpen = $state(false);

  // 表情详情弹窗（左侧展示收到表情的用户列表与 Tab 切换）
  let detailOpen = $state(false);
  let activeTab = $state<string>('all');
  let rxUsers = $state<ReactionUserItem[]>([]);
  let detailLoading = $state(false);
  let detailLoaded = $state(false);

  let barContainer: HTMLElement | null = $state(null);

  // ── 弹层 portal（对齐 UserCard M03-UI-04 方案）──
  // 卡片/面板（.card/.app-card）带 overflow:hidden，弹层原来用 absolute 向上展开时
  // 会被父卡片裁剪（截半）。改为：弹层挂到 document.body（portal）+ fixed 定位，
  // 依据触发元素矩形定位（优先上方，上方空间不足自动翻到下方，并夹紧在视口内），
  // 彻底避免被 overflow/transform 祖先裁剪。Svelte 5 委托事件同时挂在 document 上，
  // 移动节点不影响 onclick（见 svelte render.js root 事件监听注释）。
  let pickerBtn: HTMLButtonElement | null = $state(null);
  let detailTrigger: HTMLElement | null = $state(null);
  let pickerPopEl: HTMLElement | null = $state(null);
  let detailPopEl: HTMLElement | null = $state(null);

  // 手机端断点（≤767px）：弹层以 Bottom Sheet 模态呈现（docs/MOBILE-SHEET.md）。
  let isNarrow = $state(false);
  $effect(() => {
    if (typeof window.matchMedia !== 'function') return;
    const mq = window.matchMedia(SHEET_MEDIA_QUERY);
    isNarrow = mq.matches;
    const onChange = (e: MediaQueryListEvent) => (isNarrow = e.matches);
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  });

  const POPLAYER_EDGE = 8;

  function positionPopover(
    pop: HTMLElement | null,
    trigger: HTMLElement | null,
    align: 'left' | 'right'
  ) {
    if (!pop || !trigger) return;
    const rect = trigger.getBoundingClientRect();
    const popRect = pop.getBoundingClientRect();
    const popW = popRect.width || pop.offsetWidth || 290;
    const popH = popRect.height || pop.offsetHeight || 160;
    // 优先在触发元素上方展开（与原 bottom: calc(100% + 8px) 一致）；上方放不下翻到下方
    let top = rect.top - popH - POPLAYER_EDGE;
    if (top < POPLAYER_EDGE) top = rect.bottom + POPLAYER_EDGE;
    const maxTop = Math.max(POPLAYER_EDGE, window.innerHeight - popH - POPLAYER_EDGE);
    top = Math.max(POPLAYER_EDGE, Math.min(top, maxTop));
    // 选择器右对齐触发按钮（原 right:0），明细弹窗左对齐触发 Pill（原 left:0）
    let left = align === 'right' ? rect.right - popW : rect.left;
    left = Math.max(POPLAYER_EDGE, Math.min(left, window.innerWidth - popW - POPLAYER_EDGE));
    pop.style.left = `${Math.round(left)}px`;
    pop.style.top = `${Math.round(top)}px`;
  }

  function positionActivePopovers() {
    positionPopover(pickerPopEl, pickerBtn, 'right');
    positionPopover(detailPopEl, detailTrigger, 'left');
  }

  /** portal action：把弹层节点移入 document.body 并定位；销毁时还原清理。 */
  function portalPopover(node: HTMLElement, params: {
    getTrigger: () => HTMLElement | null;
    align: 'left' | 'right';
    register: (el: HTMLElement | null) => void;
  }) {
    document.body.appendChild(node);
    params.register(node);
    positionPopover(node, params.getTrigger(), params.align);
    // 二次定位：等待首帧字体/内容最终度量，修正微小偏差
    const raf = requestAnimationFrame(() =>
      positionPopover(node, params.getTrigger(), params.align)
    );
    return {
      destroy() {
        cancelAnimationFrame(raf);
        params.register(null);
        node.remove();
      }
    };
  }

  $effect(() => {
    if (cooldownUntil <= Date.now()) return;
    const timer = setInterval(() => {
      cooldownLeft = Math.max(0, Math.ceil((cooldownUntil - Date.now()) / 1000));
      if (cooldownLeft <= 0) {
        clearInterval(timer);
        errorText = '';
      }
    }, 1000);
    return () => clearInterval(timer);
  });

  function getDef(keyOrEmoji: string) {
    return getReactionDef(keyOrEmoji);
  }

  // 同一反应的两种表示（key / emoji）归一化匹配，避免选择器提交 key 时
  // 与 prop 传入的 emoji 项产生重复条目。
  function sameReaction(a: string, b: string): boolean {
    if (a === b) return true;
    const da = getDef(a);
    const db = getDef(b);
    return da.key === db.key || da.emoji === db.emoji;
  }

  // 收到表态的项列表（计数 > 0 或 active）
  const activeReactions = $derived(items.filter((r) => r.count > 0 || r.active));

  // 按选中 Tab 过滤的用户列表
  const filteredUsers = $derived.by(() => {
    if (activeTab === 'all') return rxUsers;
    return rxUsers.filter((u) => {
      const def = getDef(u.reaction);
      const targetDef = getDef(activeTab);
      return u.reaction === activeTab || def.key === targetDef.key || def.emoji === targetDef.emoji;
    });
  });

  async function loadDetail(force = false) {
    if ((detailLoaded && !force) || !targetId || targetId === 'demo') return;
    detailLoading = true;
    try {
      const res =
        targetType === 'post'
          ? await getPostReactions(fetchFn, targetId)
          : await getCommentReactions(fetchFn, targetId);
      if (res) {
        if (Array.isArray(res.users)) {
          rxUsers = res.users;
        }
        if (res.counts && typeof res.counts === 'object') {
          for (const [key, count] of Object.entries(res.counts)) {
            const active = res.viewer_reactions?.includes(key) ?? false;
            applyResult({ reaction: key, count: Number(count), active });
          }
        }
      }
      detailLoaded = true;
    } catch {
      // 优雅容错，保持已有 items
    } finally {
      detailLoading = false;
    }
  }

  onMount(() => {
    // 首次挂载预加载表态明细
    if (targetId && targetId !== 'demo') {
      loadDetail();
    }
  });

  // 点击左侧反应图标：打开明细弹窗并定位到对应 Tab；已打开且 Tab 相同时再次点击收起
  function openDetail(reaction: string, trigger?: EventTarget | null) {
    pickerOpen = false;
    if (trigger instanceof HTMLElement) detailTrigger = trigger;
    if (detailOpen && activeTab === reaction) {
      detailOpen = false;
      return;
    }
    const wasOpen = detailOpen;
    activeTab = reaction;
    detailOpen = true;
    if (wasOpen) {
      // 已展开时点击另一 Pill 切 Tab：跟随新触发元素重新定位
      positionPopover(detailPopEl, detailTrigger, 'left');
    }
    loadDetail();
  }

  function applyResult(result: { reaction: string; active: boolean; count: number }) {
    const exists = items.some((r) => sameReaction(r.reaction, result.reaction));
    if (exists) {
      items = items.map((r) =>
        sameReaction(r.reaction, result.reaction)
          ? { ...r, count: result.count, active: result.active }
          : r
      );
    } else if (result.active || result.count > 0) {
      items = [...items, { reaction: result.reaction, count: result.count, active: result.active }];
    }
  }

  // 单激活切换：乐观移除一个激活项（同时从明细列表移除当前用户对应记录）
  function optimisticRemoveItem(item: { reaction: string; count: number }) {
    applyResult({ reaction: item.reaction, active: false, count: Math.max(0, item.count - 1) });
    if (currentUser) {
      rxUsers = rxUsers.filter(
        (u) =>
          !(
            sameReaction(u.reaction, item.reaction) &&
            (u.user_id === currentUser.id || u.username === currentUser.username)
          )
      );
    }
  }

  // 用服务端响应的完整 counts 回同步所有项计数（服务端为准）
  function resyncFromCounts(counts: Record<string, number>) {
    items = items.map((it) => {
      const hit = Object.entries(counts).find(([k]) => sameReaction(k, it.reaction));
      return hit ? { ...it, count: Number(hit[1]) } : { ...it, count: 0 };
    });
  }

  async function toggle(reaction: string) {
    errorText = '';
    if (!authed) {
      errorText = '请先登录后再使用反应';
      return;
    }
    const current = items.find((r) => sameReaction(r.reaction, reaction));
    if (busyReaction) return;
    busyReaction = reaction;
    // 乐观切换的快照（失败时回滚）
    let prevSnapshot: { reaction: string; count: number } | null = null;
    try {
      if (current?.active) {
        const result =
          targetType === 'post'
            ? await removePostReaction(fetchFn, targetId, reaction)
            : await removeCommentReaction(fetchFn, targetId, reaction);
        const removedCount =
          typeof result?.count === 'number' ? result.count : Math.max(0, current.count - 1);
        applyResult({ reaction, active: false, count: removedCount });
        // 乐观从用户列表中移除当前用户的该项反应
        if (currentUser) {
          rxUsers = rxUsers.filter(
            (u) =>
              !(
                sameReaction(u.reaction, reaction) &&
                (u.user_id === currentUser.id || u.username === currentUser.username)
              )
          );
        }
        // 响应含完整 counts 时以服务端为准回同步
        if (result?.counts && typeof result.counts === 'object') {
          resyncFromCounts(result.counts);
        }
        onReactionMutated?.({ reaction, active: false, count: removedCount, counts: result?.counts });
      } else {
        // 单激活：一人只保留一个激活反应。若已有其他激活项，先乐观切换（移除旧的）。
        const prevActive = items.find((r) => r.active && !sameReaction(r.reaction, reaction));
        if (prevActive) {
          prevSnapshot = { reaction: prevActive.reaction, count: prevActive.count };
          optimisticRemoveItem(prevActive);
        }
        const result =
          targetType === 'post'
            ? await addPostReaction(fetchFn, targetId, reaction)
            : await addCommentReaction(fetchFn, targetId, reaction);
        const resolvedCount =
          typeof result?.count === 'number' ? result.count : (current?.count ?? 0) + 1;
        const resolvedActive = typeof result?.active === 'boolean' ? result.active : true;
        applyResult({ reaction, active: resolvedActive, count: resolvedCount });
        // 乐观将当前用户反应添加到弹窗列表前列
        if (currentUser) {
          const newItem: ReactionUserItem = {
            user_id: currentUser.id || 'me',
            username: currentUser.username || 'me',
            display_name: currentUser.display_name || currentUser.username || '我',
            reaction,
            created_at: Date.now()
          };
          rxUsers = [
            newItem,
            ...rxUsers.filter(
              (u) => !(u.user_id === newItem.user_id && sameReaction(u.reaction, reaction))
            )
          ];
        }
        // 响应含完整 counts 时以服务端为准回同步（单激活：除新反应外不再激活）
        if (result?.counts && typeof result.counts === 'object') {
          resyncFromCounts(result.counts);
          items = items.map((r) =>
            sameReaction(r.reaction, reaction) ? r : { ...r, active: false }
          );
        }
        onReactionMutated?.({
          reaction,
          active: resolvedActive,
          count: resolvedCount,
          counts: result?.counts
        });
      }
    } catch (err: unknown) {
      // 请求失败：回滚乐观切换
      if (prevSnapshot) {
        applyResult({ reaction: prevSnapshot.reaction, active: true, count: prevSnapshot.count });
      }
      const problem = err as Problem;
      if (problem?.status === 429) {
        const wait = retryAfterOf(problem);
        cooldownUntil = Date.now() + (wait ?? 60) * 1000;
        cooldownLeft = wait ?? 60;
        errorText = `操作过于频繁，请 ${cooldownLeft} 秒后再试`;
      } else if (problem?.status === 403) {
        errorText = '你没有权限对此内容使用反应';
      } else if (problem?.status === 401) {
        errorText = '登录状态已失效，请重新登录';
      } else {
        errorText = problemMessage(problem);
      }
    } finally {
      busyReaction = null;
    }
  }

  function pickReaction(reactionKey: string) {
    pickerOpen = false;
    toggle(reactionKey);
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      if (pickerOpen) pickerOpen = false;
      if (detailOpen) detailOpen = false;
    }
  }

  function handleWindowClick(event: MouseEvent) {
    if (!barContainer) return;
    const target = event.target as Node | null;
    if (!target) return;
    if (barContainer.contains(target)) return;
    // 弹层经 portal 挂在 body 下：弹层内部的点击（选表情/切 Tab）不视为「外部点击」
    if (pickerPopEl?.contains(target) || detailPopEl?.contains(target)) return;
    pickerOpen = false;
    detailOpen = false;
  }
</script>

<svelte:window
  onkeydown={handleKeydown}
  onclick={handleWindowClick}
  onscroll={positionActivePopovers}
  onresize={positionActivePopovers}
/>

<div class="reaction-bar" bind:this={barContainer}>
  {#if errorText}
    <p class="input-hint is-error" role="alert">{errorText}</p>
  {/if}

  <div class="reaction-bar-inner">
    <!-- 左侧：收到的反应。无反应时不展示任何图标；有反应时仅展示「图标 + 次数」，
         点击图标弹出明细弹窗（Tab 切换 + 用户列表）；添加/撤销走右侧「+ 表情」选择器。 -->
    <div class="reaction-bar-left">
      {#if activeReactions.length > 0}
        <div class="rx-pills" role="group" aria-label="收到的反应">
          {#each activeReactions as item (item.reaction)}
            {@const def = getDef(item.reaction)}
            <button
              type="button"
              class="rx-pill {detailOpen && activeTab === item.reaction ? 'is-open' : ''}"
              aria-label="查看 {def.label} 反应明细（{item.count} 次）"
              aria-haspopup="dialog"
              aria-expanded={detailOpen && activeTab === item.reaction}
              onclick={(e) => openDetail(item.reaction, e.currentTarget)}
            >
              {#if def.customIcon === 'doge'}
                <DogeIcon size={16} />
              {:else if def.customIcon === 'huaji'}
                <HuajiIcon size={16} />
              {:else}
                <span class="rx-pill-emoji" aria-hidden="true">{def.emoji}</span>
              {/if}
              <span class="rx-pill-count">{item.count}</span>
            </button>
          {/each}
        </div>
      {/if}

      {#if pickerOpen || detailOpen}
        <!-- 手机端 Bottom Sheet scrim（桌面 display:none，docs/MOBILE-SHEET.md） -->
        <button
          type="button"
          class="app-sheet-backdrop"
          aria-label="关闭弹层"
          onclick={() => {
            pickerOpen = false;
            detailOpen = false;
          }}
        ></button>
      {/if}

      {#if detailOpen}
        <!-- 反应用户明细弹窗（Tab 切换 + 用户列表） -->
        <div
          class="reaction-detail-popover"
          role="dialog"
          aria-label="收到的表情"
          aria-modal={isNarrow ? 'true' : undefined}
          use:portalPopover={{ getTrigger: () => detailTrigger, align: 'left', register: (el) => (detailPopEl = el) }}
        >
          <div class="app-sheet-grab" aria-hidden="true" use:sheetDrag={{ onClose: () => (detailOpen = false) }}></div>
          <!-- 顶部 Tab 切换：所有 + 收到各表情 -->
          <div class="rx-popover-header">
            <button
              type="button"
              class="rx-popover-tab {activeTab === 'all' ? 'is-active' : ''}"
              onclick={() => (activeTab = 'all')}
            >
              所有
            </button>
            {#each activeReactions as item (item.reaction)}
              {@const def = getDef(item.reaction)}
              <button
                type="button"
                class="rx-popover-tab {activeTab === item.reaction ? 'is-active' : ''}"
                aria-label="{def.label}（{item.count} 次）"
                onclick={() => (activeTab = item.reaction)}
              >
                {#if def.customIcon === 'doge'}
                  <DogeIcon size={15} />
                {:else if def.customIcon === 'huaji'}
                  <HuajiIcon size={15} />
                {:else}
                  <span class="rx-tab-emoji">{def.emoji}</span>
                {/if}
                <span class="rx-tab-count">{item.count}</span>
              </button>
            {/each}
          </div>

          <!-- 表情用户明细列表 -->
          <div class="rx-popover-user-list">
            {#if detailLoading && rxUsers.length === 0}
              <div class="rx-popover-status">加载中…</div>
            {:else if filteredUsers.length === 0}
              <div class="rx-popover-status">暂无该表态用户</div>
            {:else}
              {#each filteredUsers as u (u.user_id + u.reaction + u.created_at)}
                {@const def = getDef(u.reaction)}
                <div class="rx-user-row">
                  <div class="rx-user-meta">
                    <Avatar name={u.display_name || u.username} size="sm" seed={u.username ?? u.user_id} />
                    <div class="rx-user-names">
                      <span class="rx-user-display-name">{u.display_name || u.username}</span>
                      <span class="rx-user-handle">@{u.username}</span>
                    </div>
                  </div>
                  <div class="rx-user-emoji" title={def.label}>
                    {#if def.customIcon === 'doge'}
                      <DogeIcon size={20} />
                    {:else if def.customIcon === 'huaji'}
                      <HuajiIcon size={20} />
                    {:else}
                      <span class="rx-row-emoji">{def.emoji}</span>
                    {/if}
                  </div>
                </div>
              {/each}
            {/if}
          </div>
        </div>
      {/if}
    </div>

    <!-- 右侧：点击发表情 + 其他工具按钮 -->
    <div class="reaction-bar-right">
      <div class="reaction-picker-wrapper">
        <button
          type="button"
          class="reaction-add-btn {pickerOpen ? 'is-open' : ''}"
          aria-label={authed ? '添加表情反应' : '登录后添加表情反应'}
          title={authed ? '选择表情互动（含滑稽狗头、滑稽脸等）' : '登录后可参与表情互动'}
          aria-haspopup="dialog"
          aria-expanded={pickerOpen}
          disabled={busyReaction !== null || cooldownLeft > 0}
          bind:this={pickerBtn}
          onclick={() => {
            pickerOpen = !pickerOpen;
            if (pickerOpen) detailOpen = false;
          }}
        >
          <Icon name="smile" size={15} />
          <span class="reaction-add-label">{authed ? '+ 表情' : '表情'}</span>
        </button>

        {#if pickerOpen}
          <div
            class="reaction-picker-popover"
            role="dialog"
            aria-label="选择表情"
            aria-modal={isNarrow ? 'true' : undefined}
            use:portalPopover={{ getTrigger: () => pickerBtn, align: 'right', register: (el) => (pickerPopEl = el) }}
          >
            <div class="app-sheet-grab" aria-hidden="true" use:sheetDrag={{ onClose: () => (pickerOpen = false) }}></div>
            <div class="reaction-picker-header">
              <span class="reaction-picker-title">添加互动表态</span>
            </div>
            {#if !authed}
              <div style="padding: 16px 14px; text-align: center;">
                <p style="margin: 0 0 10px; font-size: var(--text-sm); color: var(--color-text-secondary);">
                  登录后即可给内容添加表情互动
                </p>
                <a
                  class="btn primary sm"
                  href="/login?next={encodeURIComponent(typeof window !== 'undefined' ? window.location.pathname + window.location.search : '')}"
                  style="text-decoration: none; display: inline-flex;"
                >
                  去登录
                </a>
              </div>
            {:else}
              <div class="reaction-picker-grid">
                {#each AVAILABLE_REACTIONS as r (r.key)}
                  {@const active = items.find(
                    (i) => (i.reaction === r.key || i.reaction === r.emoji) && i.active
                  )}
                  <button
                    type="button"
                    class="reaction-picker-item {active ? 'is-active' : ''}"
                    title="{r.label} ({r.desc})"
                    aria-label="{r.label}"
                    onclick={() => pickReaction(r.key)}
                  >
                    <span class="reaction-picker-icon">
                      {#if r.customIcon === 'doge'}
                        <DogeIcon size={26} />
                      {:else if r.customIcon === 'huaji'}
                        <HuajiIcon size={26} />
                      {:else}
                        <span class="reaction-picker-emoji">{r.emoji}</span>
                      {/if}
                    </span>
                    <span class="reaction-picker-name">{r.label}</span>
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        {/if}
      </div>

      {#if rightActions}
        <div class="reaction-extra-actions">
          {@render rightActions()}
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .reaction-bar {
    position: relative;
    width: 100%;
  }

  .reaction-bar-inner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--space-2, 8px);
    width: 100%;
  }

  /* 左侧：展示收到的表情 */
  .reaction-bar-left {
    display: flex;
    align-items: center;
    gap: var(--space-2, 8px);
    flex-wrap: wrap;
    position: relative;
  }

  /* 右侧：发表情与扩展操作 */
  .reaction-bar-right {
    display: flex;
    align-items: center;
    gap: var(--space-2, 8px);
    margin-left: auto;
    position: relative;
  }

  .reaction-extra-actions {
    display: flex;
    align-items: center;
    gap: var(--space-2, 8px);
  }

  /* 收到的反应：紧凑「图标 + 次数」Pill，点击展开明细弹窗 */
  .rx-pills {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .rx-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 10px;
    /* 圆角跟随主题 radius.control（--radius-sm），与「+ 表情」按钮及全局控件一致；
       不再硬编码 999px 胶囊形——零圆角主题下保持直角。 */
    border-radius: var(--radius-sm, 6px);
    border: 1px solid var(--color-border, #DFE3E7);
    background: var(--color-bg-subtle, #F1F3F5);
    color: var(--color-text-secondary, #545C68);
    font-size: 13px;
    font-weight: var(--weight-medium, 500);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .rx-pill:hover,
  .rx-pill.is-open {
    background: var(--color-surface-hover, #F1F3F5);
    border-color: var(--color-border-strong, #B8C0C9);
  }

  .rx-pill:focus-visible {
    outline: 2px solid var(--color-brand, #2C4BD8);
    outline-offset: 1px;
  }

  .rx-pill-emoji {
    display: inline-block;
    font-size: 14px;
    line-height: 1;
  }

  .rx-pill-count {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-secondary, #545C68);
  }

  /* 参考截图的表情详情弹窗（portal 到 body + fixed 定位，left/top 由 JS 按触发元素写入）。
     浮层表面统一走语义 token：下拉/模态用 --color-bg-raised（tokens.css 设计约定），
     边框/阴影/文字全部跟随日夜与数据型主题，不再硬编码暗色覆盖。 */
  .reaction-detail-popover {
    position: fixed;
    left: 0;
    top: 0;
    width: 290px;
    max-width: 90vw;
    background: var(--color-bg-raised, #FFFFFF);
    border: 1px solid var(--color-border, #DFE3E7);
    border-radius: var(--radius-md, 10px);
    box-shadow: var(--shadow-pop, 0 12px 32px rgba(0, 0, 0, 0.16));
    z-index: var(--z-dropdown, 100);
    padding: 8px 0;
    display: flex;
    flex-direction: column;
    animation: popoverFadeIn 0.15s ease-out;
  }

  @keyframes popoverFadeIn {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  /* 顶部 Tab 栏 */
  .rx-popover-header {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px 8px 8px;
    border-bottom: 1px solid var(--color-border-muted, #E7EAEE);
    overflow-x: auto;
    scrollbar-width: none;
  }

  .rx-popover-header::-webkit-scrollbar {
    display: none;
  }

  .rx-popover-tab {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 9px;
    border-radius: var(--radius-sm, 6px);
    border: none;
    background: transparent;
    color: var(--color-text-secondary, #545C68);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
    transition: all 0.15s ease;
  }

  .rx-popover-tab:hover {
    background: var(--color-surface-hover, #F1F3F5);
    color: var(--color-text-primary, #10141A);
  }

  /* 选中 Tab：品牌色当前态（双模式自适应——亮底深强调白字 / 深底浅强调深字） */
  .rx-popover-tab.is-active {
    background: var(--color-brand, #2C4BD8);
    color: var(--color-text-on-brand, #FFFFFF);
  }

  .rx-tab-count {
    font-size: 11px;
    opacity: 0.9;
  }

  /* 用户明细列表 */
  .rx-popover-user-list {
    max-height: 260px;
    overflow-y: auto;
    padding: 6px;
  }

  .rx-popover-status {
    padding: 16px;
    text-align: center;
    color: var(--color-text-tertiary, #737373);
    font-size: 12px;
  }

  .rx-user-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 8px;
    border-radius: var(--radius-sm, 6px);
    transition: background 0.12s ease;
  }

  .rx-user-row:hover {
    background: var(--color-surface-hover, #F1F3F5);
  }

  .rx-user-meta {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .rx-user-names {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.25;
  }

  .rx-user-display-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-primary, #10141A);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .rx-user-handle {
    font-size: 11px;
    color: var(--color-text-tertiary, #737373);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .rx-user-emoji {
    margin-left: 8px;
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 18px;
    line-height: 1;
  }

  /* 表情选择器包裹层 */
  .reaction-picker-wrapper {
    position: relative;
    display: inline-flex;
    align-items: center;
  }

  .reaction-add-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--color-border, #DFE3E7);
    border-radius: var(--radius-sm, 6px);
    background: transparent;
    color: var(--color-text-secondary, #545C68);
    cursor: pointer;
    font-size: 13px;
    transition: all 0.15s;
  }

  .reaction-add-btn:hover,
  .reaction-add-btn.is-open {
    border-color: var(--color-brand, #2C4BD8);
    color: var(--color-brand, #2C4BD8);
    background: var(--color-surface-hover, #F1F3F5);
  }

  .reaction-add-label {
    font-size: 12px;
    font-weight: 500;
  }

  /* 快速表情选择弹窗（portal 到 body + fixed 定位，left/top 由 JS 按触发元素写入） */
  .reaction-picker-popover {
    position: fixed;
    left: 0;
    top: 0;
    width: 290px;
    max-width: 90vw;
    background: var(--color-bg-raised, #FFFFFF);
    border: 1px solid var(--color-border, #DFE3E7);
    border-radius: var(--radius-md, 8px);
    box-shadow: var(--shadow-pop, 0 10px 25px rgba(0, 0, 0, 0.16));
    z-index: var(--z-dropdown, 100);
    padding: 10px;
    animation: popoverFadeIn 0.15s ease-out;
  }

  .reaction-picker-header {
    padding-bottom: 6px;
    margin-bottom: 6px;
    border-bottom: 1px solid var(--color-border-muted, #E7EAEE);
  }

  .reaction-picker-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-secondary, #545C68);
  }

  .reaction-picker-grid {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: 6px;
  }

  .reaction-picker-item {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    padding: 6px 2px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm, 6px);
    background: transparent;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .reaction-picker-item:hover {
    background: var(--color-surface-hover, #F1F3F5);
    border-color: var(--color-border, #DFE3E7);
    transform: translateY(-1px);
  }

  .reaction-picker-item.is-active {
    background: var(--color-brand-soft, #F1F3F5);
    border-color: var(--color-brand, #2C4BD8);
  }

  .reaction-picker-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
  }

  .reaction-picker-emoji {
    font-size: 24px;
    line-height: 1;
  }

  .reaction-picker-name {
    font-size: 11px;
    color: var(--color-text-secondary, #545C68);
    white-space: nowrap;
  }

  .input-hint {
    margin-top: 6px;
    font-size: 12px;
    color: var(--color-text-tertiary, #737373);
  }

  .input-hint.is-error {
    color: var(--color-danger, #cf222e);
  }
</style>
