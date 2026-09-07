<script lang="ts">
  // GAP-FIX（社交域·私信）：/messages——SSR 双栏（桌面左会话右线程；
  // ≤768px 单栏切换：列出会话，点进 ?c= 后显示线程 + 返回按钮）。
  //
  // - 无 JS 基线：会话/线程由 load SSR 直出；发送为原生 form POST；
  // - use:enhance：成功 → toast + invalidateAll 刷新线程并滚到底部；
  //   失败 → toast 错误文案；
  // - 空态：「还没有私信，去关注的人那里打个招呼吧」。
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import Button from '$lib/components/ui/Button.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import LoadFailureState from '$lib/components/LoadFailureState.svelte';
  import ProblemState from '$lib/components/ProblemState.svelte';
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import { isTransientProblem } from '$lib/errors';
  import { announceTransientProblem } from '$lib/ui/problem-toast';
  import { show } from '$lib/ui/toast';
  import { formatRelative, formatTime } from '$lib/utils';
  import type { MessagesActionData, MessagesPageData } from './+page.server';

  let { data, form }: { data: MessagesPageData; form?: MessagesActionData | null } = $props();

  let threadBody = $state<HTMLDivElement | null>(null);
  let sending = $state(false);

  /** 后端时间戳为毫秒（M01-DB-08），formatRelative/formatTime 口径为秒。 */
  function toSeconds(ts: number | null | undefined): number | null {
    if (typeof ts !== 'number' || !Number.isFinite(ts) || ts <= 0) return null;
    return ts > 1e11 ? Math.floor(ts / 1000) : ts;
  }

  function scrollToBottom(): void {
    if (threadBody) threadBody.scrollTop = threadBody.scrollHeight;
  }

  // 线程数据变化（首次进入 / 发送成功 invalidate 后）滚到底部。
  $effect(() => {
    void data.messages.length;
    void data.conversationId;
    scrollToBottom();
  });

  // 瞬态服务端错误（5xx/429）→ 全局 Toast 提示 + 页面只留「加载失败·重试」
  // 占位（产品约定：不整页展示错误态）；持续性错误仍走 ProblemState。
  // threadProblem 与 problem 各自独立 announce（WeakSet 按对象去重）。
  $effect(() => {
    void data.problem;
    void data.threadProblem;
    announceTransientProblem(data.problem);
    announceTransientProblem(data.threadProblem);
  });

  const otherLabel = $derived(
    data.conversation
      ? data.conversation.other.display_name || data.conversation.other.username
      : (data.conversationId ? '对方' : '')
  );
  const actionMessage = $derived(form?.message ?? null);
</script>

