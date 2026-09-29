<script lang="ts">
  // M18-ADMIN-CONTENT：内容审核页重构。
  // - 待审队列：表格管理，展示待审帖子列表，支持点击列表行/标题/「审核」按钮呼出审核弹窗；
  // - 审核弹窗重构：
  //   1) 展示发帖人完整信息（头像缩写、用户名、所属板块、提交时间、待审状态、版本标识、修订说明）；
  //   2) 突出展示帖子内容（标题与排版优美可读的正文），并支持与版本比对（Diff）一键切换；
  //   3) 下方为「通过审核」与「不通过」两大明确操作：
  //      - 「通过审核」：一键快速通过并发布，无需繁琐输入理由；
  //      - 「不通过」：点击后再展开理由输入面板，支持快捷填入与详细驳回原因输入；
  // - 兼容性：保留无 JS 锚点跳转（?post={id}#review-detail）与 SSR 快照契约。
  import { enhance } from '$app/forms';
  import { goto, invalidateAll } from '$app/navigation';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import RowActionsMenu from '$lib/components/admin/RowActionsMenu.svelte';
  import GitDiffViewer from '$lib/components/admin/GitDiffViewer.svelte';
  import SafeHtml from '$lib/components/SafeHtml.svelte';
  import { toastActionResult } from '$lib/ui/action-toast';
  import type { AdminContentActionData, AdminContentPageData, AdminContentPost } from './+page.server';

  let { data, form }: { data: AdminContentPageData; form?: AdminContentActionData | null } = $props();

  const posts = $derived(data.posts);
  const currentPost = $derived(data.current);
  const diff = $derived(data.diff);

  // 审核大弹窗状态
  let modalOpen = $state(false);
  let modalLoading = $state(false);
  let submitting = $state(false);

  // 弹窗视图模式：'content' 帖子内容（默认） vs 'diff' 版本对比
  let activeView = $state<'content' | 'diff'>('content');

  // 审核驳回面板展开状态（点击「不通过」后再输入理由）
  let rejecting = $state(false);
  let rejectReason = $state('');

  // 常用不通过快捷理由
  const REJECT_QUICK_REASONS = [
    '排版混乱，请重新整理段落与格式',
    '含有违规广告或推广内容',
    '内容不完整，请补充详细说明',
    '代码块或格式存在错误',
    '内容与当前板块定位不符',
    '包含违规或不适宜公开内容'
  ];

  /** 点击列表行/标题/「审核」按钮：打开审核大弹窗 */
  async function openReviewModal(post: AdminContentPost): Promise<void> {
    activeView = 'content';
    rejecting = false;
    rejectReason = '';
    modalOpen = true;

    if (currentPost?.id !== post.id) {
      modalLoading = true;
      try {
        await goto(`?post=${post.id}`, { replaceState: true, noScroll: true, keepFocus: true });
      } finally {
        modalLoading = false;
      }
    }
  }

  function closeReviewModal(): void {
    modalOpen = false;
    rejecting = false;
    rejectReason = '';
    submitting = false;
  }

  /** 兼容「⋮」菜单触发的独立动作 */
  function openOps(action: 'approve' | 'reject'): void {
    modalOpen = true;
    activeView = 'content';
    if (action === 'reject') {
      rejecting = true;
      rejectReason = '';
    } else {
      rejecting = false;
    }
  }

  /** 「⋮」菜单项：通过/驳回；diff 不可用时禁用（避免按错误内容审批） */
  const DIFF_UNAVAILABLE_HINT = '需要真实版本对比数据后才能审核';
  function reviewMenuActions() {
    const available = diff !== null;
    return [
      {
        label: '通过',
        disabled: !available,
        hint: available ? undefined : DIFF_UNAVAILABLE_HINT,
        run: () => openOps('approve')
      },
      {
        label: '驳回',
        danger: true,
        disabled: !available,
        hint: available ? undefined : DIFF_UNAVAILABLE_HINT,
        run: () => openOps('reject')
      }
    ];
  }

  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  // 确定性格式（UTC）：YYYY-MM-DD HH:mm
  const stampOf = (ms: number) => new Date(ms).toISOString().slice(0, 16).replace('T', ' ');

  /** 详情区类型徽章：修改（有前版）vs 首次提交（仅 v1）；diff 未知时不渲染 */
  const detailType = $derived(
    diff === null
      ? null
      : diff.from_version === null
        ? { label: '首次提交', cls: 'sbadge sb-brand' }
        : { label: '修改', cls: 'sbadge sb-hot' }
  );

  const authorInitial = $derived(
    currentPost?.author_username ? currentPost.author_username.charAt(0).toUpperCase() : 'U'
  );
