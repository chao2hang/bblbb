<!-- M07-UI-07：Reaction 选择/撤销组件。
  - 点击已激活反应 = 撤销（DELETE），未激活 = 添加（POST，body 含 reaction）。
  - 429 限流：显示 Retry-After 秒数并禁用按钮（冷却倒计时）。
  - 403 目标权限错误 / 401 未登录：分别给出指引。
  - 通知偏好：提示反应可能通知作者（可在 /notifications 偏好中关闭）。
  - 键盘：原生 <button>（Enter/Space 激活）。
-->
<script lang="ts">
  import { addCommentReaction, addPostReaction, removeCommentReaction, removePostReaction } from '$lib/api/client';
  import { problemMessage, retryAfterOf, type Problem } from '$lib/errors';

  let {
    targetType,
    targetId,
    reactions = [],
    authed = true,
    fetchFn = fetch,
    notificationUrl = '/notifications'
  }: {
    targetType: 'post' | 'comment';
    targetId: string;
    /** 初始计数 [{reaction, count, active}]。 */
    reactions?: Array<{ reaction: string; count: number; active?: boolean }>;
    authed?: boolean;
    fetchFn?: typeof fetch;
    notificationUrl?: string;
  } = $props();

  let items = $state<Array<{ reaction: string; count: number; active: boolean }>>([]);
  // 初始化 items（在组件挂载时反映初始 reactions，后续通过 applyResult 更新）。
  let initialized = false;
  $effect(() => {
    if (!initialized) {
      items = reactions.map((r) => ({ ...r, active: Boolean(r.active) }));
      initialized = true;
    }
  });
  let busyReaction = $state<string | null>(null);
  let errorText = $state('');
  let cooldownUntil = $state<number>(0);
  let cooldownLeft = $state(0);

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

  function applyResult(result: { reaction: string; active: boolean; count: number }) {
    items = items.map((r) =>
      r.reaction === result.reaction ? { ...r, count: result.count, active: result.active } : r
    );
  }

  async function toggle(reaction: string) {
    errorText = '';
    if (!authed) {
      errorText = '请先登录后再使用反应';
      return;
    }
    const current = items.find((r) => r.reaction === reaction);
    if (!current) return;
    if (busyReaction) return;
    busyReaction = reaction;
    try {
      if (current.active) {
        if (targetType === 'post') {
          await removePostReaction(fetchFn, targetId, reaction);
        } else {
          await removeCommentReaction(fetchFn, targetId, reaction);
        }
        applyResult({ reaction, active: false, count: Math.max(0, current.count - 1) });
      } else {
        const result =
          targetType === 'post'
            ? await addPostReaction(fetchFn, targetId, reaction)
            : await addCommentReaction(fetchFn, targetId, reaction);
        applyResult(result);
      }
    } catch (err: unknown) {
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
</script>

<div class="reaction-bar">
  {#if errorText}
    <p class="input-hint is-error" role="alert">{errorText}</p>
  {/if}
  <div class="reaction-items" role="group" aria-label="反应">
    {#each items as item (item.reaction)}
      <button
        type="button"
        class="reaction-btn {item.active ? 'is-active' : ''}"
        aria-pressed={item.active}
        aria-label={item.active ? `撤销反应 ${item.reaction}` : `添加反应 ${item.reaction}`}
        disabled={busyReaction !== null || cooldownLeft > 0}
        onclick={() => toggle(item.reaction)}
      >
        <span aria-hidden="true">{item.reaction}</span>
        <span class="reaction-count" aria-hidden="true">{item.count}</span>
      </button>
    {/each}
  </div>
  <p class="input-hint">
    反应可能通知作者，可在<a href={notificationUrl}>通知设置</a>中关闭
  </p>
</div>

<style>
  .reaction-items {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .reaction-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    border: 1px solid var(--color-border, #d0d7de);
    border-radius: 999px;
    background: var(--color-surface, #fff);
    cursor: pointer;
    font-size: var(--text-sm, 14px);
  }
  .reaction-btn.is-active {
    border-color: var(--color-primary, #0969da);
    background: color-mix(in srgb, var(--color-primary, #0969da) 10%, #fff);
  }
  .reaction-count {
    font-variant-numeric: tabular-nums;
    color: var(--color-text-secondary, #57606a);
  }
  .reaction-btn:focus-visible {
    outline: 2px solid var(--color-primary, #0969da);
    outline-offset: 1px;
  }
</style>