<svelte:head>
  <title>消息 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <!-- 原型对齐（prototype/pages/messages.html）：页头为 app-route-head（INBOX / MESSAGES + h1 消息），无面包屑。 -->
  <div class="app-route-head">
    <div class="app-route-head__copy">
      <span class="app-kicker">INBOX / MESSAGES</span>
      <h1 tabindex="-1">消息</h1>
      <p>私信会话、发送状态与失败重试</p>
    </div>
  </div>

  {#if data.problem && isTransientProblem(data.problem)}
    <LoadFailureState onretry={() => void invalidateAll()} />
  {:else if data.problem}
    <ProblemState problem={data.problem} />
  {:else}
    <div class="messages-layout">
      <!-- 左栏：会话列表（≤768px 时选中会话后隐藏） -->
      <aside class="messages-pane" class:is-hidden-mobile={data.conversationId !== null}>
        <div class="card">
          <div class="card-header"><span class="card-title">会话</span></div>
          <div class="card-body" style="padding:0;">
            {#if data.conversations.length === 0}
              <div style="padding:var(--space-6);">
                <EmptyState
                  icon="message-circle"
                  title="还没有私信"
                  desc="去关注的人那里打个招呼吧"
                />
                <div style="text-align:center;margin-top:var(--space-3);">
                  <Button text="去逛逛社区" variant="secondary" size="sm" href="/" />
                </div>
              </div>
            {:else}
              <ul class="messages-list" style="margin:0;padding:0;list-style:none;">
                {#each data.conversations as c (c.id)}
                  <li>
                    <a
                      class="messages-item"
                      class:is-active={c.id === data.conversationId}
                      href="/messages?c={encodeURIComponent(c.id)}"
                      aria-current={c.id === data.conversationId ? 'page' : undefined}
                    >
                      <Avatar name={c.other.username} size="lg" title={c.other.username} />
                      <span class="messages-item-main">
                        <span class="messages-item-top">
                          <span class="messages-item-name">{c.other.display_name || c.other.username}</span>
                          {#if c.unread_count > 0}
                            <span class="badge badge-danger">{c.unread_count}</span>
                          {/if}
                        </span>
                        <span class="messages-item-preview">{c.last_message?.body ?? '（暂无消息）'}</span>
                      </span>
                      <span class="messages-item-time text-secondary">
                        {formatRelative(toSeconds(c.updated_at))}
                      </span>
                    </a>
                  </li>
                {/each}
              </ul>
            {/if}
          </div>
        </div>
      </aside>

      <!-- 右栏：线程（≤768px 时未选会话隐藏） -->
      <section class="messages-pane" class:is-hidden-mobile={data.conversationId === null}>
        {#if data.conversationId === null}
          <div class="card">
            <div class="card-body">
              <EmptyState icon="message-square" title="选择一个会话" desc="从左侧选择会话开始聊天" />
            </div>
          </div>
        {:else}
          <div class="card messages-thread">
            <div class="card-header messages-thread-header">
              <!-- ≤768px 返回会话列表（无 JS 可用，普通链接） -->
              <a href="/messages" class="btn btn-ghost btn-sm messages-back">← 返回</a>
              <span class="card-title">{otherLabel}</span>
            </div>
            <div class="card-body" style="padding:0;display:flex;flex-direction:column;">
              {#if data.threadProblem && isTransientProblem(data.threadProblem)}
                <div style="padding:var(--space-4);">
                  <LoadFailureState title="会话加载失败" onretry={() => void invalidateAll()} />
                </div>
              {:else if data.threadProblem}
                <div style="padding:var(--space-4);">
                  <ProblemState problem={data.threadProblem} title="会话加载失败" />
                </div>
              {:else}
                {#if data.messages.length === 0}
                  <div style="padding:var(--space-6);">
                    <EmptyState icon="message-square" title="会话还没有消息" desc="说点什么吧" />
                  </div>
                {:else}
                  <div class="messages-body" bind:this={threadBody}>
                    {#each data.messages as m (m.id)}
                      <!-- 双人会话：非对方发送即本人发送（无需再取 viewer 投影）。 -->
                      {@const own = !data.conversation || m.sender_username !== data.conversation.other.username}
                      <div class="msg" class:is-own={own}>
                        {m.body}
                        <span class="msg-time">{formatTime(toSeconds(m.created_at))}</span>
                      </div>
                    {/each}
                  </div>
                {/if}
                <form
                  class="messages-compose"
                  method="POST"
                  action="?/send"
                  use:enhance={() => {
                    sending = true;
                    return async ({ result, update }) => {
                      sending = false;
                      if (result.type === 'success') {
                        show('私信已发送', 'success');
                        await update();
                        // 刷新线程（load 重跑，readConversation 重置未读）。
                        await invalidateAll();
                        scrollToBottom();
                      } else if (result.type === 'failure') {
                        show(
                          String(
                            (result.data as MessagesActionData | undefined)?.message ?? '发送失败'
                          ),
                          'danger'
                        );
                        await update();
                      } else {
                        await update();
                      }
                    };
                  }}
                >
                  {#if actionMessage}
                    <p class="input-hint" role="status" style="margin:0 0 var(--space-2);">{actionMessage}</p>
                  {/if}
                  <input type="hidden" name="conversation_id" value={data.conversationId ?? ''} />
                  <input type="hidden" name="client_request_id" value={data.clientRequestId} />
                  <label class="u-visually-hidden" for="messages-body-input">私信内容</label>
                  <textarea
                    id="messages-body-input"
                    class="input-field"
                    name="body"
                    rows="3"
                    maxlength="2000"
                    required
                    placeholder="输入私信内容（1-2000 字）…"
                  ></textarea>
                  <div style="display:flex;justify-content:flex-end;margin-top:var(--space-2);">
                    <Button
                      type="submit"
                      text={sending ? '发送中…' : '发送'}
                      variant="primary"
                      size="sm"
                      disabled={sending}
                    />
                  </div>
                </form>
              {/if}
            </div>
          </div>
        {/if}
      </section>
    </div>
  {/if}
</div>

<style>
  .messages-layout {
    display: grid;
    grid-template-columns: 320px minmax(0, 1fr);
    gap: var(--space-5);
    align-items: start;
  }

  .messages-item {
    display: flex;
    gap: var(--space-3);
    align-items: center;
    padding: var(--space-3) var(--space-4);
    border-bottom: var(--border-default);
    text-decoration: none;
    color: inherit;
  }

  .messages-item:hover {
    background: var(--color-bg-subtle);
  }

  .messages-item.is-active {
    background: var(--color-bg-inset);
  }

  .messages-item-main {
    min-width: 0;
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .messages-item-top {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .messages-item-name {
    font-weight: var(--weight-medium);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .messages-item-preview {
    color: var(--color-text-secondary);
    font-size: var(--text-sm);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .messages-item-time {
    flex-shrink: 0;
    font-size: var(--text-xs);
  }

  .messages-thread-header {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .messages-back {
    display: none;
  }

  .messages-body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
    max-height: 480px;
    overflow-y: auto;
    border-bottom: var(--border-default);
  }

  .msg {
    max-width: 78%;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    background: var(--color-bg-subtle);
    font-size: var(--text-sm);
    white-space: pre-wrap;
    word-break: break-word;
  }

  .msg.is-own {
    align-self: flex-end;
    background: var(--color-accent-soft);
  }

  .msg-time {
    display: block;
    margin-top: var(--space-1);
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
  }

  .messages-compose {
    padding: var(--space-3) var(--space-4);
    display: flex;
    flex-direction: column;
  }

  @media (max-width: 768px) {
    .messages-layout {
      grid-template-columns: 1fr;
    }

    .messages-pane.is-hidden-mobile {
      display: none;
    }

    .messages-back {
      display: inline-flex;
    }
  }
</style>