</script>

<svelte:head>
  <title>内容审核 — BBLBB Admin</title>
</svelte:head>

<div style="margin-bottom:12px;">
  <a href="/admin/posts" class="text-link" style="font-size:13px;display:inline-flex;align-items:center;gap:4px;">
    返回内容列表
  </a>
</div>

{#if form?.message && !hasJs}
  <div class="alert alert-success" role="status" style="margin-bottom:14px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-md);border-left:3px solid var(--color-success);">
    {form.message}
  </div>
{/if}
{#if form?.error && !hasJs}
  <div class="alert alert-danger" role="alert" style="margin-bottom:14px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-md);border-left:3px solid var(--color-danger);">
    {form.error}
  </div>
{/if}

{#if data.state !== 'ok'}
  <!-- forbidden / error：明确提示 -->
  <div class="alert alert-danger" role="alert" style="margin-bottom:14px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-md);border-left:3px solid var(--color-danger);">
    {data.error ?? (data.state === 'forbidden' ? '没有审核权限' : '加载失败，请稍后重试')}
  </div>
{:else if posts.length === 0 || !currentPost}
  <div class="app-card">
    <div class="app-card__body">
      <EmptyState icon="file-text" title="没有待审核的内容" desc="当前没有需要对比审核的内容" />
    </div>
  </div>
{:else}
  <!-- 1) 待审队列表格：点击列表行或「审核」唤起审核管理弹窗 -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head">
      <h2>
        待审队列
        <span class="app-muted" style="font-size:12px;font-weight:400;">
          （共 {posts.length} 篇 · 点击列表项直接弹窗审核）
        </span>
      </h2>
    </header>
    <div class="app-card__body">
      <div class="app-table-wrap">
        <table class="app-table review-table" aria-label="待审队列">
          <thead>
            <tr>
              <th style="min-width:240px;">标题</th>
              <th>作者</th>
              <th>板块</th>
              <th>提交时间</th>
              <th style="width:100px;">操作</th>
            </tr>
          </thead>
          <tbody>
            {#each posts as p (p.id)}
              {@const isCurrent = p.id === currentPost.id}
              <tr
                class="review-row"
                class:is-current={isCurrent}
                aria-current={isCurrent ? 'page' : undefined}
                onclick={() => openReviewModal(p)}
              >
                <td>
                  <b>
                    <a
                      class="review-row__title"
                      href="?post={p.id}#review-detail"
                      onclick={(e) => {
                        e.preventDefault();
                        openReviewModal(p);
                      }}
                    >
                      {p.title || '（无标题）'}
                    </a>
                  </b>
                </td>
                <td><span style="font-weight:500;">{p.author_username ?? '未知作者'}</span></td>
                <td><span class="text-secondary" style="font-size:12px;">{p.board_name ?? '未分区'}</span></td>
                <td><span class="text-secondary" style="font-size:12px;">{stampOf(p.created_at)}</span></td>
                <td class="adm-acts">
                  <button
                    type="button"
                    class="btn ghost sm review-action-btn"
                    onclick={(e) => {
                      e.stopPropagation();
                      openReviewModal(p);
                    }}
                  >
                    审核
                  </button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
  </section>

  <!-- 2) 页面内详情区（支持无 JS 锚点浏览与 SSR 渲染） -->
  <section class="app-card" id="review-detail">
    <header class="app-card__head" style="padding:16px 20px;display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:10px;">
      <h2 style="margin:0;font-size:17px;font-weight:700;line-height:1.4;">
        版本对比 · {currentPost.title}
      </h2>
      <button
        type="button"
        class="btn primary sm"
        onclick={() => openReviewModal(currentPost)}
      >
        <Icon name="maximize-2" size={14} />
        <span>弹窗全屏管理</span>
      </button>
    </header>

    <div class="app-card__body" style="padding:16px 20px;">
      {#if diff}
        <!-- 发帖人信息展示栏 -->
        <div class="author-info-card" style="margin-bottom:16px;">
          <div class="author-avatar">{authorInitial}</div>
          <div class="author-meta-block">
            <div class="author-primary-row">
              <span class="author-name">{currentPost.author_username ?? '未知作者'}</span>
              <span class="author-role-tag">发帖人</span>
              {#if currentPost.board_name}
                <span class="author-board-tag">
                  <Icon name="folder" size={12} />
                  {currentPost.board_name}
                </span>
              {/if}
            </div>
            <div class="author-secondary-row">
              <span>提交时间：{stampOf(currentPost.created_at)}</span>
              <span>·</span>
              <span>状态：<span class="status-badge-pending">待审核</span></span>
              <span>·</span>
              {#if detailType}
                <span class={detailType.cls} style="padding:1px 6px;border-radius:4px;font-size:11px;">{detailType.label}</span>
              {/if}
            </div>
          </div>
        </div>

        <!-- 图例与版本说明 -->
        <div class="review-diff__legend">
          {#if detailType}
            <span class={detailType.cls} style="padding:2px 8px;border-radius:4px;font-size:11px;font-weight:var(--weight-semibold);">{detailType.label}</span>
          {/if}
          <span class="diff-chip diff-chip--added">新增</span>
          <span class="diff-chip diff-chip--removed">删除</span>
          <span class="diff-chip diff-chip--changed">变更</span>
          <span class="review-diff__versions">
            {diff.from_version === null ? '首次提交' : `v${diff.from_version} → v${diff.to_version}`}
          </span>
        </div>

        {#if diff.reason}
          <p class="review-diff__reason">修订说明：{diff.reason}</p>
        {/if}

        <!-- Git 风格差异查看器 -->
        <div style="margin-bottom:var(--space-4);">
          <GitDiffViewer
            beforeBody={diff.before_body}
            afterBody={diff.after_body}
            fromVersion={diff.from_version}
            toVersion={diff.to_version}
            reason={diff.reason}
            defaultMode="split"
          />
        </div>

        <!-- 兼容 SSR 测试的文本对比语义块 -->
        <div class="review-diff__grid review-diff__grid--ssr" aria-label="SSR快照对比">
          <section class="review-diff__pane review-diff__pane--before">
            <h3>修改前{diff.from_version === null ? '（首次提交，无先前版本）' : ` · v${diff.from_version}`}</h3>
            {#if diff.before_body !== null}
              <pre class="review-diff__text">{diff.before_body}</pre>
            {:else}
              <p class="review-diff__empty">—（无先前版本，全部内容为新增）</p>
            {/if}
          </section>
          <section class="review-diff__pane review-diff__pane--after">
            <h3>修改后 · v{diff.to_version}</h3>
            <pre class="review-diff__text">{diff.after_body}</pre>
          </section>
        </div>
      {:else}
        <!-- 对比数据不可用：禁用审核操作 -->
        <div class="review-data-warning" role="alert">
          <strong>当前无法进行版本对比</strong>
          <span>
            {data.diff_error ?? '没有可用的修订快照数据。为避免按错误内容审批，审核操作暂时禁用。'}
          </span>
        </div>
      {/if}

      <!-- 审核操作菜单 -->
      <div style="display:flex;gap:10px;align-items:center;flex-wrap:wrap;margin-top:14px;">
        <RowActionsMenu label="更多操作：待审帖 {currentPost.title}" actions={reviewMenuActions()} />
        {#if !diff}
          <span class="review-entry--disabled" title="需要真实版本对比数据后才能审核">审核暂不可用</span>
        {/if}
      </div>
    </div>
  </section>

  <!-- 3) 审核管理大弹窗（重构：显示发帖人信息、帖子正文、下方通过/不通过并输入理由） -->
  <Dialog
    open={modalOpen && !!currentPost}
    size="xl"
    title={`审核管理 · ${currentPost.title}`}
    description={`作者：${currentPost.author_username ?? '未知作者'} · 板块：${currentPost.board_name ?? '未分区'} · 提交时间：${stampOf(currentPost.created_at)}`}
    onclose={closeReviewModal}
  >
    {#if modalLoading}
      <div style="padding:40px;text-align:center;color:var(--color-text-secondary);">
        <Icon name="loader" size={24} />
        <p style="margin-top:8px;">正在加载版本与帖子内容...</p>
      </div>
    {:else if diff}
      <div class="audit-modal-body">
        <!-- A. 发帖人信息卡片 -->
        <div class="author-info-card">
          <div class="author-avatar">{authorInitial}</div>
          <div class="author-meta-block">
            <div class="author-primary-row">
              <span class="author-name">{currentPost.author_username ?? '未知作者'}</span>
              <span class="author-role-tag">发帖人</span>
              {#if currentPost.board_name}
                <span class="author-board-tag">
                  <Icon name="folder" size={12} />
                  {currentPost.board_name}
                </span>
              {/if}
              <span class="status-badge-pending">待审核</span>
            </div>
            <div class="author-secondary-row">
              <span><Icon name="clock" size={12} /> 提交时间：{stampOf(currentPost.created_at)}</span>
              <span>·</span>
              <span>
                {#if diff.from_version === null}
                  <span class="sbadge sb-brand">首次提交 · v{diff.to_version}</span>
                {:else}
                  <span class="sbadge sb-hot">修订版本 · v{diff.from_version} → v{diff.to_version}</span>
                {/if}
              </span>
            </div>
          </div>
          {#if diff.reason}
            <div class="author-revision-callout">
              <span class="callout-label">修订说明：</span>
              <span class="callout-value">{diff.reason}</span>
            </div>
          {/if}
        </div>

        <!-- B. 帖子内容与版本比对区域 -->
        <div class="post-content-container">
          <div class="post-content-header">
            <div class="post-header-left">
              <h3 class="post-title-display">{currentPost.title}</h3>
            </div>
            {#if diff.from_version !== null}
              <div class="view-switcher" role="tablist">
                <button
                  type="button"
                  role="tab"
                  class="view-tab-btn"
                  class:is-active={activeView === 'content'}
                  aria-selected={activeView === 'content'}
                  onclick={() => (activeView = 'content')}
                >
                  <Icon name="file-text" size={13} />
                  <span>帖子内容</span>
                </button>
                <button
                  type="button"
                  role="tab"
                  class="view-tab-btn"
                  class:is-active={activeView === 'diff'}
                  aria-selected={activeView === 'diff'}
                  onclick={() => (activeView = 'diff')}
                >
                  <Icon name="git-commit" size={13} />
                  <span>版本比对 (Diff)</span>
                </button>
              </div>
            {/if}
          </div>

          <!-- 内容展示 -->
          {#if activeView === 'content' || diff.from_version === null}
            <div class="post-reading-view">
              {#if diff.after_html}
                <SafeHtml html={diff.after_html} />
              {:else if diff.after_body}
                <div class="post-plain-body">{diff.after_body}</div>
              {:else}
                <div class="post-empty-body">（无正文内容）</div>
              {/if}
            </div>
          {:else}
            <div class="post-diff-wrapper">
              <GitDiffViewer
                beforeBody={diff.before_body}
                afterBody={diff.after_body}
                fromVersion={diff.from_version}
                toVersion={diff.to_version}
                reason={diff.reason}
                defaultMode="split"
              />
            </div>
          {/if}
        </div>

        <!-- C. 下方审核操作区：通过审核 vs 不通过（点击输入理由） -->
        <div class="audit-bottom-bar">
          {#if !rejecting}
            <!-- 初始状态：通过审核 / 不通过 两大主按钮 -->
            <div class="audit-actions-row">
              <div class="audit-hint">
                <Icon name="shield-check" size={16} />
                <span>请对该内容进行审核处置：</span>
              </div>
              <div class="audit-buttons-group">
                <!-- 通过审核：点击一键通过 -->
                <form
                  method="POST"
                  action="?/approve"
                  use:enhance={() => {
                    submitting = true;
                    return async ({ result, update }) => {
                      submitting = false;
                      toastActionResult(result, { message: (d) => (d?.message ?? d?.error) as string | null });
                      await update();
                      await invalidateAll();
                      if (result.type === 'success') {
                        closeReviewModal();
                      }
                    };
                  }}
                  style="display:inline-flex;"
                >
                  <input type="hidden" name="id" value={currentPost.id} />
                  <input type="hidden" name="reason" value="内容合规，同意发布" />
                  <button
                    type="submit"
                    class="audit-op-btn audit-op-btn--approve"
                    disabled={submitting}
                  >
                    <Icon name="check-circle" size={16} />
                    <span>{submitting ? '提交中...' : '通过审核'}</span>
                  </button>
                </form>

                <!-- 不通过：点击后展开理由输入 -->
                <button
                  type="button"
                  class="audit-op-btn audit-op-btn--reject review-btn--reject"
                  disabled={submitting}
                  onclick={() => {
                    rejecting = true;
                    rejectReason = '';
                  }}
                >
                  <Icon name="x-circle" size={16} />
                  <span>不通过</span>
                </button>
              </div>
            </div>
          {:else}
            <!-- 不通过状态：输入理由面板 -->
            <div class="reject-panel">
              <div class="reject-panel-header">
                <div class="reject-header-title">
                  <Icon name="alert-triangle" size={15} />
                  <label id="content-modal-reason-label" for="content-modal-reason">请填写不通过理由（作者可见，将写入审计日志）</label>
                </div>
                <button
                  type="button"
                  class="btn ghost sm"
                  onclick={() => {
                    rejecting = false;
                    rejectReason = '';
                  }}
                >
                  返回
                </button>
              </div>

              <!-- 快捷理由标签 -->
              <div class="quick-reasons-bar">
                <span class="quick-reasons-label">快捷填入：</span>
                <div class="quick-reasons-list">
                  {#each REJECT_QUICK_REASONS as qr}
                    <button
                      type="button"
                      class="quick-reason-chip"
                      class:is-active={rejectReason === qr}
                      onclick={() => (rejectReason = qr)}
                    >
                      {qr}
                    </button>
                  {/each}
                </div>
              </div>

              <!-- 驳回提交表单 -->
              <form
                method="POST"
                action="?/reject"
                use:enhance={() => {
                  submitting = true;
                  return async ({ result, update }) => {
                    submitting = false;
                    toastActionResult(result, { message: (d) => (d?.message ?? d?.error) as string | null });
                    await update();
                    await invalidateAll();
                    if (result.type === 'success') {
                      closeReviewModal();
                    }
                  };
                }}
                class="reject-form"
              >
                <input type="hidden" name="id" value={currentPost.id} />
                <textarea
                  id="content-modal-reason"
                  name="reason"
                  bind:value={rejectReason}
                  class="reject-textarea"
                  placeholder="请详细说明不通过的具体原因，例如：排版格式混乱，请调整后再试..."
                  rows={3}
                  required
                ></textarea>

                <div class="reject-form-actions">
                  <button
                    type="button"
                    class="btn ghost sm"
                    onclick={() => {
                      rejecting = false;
                      rejectReason = '';
                    }}
                  >
                    取消
                  </button>
                  <Button
                    text={submitting ? '提交中...' : '确认不通过并驳回'}
                    variant="danger"
                    size="sm"
                    type="submit"
                    disabled={submitting || !rejectReason.trim()}
                  />
                </div>
              </form>
            </div>
          {/if}
        </div>
      </div>
    {:else}
      <div class="review-data-warning" role="alert" style="margin:20px 0;">
        <strong>当前无法进行版本对比</strong>
        <span>
          {data.diff_error ?? '没有可用的修订快照数据。为避免按错误内容审批，审核操作暂时禁用。'}
        </span>
      </div>
      <div style="display:flex;justify-content:flex-end;">
        <button type="button" class="btn ghost sm" onclick={closeReviewModal}>关闭</button>
      </div>
    {/if}
  </Dialog>
{/if}

<style>
  /* ── 表格与列表行交互 ── */
  .review-row {
    cursor: pointer;
    transition: background-color var(--duration-fast) ease;
  }

  .review-row:hover > td {
    background-color: var(--color-bg-subtle);
  }

  .review-table tr.is-current > td {
    background: var(--color-bg-subtle);
  }

  .review-table tr.is-current > td:first-child {
    box-shadow: inset 3px 0 0 var(--color-brand);
  }

  .review-row__title {
    color: inherit;
    text-decoration: none;
  }

  .review-row__title:hover {
    color: var(--color-brand);
    text-decoration: underline;
  }

  .review-action-btn {
    font-weight: 500;
  }

  /* ── 发帖人信息卡片 ── */
  .author-info-card {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 16px;
    background: var(--color-bg-subtle);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    flex-wrap: wrap;
  }

  .author-avatar {
    width: 38px;
    height: 38px;
    border-radius: 50%;
    background: linear-gradient(135deg, var(--color-brand) 0%, #6366f1 100%);
    color: #ffffff;
    font-weight: 700;
    font-size: 15px;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.15);
  }

  .author-meta-block {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
    min-width: 200px;
  }

  .author-primary-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .author-name {
    font-size: 14px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .author-role-tag {
    font-size: 11px;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    color: var(--color-text-secondary);
    font-weight: 500;
  }

  .author-board-tag {
    font-size: 11px;
    padding: 1px 8px;
    border-radius: 12px;
    background: rgba(99, 102, 241, 0.12);
    color: #6366f1;
    font-weight: 500;
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .status-badge-pending {
    font-size: 11px;
    padding: 1px 7px;
    border-radius: 4px;
    background: var(--color-warning-soft);
    color: var(--color-warning);
    font-weight: 600;
  }

  .author-secondary-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--color-text-secondary);
    flex-wrap: wrap;
  }

  .author-secondary-row span {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .author-revision-callout {
    font-size: 12px;
    padding: 6px 12px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-secondary);
    max-width: 100%;
  }

  .callout-label {
    font-weight: 600;
    color: var(--color-text-tertiary);
  }

  /* ── 帖子正文与内容展示区域 ── */
  .post-content-container {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg-card);
    overflow: hidden;
  }

  .post-content-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 18px;
    background: var(--color-bg-subtle);
    border-bottom: 1px solid var(--color-border);
    flex-wrap: wrap;
    gap: 10px;
  }

  .post-title-display {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .view-switcher {
    display: inline-flex;
    gap: 4px;
    background: var(--color-bg-page);
    padding: 2px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--color-border);
  }

  .view-tab-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    font-size: 12px;
    border: none;
    background: transparent;
    color: var(--color-text-secondary);
    border-radius: calc(var(--radius-sm) - 2px);
    cursor: pointer;
    font-weight: 500;
    transition: all var(--duration-fast) ease;
  }

  .view-tab-btn:hover {
    color: var(--color-text-primary);
  }

  .view-tab-btn.is-active {
    background: var(--color-bg-card);
    color: var(--color-brand);
    font-weight: 600;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
  }

  .post-reading-view {
    padding: 20px 24px;
    max-height: 440px;
    overflow-y: auto;
    font-size: 14px;
    line-height: 1.75;
    color: var(--color-text-primary);
    background: var(--color-bg-card);
  }

  .post-plain-body {
    white-space: pre-wrap;
    word-break: break-word;
    font-family: inherit;
    line-height: 1.8;
  }

  .post-empty-body {
    text-align: center;
    color: var(--color-text-tertiary);
    font-style: italic;
    padding: 32px 0;
  }

  .post-diff-wrapper {
    max-height: 480px;
  }

  /* ── 底部操作区 ── */
  .audit-bottom-bar {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg-subtle);
    padding: 16px 20px;
  }

  .audit-actions-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 14px;
  }

  .audit-hint {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-secondary);
  }

  .audit-buttons-group {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .audit-op-btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 8px 18px;
    font-size: 13px;
    font-weight: 600;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: all var(--duration-fast) ease;
    border: 1px solid transparent;
  }

  .audit-op-btn--approve {
    background: var(--color-success);
    color: #ffffff;
    border-color: var(--color-success);
    box-shadow: 0 2px 4px rgba(16, 185, 129, 0.2);
  }

  .audit-op-btn--approve:hover:not(:disabled) {
    background: #059669;
    border-color: #059669;
  }

  .audit-op-btn--reject {
    background: var(--color-bg-card);
    color: var(--color-danger);
    border-color: var(--color-danger);
  }

  .audit-op-btn--reject:hover:not(:disabled) {
    background: var(--color-danger-soft);
  }

  .audit-op-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  /* ── 驳回理由面板 ── */
  .reject-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .reject-panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 8px;
  }

  .reject-header-title {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    font-weight: 600;
    color: var(--color-danger);
  }

  .quick-reasons-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .quick-reasons-label {
    font-size: 12px;
    color: var(--color-text-tertiary);
    font-weight: 500;
  }

  .quick-reasons-list {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .quick-reason-chip {
    padding: 3px 10px;
    border-radius: 12px;
    border: 1px solid var(--color-border);
    background: var(--color-bg-card);
    color: var(--color-text-secondary);
    font-size: 11px;
    cursor: pointer;
    transition: all var(--duration-fast) ease;
  }

  .quick-reason-chip:hover {
    background: var(--color-bg-page);
    border-color: var(--color-danger);
    color: var(--color-danger);
  }

  .quick-reason-chip.is-active {
    background: var(--color-danger-soft);
    border-color: var(--color-danger);
    color: var(--color-danger);
    font-weight: 600;
  }

  .reject-form {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .reject-textarea {
    width: 100%;
    padding: 10px 12px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-bg-card);
    color: var(--color-text-primary);
    font-size: 13px;
    font-family: inherit;
    line-height: 1.5;
    resize: vertical;
    box-sizing: border-box;
    transition: border-color var(--duration-fast) ease;
  }

  .reject-textarea:focus {
    outline: none;
    border-color: var(--color-danger);
    box-shadow: 0 0 0 2px rgba(239, 68, 68, 0.15);
  }

  .reject-form-actions {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 10px;
  }

  /* ── 弹窗容器 ── */
  .audit-modal-body {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  /* ── 原生 diff 样式与 SSR 回退兼容 ── */
  .review-diff__legend {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    margin-bottom: var(--space-3);
  }

  .diff-chip {
    padding: 2px 8px;
    border-radius: 4px;
    font-size: 11px;
    font-weight: var(--weight-semibold);
  }

  .diff-chip--added {
    background: var(--color-success-soft);
    color: var(--color-success);
  }

  .diff-chip--removed {
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }

  .diff-chip--changed {
    background: var(--color-warning-soft);
    color: var(--color-warning);
  }

  .review-diff__versions {
    color: var(--color-text-tertiary);
    font: 600 var(--text-xs)/1.4 var(--font-family-mono);
  }

  .review-diff__grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-4);
    margin-bottom: var(--space-4);
  }

  @media (max-width: 767px) {
    .review-diff__grid {
      grid-template-columns: 1fr;
    }
  }

  .review-diff__pane {
    min-width: 0;
    padding: var(--space-3) var(--space-4);
    background: var(--color-bg-subtle);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
  }

  .review-diff__pane h3 {
    margin: 0 0 var(--space-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
  }

  .review-diff__pane--before h3 {
    color: var(--color-danger);
  }

  .review-diff__pane--after h3 {
    color: var(--color-success);
  }

  .review-diff__text {
    max-height: 240px;
    margin: 0;
    overflow: auto;
    color: var(--color-text-primary);
    font-family: var(--font-family-mono);
    font-size: var(--text-sm);
    line-height: 1.6;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .review-diff__empty {
    margin: 0;
    color: var(--color-text-tertiary);
    font-size: var(--text-sm);
  }

  .review-diff__reason {
    margin: 0 0 var(--space-3);
    color: var(--color-text-secondary);
    font-size: var(--text-sm);
  }

  .review-entry--disabled {
    display: inline-flex;
    align-items: center;
    padding: 0 14px;
    height: var(--aui-control-height, 36px);
    color: var(--color-text-tertiary);
    border: 1px dashed var(--color-border-strong);
    border-radius: var(--aui-radius, var(--radius-sm));
    font-size: var(--text-sm);
    cursor: not-allowed;
  }

  .review-data-warning {
    display: grid;
    gap: var(--space-1);
    margin-bottom: var(--space-4);
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--color-warning);
    background: var(--color-warning-soft);
    color: var(--color-text-primary);
    border-radius: var(--radius-md);
  }

  .review-data-warning span {
    color: var(--color-text-secondary);
    font-size: var(--text-sm);
    line-height: 1.6;
  }
</style>
