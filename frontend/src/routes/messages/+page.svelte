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
  import CosmeticAvatar from '$lib/components/wardrobe/CosmeticAvatar.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { page } from '$app/state';
  import { isTransientProblem } from '$lib/errors';
  import { announceTransientProblem } from '$lib/ui/problem-toast';
  import { show } from '$lib/ui/toast';
  import { formatChatTime, formatRelative } from '$lib/utils';
  import type { ConversationMessage } from '$lib/api/types';
  import type { MessagesActionData, MessagesPageData } from './+page.server';
  import PageTitle from '$lib/components/PageTitle.svelte';

  let { data, form }: { data: MessagesPageData; form?: MessagesActionData | null } = $props();

  let threadBody = $state<HTMLDivElement | null>(null);
  let composeInput = $state<HTMLTextAreaElement | null>(null);
  let sending = $state(false);

  let activeQuote = $state<{ id: string; sender: string; body: string } | null>(null);
  let activeMenuMsgId = $state<string | null>(null);
  let recallingId = $state<string | null>(null);
  interface RecalledNotice {
    id: string;
    text: string;
  }
  let recalledList = $state<RecalledNotice[]>([]);

  interface ParsedMessage {
    quote: { sender: string; body: string } | null;
    text: string;
  }

  function parseQuote(body: string): ParsedMessage {
    const match = body.match(/^(?:> ?)?「(?:引用\s*)?([^：:\n]+)[：:]([\s\S]*?)」\n([\s\S]*)$/);
    if (match) {
      return {
        quote: {
          sender: match[1].trim(),
          body: match[2].trim()
        },
        text: match[3]
      };
    }
    return { quote: null, text: body };
  }

  function startQuote(m: ConversationMessage, senderDisplayName: string) {
    const preview = m.body.replace(/^(?:> ?)?「[\s\S]*?」\n/, '').slice(0, 80);
    activeQuote = {
      id: m.id,
      sender: senderDisplayName,
      body: preview
    };
    activeMenuMsgId = null;
    if (composeInput) {
      composeInput.focus();
    }
  }

  function cancelQuote() {
    activeQuote = null;
  }

  function canRecall(m: ConversationMessage, own: boolean): boolean {
    if (!own) return false;
    const ts = toSeconds(m.created_at);
    if (ts === null) return false;
    const elapsed = Date.now() - ts * 1000;
    return elapsed >= 0 && elapsed <= 120_000;
  }

  async function handleRecall(msgId: string, msgBody: string) {
    activeMenuMsgId = null;
    recallingId = msgId;
    const fd = new FormData();
    fd.append('conversation_id', data.conversationId ?? '');
    fd.append('message_id', msgId);
    try {
      const res = await fetch('?/recall', {
        method: 'POST',
        body: fd
      });
      const result = await res.json();
      if (result.type === 'success') {
        show('消息已撤回', 'success');
        recalledList = [...recalledList, { id: msgId, text: msgBody }];
        await invalidateAll();
      } else {
        show('撤回失败，可能已超过 2 分钟', 'danger');
      }
    } catch {
      show('网络请求失败', 'danger');
    } finally {
      recallingId = null;
    }
  }

  function toggleMenu(msgId: string, e: MouseEvent) {
    e.stopPropagation();
    activeMenuMsgId = activeMenuMsgId === msgId ? null : msgId;
  }

  function closeMenu() {
    activeMenuMsgId = null;
  }

  function getViewerUser() {
    try {
      return page.data?.user ?? null;
    } catch {
      return null;
    }
  }
  const viewerUser = $derived(getViewerUser());
  const userProfiles = $derived(data.userProfiles ?? {});

  function getUserPresentation(username: string | null | undefined) {
    if (!username) return null;
    if (viewerUser && viewerUser.username === username && viewerUser.presentation_tokens) {
      return viewerUser.presentation_tokens;
    }
    return userProfiles[username]?.presentation_tokens ?? null;
  }

  function getUserAvatarId(username: string | null | undefined) {
    if (!username) return null;
    if (viewerUser && viewerUser.username === username && viewerUser.avatar_attachment_id) {
      return viewerUser.avatar_attachment_id;
    }
    return userProfiles[username]?.avatar_attachment_id ?? null;
  }

  function getUserDisplayName(username: string | null | undefined, fallback?: string | null) {
    if (!username) return fallback ?? '?';
    if (viewerUser && viewerUser.username === username) {
      return viewerUser.display_name || viewerUser.username || fallback || '我';
    }
    return userProfiles[username]?.display_name || userProfiles[username]?.username || fallback || username;
  }

  function handleComposeInput(e: Event): void {
    const el = e.currentTarget as HTMLTextAreaElement;
    el.style.height = 'auto';
    el.style.height = `${Math.min(el.scrollHeight, 120)}px`;
  }

  function handleComposeKeydown(e: KeyboardEvent): void {
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      const form = (e.currentTarget as HTMLTextAreaElement).form;
      form?.requestSubmit();
    }
  }

  /** 后端时间戳为毫秒（M01-DB-08），formatRelative/formatChatTime 口径为秒。 */
  function toSeconds(ts: number | null | undefined): number | null {
    if (typeof ts !== 'number' || !Number.isFinite(ts) || ts <= 0) return null;
    return ts > 1e11 ? Math.floor(ts / 1000) : ts;
  }

  /** 微信聊天时间展示规则：
   *  - 首条消息展示时间；
   *  - 后续消息与上一次展示时间的间隔超过 5 分钟（300 秒）时展示时间节点；
   *  - 气泡本身不嵌入时间，改为居中独立时间分隔线展示。
   */
  const messageTimeDisplays = $derived.by(() => {
    let lastShownTs = -Infinity;
    return data.messages.map((m) => {
      const ts = toSeconds(m.created_at);
      if (ts !== null && ts - lastShownTs >= 300) {
        lastShownTs = ts;
        return formatChatTime(ts);
      }
      return null;
    });
  });

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

<svelte:window onclick={closeMenu} />

<PageTitle title="消息" />

<div class="container page-content app-page" id="page-messages" class:in-thread={data.conversationId !== null}>
  <h1 class="u-visually-hidden">消息</h1>

  {#if data.problem && isTransientProblem(data.problem)}
    <LoadFailureState onretry={() => void invalidateAll()} />
  {:else if data.problem}
    <ProblemState problem={data.problem} />
  {:else}
    <div class="messages-layout">
      <!-- 左栏：会话列表（≤768px 时选中会话后隐藏） -->
      <aside class="messages-pane" class:is-hidden-mobile={data.conversationId !== null}>
        <div class="card">
          <div class="card-header messages-list-header"><span class="messages-list-title">会话</span></div>
          <div class="card-body" style="padding:0;">
            {#if data.conversations.length === 0}
              <div class="messages-empty">
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
                      data-username={c.other.username}
                      aria-current={c.id === data.conversationId ? 'page' : undefined}
                    >
                      <div class="messages-item-avatar-wrap">
                        <CosmeticAvatar
                          name={getUserDisplayName(c.other.username, c.other.display_name || c.other.username)}
                          size="lg"
                          presentation={getUserPresentation(c.other.username) ?? ('presentation_tokens' in c.other ? (c.other as any).presentation_tokens : null)}
                          avatarAttachmentId={getUserAvatarId(c.other.username) ?? ('avatar_attachment_id' in c.other ? (c.other as any).avatar_attachment_id : null)}
                          seed={c.other.username ?? ('id' in c.other ? (c.other as { id?: string }).id : undefined)}
                          username={c.other.username}
                          title={c.other.username}
                        />
                        {#if c.unread_count > 0}
                          <span class="badge badge-danger messages-unread-badge" aria-label="{c.unread_count} 条未读">
                            {c.unread_count > 99 ? '99+' : c.unread_count}
                          </span>
                        {/if}
                      </div>
                      <span class="messages-item-main">
                        <span class="messages-item-top">
                          <span class="messages-item-name">{c.other.display_name || c.other.username}</span>
                          <span class="messages-item-time text-secondary">
                            {formatRelative(toSeconds(c.updated_at))}
                          </span>
                        </span>
                        <span class="messages-item-preview">{c.last_message?.body ?? '（暂无消息）'}</span>
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
              <div class="messages-empty">
                <EmptyState icon="message-square" title="选择一个会话" desc="从左侧选择会话开始聊天" />
              </div>
            </div>
          </div>
        {:else}
          <div class="card messages-thread">
            <div class="card-header messages-thread-header">
              <!-- ≤768px 返回会话列表（无 JS 可用，普通链接） -->
              <a href="/messages" class="btn btn-ghost btn-sm messages-back" aria-label="返回会话列表">
                <Icon name="chevron-left" size={18} />
                <span>返回</span>
              </a>
              <span class="messages-thread-title">{otherLabel}</span>
            </div>
            <div class="card-body" style="padding:0;">
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
                  <div class="messages-empty">
                    <EmptyState icon="message-square" title="会话还没有消息" desc="说点什么吧" />
                  </div>
                {:else}
                  <div class="messages-body" bind:this={threadBody}>
                    {#each data.messages as m, i (m.id)}
                      <!-- 双人会话：非对方发送即本人发送（无需再取 viewer 投影）。 -->
                      {@const own = !data.conversation || m.sender_username !== data.conversation.other.username}
                      {@const senderDisplayName = getUserDisplayName(m.sender_username, own ? '我' : (data.conversation?.other.display_name || data.conversation?.other.username || m.sender_username))}
                      {@const parsed = parseQuote(m.body)}
                      {@const recallable = canRecall(m, own)}

                      {#if messageTimeDisplays[i]}
                        <div class="chat-time">
                          <span>{messageTimeDisplays[i]}</span>
                        </div>
                      {/if}

                      <div class="msg-row" class:is-own={own} id="msg-{m.id}">
                        {#if !own}
                          <div class="msg-avatar">
                            <CosmeticAvatar
                              name={senderDisplayName}
                              size="md"
                              presentation={getUserPresentation(m.sender_username)}
                              avatarAttachmentId={getUserAvatarId(m.sender_username)}
                              seed={m.sender_username}
                              username={m.sender_username}
                            />
                          </div>
                        {/if}

                        <div class="msg-wrapper">
                          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
                          <div
                            class="msg"
                            class:is-own={own}
                            onclick={(e) => toggleMenu(m.id, e)}
                          >
                            {#if parsed.quote}
                              <div class="msg-quote-card" class:is-own={own}>
                                <span class="msg-quote-name">{parsed.quote.sender}：</span>
                                <span class="msg-quote-text">{parsed.quote.body}</span>
                              </div>
                            {/if}
                            <div class="msg-text">{parsed.text}</div>
                          </div>

                          <!-- 快捷操作浮层（微信风格操作气泡：引用 / 撤回） -->
                          <div class="msg-actions" class:is-open={activeMenuMsgId === m.id}>
                            <button
                              type="button"
                              class="msg-action-btn"
                              onclick={(e) => { e.stopPropagation(); startQuote(m, senderDisplayName); }}
                              title="引用此消息"
                            >
                              <Icon name="reply" size={13} />
                              <span>引用</span>
                            </button>
                            {#if recallable}
                              <button
                                type="button"
                                class="msg-action-btn is-danger"
                                onclick={(e) => { e.stopPropagation(); handleRecall(m.id, m.body); }}
                                disabled={recallingId === m.id}
                                title="撤回（2分钟内可用）"
                              >
                                <Icon name="undo" size={13} />
                                <span>{recallingId === m.id ? '撤回中…' : '撤回'}</span>
                              </button>
                            {/if}
                          </div>
                        </div>

                        {#if own}
                          <div class="msg-avatar">
                            <CosmeticAvatar
                              name={senderDisplayName}
                              size="md"
                              presentation={getUserPresentation(m.sender_username)}
                              avatarAttachmentId={getUserAvatarId(m.sender_username)}
                              seed={m.sender_username}
                              username={m.sender_username}
                            />
                          </div>
                        {/if}
                      </div>
                    {/each}

                    {#each recalledList as item (item.id)}
                      <div class="chat-time chat-recalled-notice">
                        <span>你撤回了一条消息</span>
                        <button
                          type="button"
                          class="chat-reedit-btn"
                          onclick={() => {
                            const raw = item.text.replace(/^(?:> ?)?「[\s\S]*?」\n/, '');
                            if (composeInput) {
                              composeInput.value = raw;
                              composeInput.focus();
                              composeInput.dispatchEvent(new Event('input'));
                            }
                          }}
                        >
                          重新编辑
                        </button>
                      </div>
                    {/each}
                  </div>
                {/if}
                <form
                  class="messages-compose"
                  method="POST"
                  action="?/send"
                  use:enhance={({ formData }) => {
                    sending = true;
                    const rawBody = String(formData.get('body') ?? '').trim();
                    if (activeQuote && rawBody) {
                      formData.set('body', `「引用 ${activeQuote.sender}：${activeQuote.body}」\n${rawBody}`);
                    }
                    return async ({ result, update }) => {
                      sending = false;
                      if (result.type === 'success') {
                        show('私信已发送', 'success');
                        activeQuote = null;
                        if (composeInput) {
                          composeInput.value = '';
                          composeInput.style.height = 'auto';
                        }
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
                    <p class="input-hint messages-compose-hint" role="status">{actionMessage}</p>
                  {/if}
                  {#if activeQuote}
                    <div class="messages-quote-bar">
                      <div class="messages-quote-bar-content">
                        <span class="messages-quote-bar-tag">引用</span>
                        <span class="messages-quote-bar-sender">{activeQuote.sender}：</span>
                        <span class="messages-quote-bar-text">{activeQuote.body}</span>
                      </div>
                      <button type="button" class="messages-quote-bar-close" onclick={cancelQuote} aria-label="取消引用">
                        <Icon name="x" size={14} />
                      </button>
                    </div>
                  {/if}
                  <input type="hidden" name="conversation_id" value={data.conversationId ?? ''} />
                  <input type="hidden" name="client_request_id" value={data.clientRequestId} />
                  <div class="messages-compose-row">
                    <label class="u-visually-hidden" for="messages-body-input">私信内容</label>
                    <textarea
                      id="messages-body-input"
                      class="input-field messages-compose-textarea"
                      name="body"
                      rows="1"
                      maxlength="2000"
                      required
                      placeholder="发消息…"
                      bind:this={composeInput}
                      oninput={handleComposeInput}
                      onkeydown={handleComposeKeydown}
                    ></textarea>
                    <Button
                      type="submit"
                      text={sending ? '发送中…' : '发送'}
                      variant="primary"
                      size="sm"
                      disabled={sending}
                      extraClass="messages-compose-submit"
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
    grid-template-columns: minmax(260px, 320px) minmax(0, 1fr);
    gap: var(--space-5);
    align-items: stretch;
    min-height: min(680px, calc(100dvh - 220px));
  }

  .messages-pane {
    min-width: 0;
  }

  .messages-pane > .card,
  .messages-thread {
    height: 100%;
    display: flex;
    flex-direction: column;
  }

  .messages-pane > .card .card-body,
  .messages-thread .card-body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .messages-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  /** 空态包装：内容居中；在撑满的卡片内垂直居中。 */
  .messages-empty {
    padding: var(--space-6);
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    justify-content: center;
  }

  .messages-item {
    display: flex;
    gap: 12px;
    align-items: center;
    padding: 12px 16px;
    border-bottom: 1px solid var(--color-border);
    text-decoration: none;
    color: inherit;
    transition: background 0.15s ease;
  }

  .messages-item:hover {
    background: var(--color-bg-subtle);
  }

  .messages-item.is-active {
    background: var(--color-bg-inset);
  }

  .messages-item-avatar-wrap {
    position: relative;
    flex-shrink: 0;
  }

  .messages-item-avatar-wrap :global(.avatar) {
    border-radius: 8px !important;
  }

  .messages-unread-badge {
    position: absolute;
    top: -4px;
    right: -4px;
    min-width: 18px;
    height: 18px;
    padding: 0 4px;
    font-size: 11px;
    font-weight: 600;
    line-height: 18px;
    text-align: center;
    border-radius: 9px;
    box-shadow: 0 0 0 2px var(--color-bg-card, #fff);
  }

  .messages-item-main {
    min-width: 0;
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .messages-item-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .messages-item-name {
    font-family: var(--font-family-base);
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .messages-item-preview {
    color: var(--color-text-secondary);
    font-family: var(--font-family-base);
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    line-height: 1.4;
  }

  .messages-item-time {
    flex-shrink: 0;
    font-family: var(--font-family-mono);
    font-variant-numeric: tabular-nums;
    font-size: 11px;
    color: var(--color-text-tertiary);
  }

  .messages-list-header {
    min-height: 48px;
    padding: 8px 14px;
    display: flex;
    align-items: center;
  }

  .messages-list-title {
    font-family: var(--font-family-base) !important;
    font-size: 15px !important;
    font-weight: 600 !important;
    letter-spacing: normal !important;
    text-transform: none !important;
    color: var(--color-text-primary) !important;
  }

  .messages-thread-header {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 48px;
    padding: 8px 14px;
  }

  .messages-thread-title {
    font-family: var(--font-family-base) !important;
    font-size: 16px !important;
    font-weight: 600 !important;
    line-height: 1.3 !important;
    letter-spacing: normal !important;
    text-transform: none !important;
    color: var(--color-text-primary) !important;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .messages-back {
    display: none;
    align-items: center;
    gap: 2px;
    padding: 6px 8px !important;
    margin-left: -6px;
    font-size: 15px;
    font-weight: var(--weight-medium);
    color: var(--color-text);
    text-decoration: none;
    border-radius: var(--radius-sm);
    border: none !important;
    background: transparent !important;
    box-shadow: none !important;
    height: 36px !important;
    min-height: 36px !important;
  }

  .messages-back:hover,
  .messages-back:active {
    color: var(--color-text);
    background: var(--color-bg-subtle) !important;
  }

  .messages-body {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 16px 14px;
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    background: var(--color-bg-subtle);
  }

  .msg-row {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    max-width: 82%;
  }

  .msg-row.is-own {
    align-self: flex-end;
    flex-direction: row;
    justify-content: flex-end;
  }

  .msg-wrapper {
    position: relative;
    max-width: 100%;
    display: flex;
    flex-direction: column;
  }

  .msg-row.is-own .msg-wrapper {
    align-items: flex-end;
  }

  .msg-row:not(.is-own) .msg-wrapper {
    align-items: flex-start;
  }

  .msg-quote-card {
    margin-bottom: 6px;
    padding: 4px 8px;
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.06);
    border-left: 3px solid rgba(0, 0, 0, 0.25);
    font-size: 12px;
    line-height: 1.35;
    color: rgba(0, 0, 0, 0.7);
    word-break: break-word;
  }

  .msg-quote-card.is-own {
    background: rgba(0, 0, 0, 0.08);
    border-left-color: rgba(0, 0, 0, 0.35);
    color: rgba(0, 0, 0, 0.75);
  }

  :global(html.dark) .msg-quote-card,
  :global(html[data-theme='dark']) .msg-quote-card {
    background: rgba(255, 255, 255, 0.08);
    border-left-color: rgba(255, 255, 255, 0.35);
    color: rgba(255, 255, 255, 0.75);
  }

  :global(html.dark) .msg-quote-card.is-own,
  :global(html[data-theme='dark']) .msg-quote-card.is-own {
    background: rgba(0, 0, 0, 0.3);
    border-left-color: #95ec69;
    color: #e5e7eb;
  }

  .msg-actions {
    position: absolute;
    bottom: calc(100% + 4px);
    z-index: 10;
    display: none;
    align-items: center;
    gap: 2px;
    padding: 3px;
    border-radius: 6px;
    background: #2b2b2b;
    color: #ffffff;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.28);
    white-space: nowrap;
    animation: popIn 0.15s ease-out;
  }

  .msg-row.is-own .msg-actions {
    right: 0;
  }

  .msg-row:not(.is-own) .msg-actions {
    left: 0;
  }

  @media (hover: hover) {
    .msg-wrapper:hover .msg-actions {
      display: inline-flex;
    }
  }

  .msg-actions.is-open {
    display: inline-flex !important;
  }

  .msg-action-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: #ffffff;
    font-size: 12px;
    line-height: 1;
    cursor: pointer;
    transition: background 0.15s ease;
  }

  .msg-action-btn:hover,
  .msg-action-btn:active {
    background: rgba(255, 255, 255, 0.15);
  }

  .msg-action-btn.is-danger:hover {
    background: rgba(239, 68, 68, 0.4);
  }

  .messages-quote-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 10px;
    margin-bottom: 6px;
    border-radius: 6px;
    background: var(--color-bg-subtle);
    border-left: 3px solid var(--color-brand, #2b6c38);
    font-size: 13px;
  }

  .messages-quote-bar-content {
    min-width: 0;
    flex: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .messages-quote-bar-tag {
    font-size: 11px;
    padding: 1px 5px;
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.08);
    color: var(--color-text-secondary);
  }

  .messages-quote-bar-sender {
    font-weight: 500;
    color: var(--color-text);
  }

  .messages-quote-bar-text {
    color: var(--color-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .messages-quote-bar-close {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 2px;
    border: none;
    background: transparent;
    color: var(--color-text-tertiary);
    cursor: pointer;
    border-radius: 4px;
  }

  .messages-quote-bar-close:hover {
    color: var(--color-text);
    background: rgba(0, 0, 0, 0.06);
  }

  .chat-recalled-notice {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    font-size: 12px;
    color: var(--color-text-tertiary);
  }

  .chat-reedit-btn {
    border: none;
    background: transparent;
    color: var(--color-brand, #0969da);
    font-size: 12px;
    padding: 0;
    cursor: pointer;
    text-decoration: underline;
  }

  .chat-reedit-btn:hover {
    opacity: 0.8;
  }

  @keyframes popIn {
    from {
      opacity: 0;
      transform: scale(0.92);
    }
    to {
      opacity: 1;
      transform: scale(1);
    }
  }

  .msg-avatar {
    flex-shrink: 0;
    margin-top: 1px;
  }

  .msg-avatar :global(.avatar) {
    border-radius: 6px !important;
  }

  .msg {
    position: relative;
    max-width: 100%;
    padding: 9px 12px;
    border-radius: 6px;
    background: #ffffff;
    font-family: var(--font-family-base);
    font-size: 15px;
    white-space: pre-wrap;
    word-break: break-word;
    line-height: 1.45;
    border: 1px solid rgba(0, 0, 0, 0.08);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.03);
    color: #111827;
  }

  .msg.is-own {
    background: #95ec69;
    color: #000000;
    border: none;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
  }

  /* 微信深色模式标准气泡配色：自发生信采用清爽温润的深森林绿，收件气泡采用现代深灰 */
  :global(html.dark) .msg.is-own,
  :global(html[data-theme='dark']) .msg.is-own {
    background: #2e6a38;
    color: #f3f4f6;
    border: none;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
  }

  :global(html.dark) .msg:not(.is-own),
  :global(html[data-theme='dark']) .msg:not(.is-own) {
    background: #25292e;
    color: #e5e7eb;
    border: 1px solid rgba(255, 255, 255, 0.08);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
  }

  @media (prefers-color-scheme: dark) {
    :global(html:not(.light):not([data-theme='light'])) .msg.is-own {
      background: #2e6a38;
      color: #f3f4f6;
      border: none;
      box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
    }
    :global(html:not(.light):not([data-theme='light'])) .msg:not(.is-own) {
      background: #25292e;
      color: #e5e7eb;
      border: 1px solid rgba(255, 255, 255, 0.08);
      box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
    }
  }

  .chat-time {
    align-self: center;
    padding: 2px 8px;
    font-family: var(--font-family-mono);
    font-variant-numeric: tabular-nums;
    font-size: var(--text-xs);
    color: var(--color-text-tertiary);
    user-select: none;
    line-height: 1.4;
  }

  .messages-compose {
    flex-shrink: 0;
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    border-top: var(--border-default);
    background: var(--color-bg-card);
  }

  .messages-compose-row {
    display: flex;
    align-items: flex-end;
    gap: 8px;
    width: 100%;
  }

  .messages-compose-textarea {
    flex: 1 1 auto;
    min-height: 38px;
    max-height: 120px;
    padding: 8px 12px;
    border-radius: 6px;
    resize: none;
    line-height: 1.4;
    font-family: var(--font-family-base);
    font-size: 15px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    color: var(--color-text-primary);
  }

  :global(.messages-compose-submit) {
    flex-shrink: 0;
    height: 38px !important;
    min-height: 38px !important;
    padding: 0 16px !important;
    border-radius: 6px !important;
  }

  .messages-compose-hint {
    margin: 0 0 var(--space-2);
  }

  /* 移动端断点与 styles/mobile.css 对齐（≤767）：mobile.css 已 owning 线程视图
     （单滚动宿主 / 输入框安全区 / 16px 字号防 iOS 缩放），本组件只负责
     「会话列表空态场景」的视口拉伸，不与它竞争。断点若写成 768 会在 768px
     宽度产生两边都不管的缝隙。 */
  @media (max-width: 767px) {
    /* 撑满视口（Playwright @390×844 实测校准）：顶栏 sticky 52px 流内 +
       底部 Tab 58px fixed = 110px；可见卡片高度 = 100dvh - 110px - safe，
       卡片底正好贴 Tab 顶。页面根 padding 归零（对齐 mobile.css 的选择器与
       优先级），消除底部 46px 弹性滚动；顶栏到卡片为全出血应用式布局。 */
    :global(#main-content #page-messages.app-page) {
      /* 压过 chinese-elegance 遗留的 height: calc(100dvh - 64px)（旧 chrome 口径，
         实测多出 46px 造成页面滚动）与 display:block !important（行 1210）——
         级联中 !important 无视优先级，display 需同样 !important 才能取胜。 */
      display: flex !important;
      flex-direction: column;
      height: calc(100dvh - 110px - env(safe-area-inset-bottom, 0px)) !important;
      min-height: 0 !important;
      padding-top: 0 !important;
      padding-bottom: 0 !important;
      overflow: hidden !important;
    }

    /* 微信交互模式：进入对话后无须预留底部导航高度，全屏贴紧顶栏与底栏 */
    :global(#main-content #page-messages.app-page.in-thread) {
      height: calc(100dvh - var(--mobile-header-h, 52px)) !important;
    }

    .messages-layout {
      display: flex;
      flex-direction: column;
      flex: 1;
      min-height: 0;
    }

    .messages-pane {
      display: flex;
      flex-direction: column;
      flex: 1;
      min-height: 0;
    }

    .messages-pane.is-hidden-mobile {
      display: none !important;
    }

    .messages-pane > .card {
      flex: 1;
      display: flex;
      flex-direction: column;
    }

    .messages-pane > .card .card-body {
      flex: 1;
      display: flex;
      flex-direction: column;
    }

    .messages-back {
      display: inline-flex;
    }
  }
</style>
