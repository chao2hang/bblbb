<script lang="ts">
  // M18-ADMIN-CONTENT：内容审核页（对齐原型 #admin-article-audit 与 Git 风格对比体验）。
  // - 待审队列：表格管理，支持点击列表行/标题/「审核」按钮直接唤起管理弹窗（Dialog）；
  // - Git 风格比对：弹窗内及详情区参照 Git diff 展现（分栏 Split / 统一 Unified、
  //   行号、增减统计 +N -M、增删小方块进度条、行内单词/字符级高亮、版本号）；
  // - 弹窗内一站式审核：动作切换（通过/驳回）+ 快捷理由一键填入 + 表单异步提交与 Toast；
  // - 兼容性：保留无 JS 锚点跳转（?post={id}#review-detail）与 SSR 快照契约。
  import { enhance } from '$app/forms';
  import { goto, invalidateAll } from '$app/navigation';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import RowActionsMenu from '$lib/components/admin/RowActionsMenu.svelte';
  import GitDiffViewer from '$lib/components/admin/GitDiffViewer.svelte';
  import { toastActionResult } from '$lib/ui/action-toast';
  import type { AdminContentActionData, AdminContentPageData, AdminContentPost } from './+page.server';

  let { data, form }: { data: AdminContentPageData; form?: AdminContentActionData | null } = $props();

  const posts = $derived(data.posts);
  const currentPost = $derived(data.current);
  const diff = $derived(data.diff);

  // 审核大弹窗（点击列表直接弹窗进行管理）
  let modalOpen = $state(false);
  let modalLoading = $state(false);
  let submitting = $state(false);

  // 审核动作与理由
  let opsAction = $state<'approve' | 'reject'>('approve');
  let opsReason = $state('');

  // 快捷理由配置
  const APPROVE_QUICK_REASONS = [
    '内容合规，同意发布',
    '排版规范，质量良好',
    '修改符合规范，准予发布'
  ];

  const REJECT_QUICK_REASONS = [
    '排版混乱，请重新整理段落与格式',
    '含有违规广告或推广内容',
    '内容不完整，请补充详细说明',
    '代码块或格式存在错误',
    '内容与当前板块定位不符'
  ];

  const quickReasons = $derived(
    opsAction === 'approve' ? APPROVE_QUICK_REASONS : REJECT_QUICK_REASONS
  );

  function selectAction(action: 'approve' | 'reject'): void {
    opsAction = action;
    opsReason = action === 'approve' ? '内容合规，同意发布' : '';
  }

  /** 点击列表行/标题/「审核」按钮：打开审核大弹窗 */
  async function openReviewModal(post: AdminContentPost): Promise<void> {
    selectAction('approve');
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
    submitting = false;
  }

  /** 兼容既有「⋮」菜单触发的独立动作 */
  function openOps(action: 'approve' | 'reject'): void {
    selectAction(action);
    modalOpen = true;
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
  <!-- 1) 待审队列表格：点击列表行或「审核」唤起 Git diff 审核管理弹窗 -->
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

  <!-- 3) Git 风格沉浸式审核管理弹窗（核心新特性） -->
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
        <p style="margin-top:8px;">正在加载版本对比数据...</p>
      </div>
    {:else if diff}
      <!-- 弹窗主体：Git Diff 比对视图 -->
      <div class="audit-modal-body">
        <div class="audit-modal-diff">
          <GitDiffViewer
            beforeBody={diff.before_body}
            afterBody={diff.after_body}
            fromVersion={diff.from_version}
            toVersion={diff.to_version}
            reason={diff.reason}
            defaultMode="split"
          />
        </div>

        <!-- 弹窗内一站式审核管理操作区 -->
        <div class="audit-modal-panel">
          <div class="audit-panel-header">
            <span class="audit-panel-title">
              <Icon name="shield-check" size={16} />
              审核处置管理
            </span>
            <div class="audit-action-tabs" role="radiogroup" aria-label="审核决定">
              <button
                type="button"
                class="audit-tab-btn audit-tab-btn--approve"
                class:is-active={opsAction === 'approve'}
                onclick={() => selectAction('approve')}
              >
                <Icon name="check-circle" size={14} />
                <span>通过审核（公开发布）</span>
              </button>
              <button
                type="button"
                class="audit-tab-btn audit-tab-btn--reject"
                class:is-active={opsAction === 'reject'}
                onclick={() => selectAction('reject')}
              >
                <Icon name="x-circle" size={14} />
                <span>驳回修改（退回草稿）</span>
              </button>
            </div>
          </div>

          <!-- 常用快捷理由 -->
          <div class="audit-quick-bar">
            <span class="audit-quick-title">快捷填入：</span>
            <div class="audit-quick-chips">
              {#each quickReasons as qr}
                <button
                  type="button"
                  class="audit-quick-chip"
                  onclick={() => (opsReason = qr)}
                >
                  {qr}
                </button>
              {/each}
            </div>
          </div>

          <!-- 提交表单 -->
          <form
            method="POST"
            action={opsAction === 'reject' ? '?/reject' : '?/approve'}
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
            class="audit-form"
          >
            <input type="hidden" name="id" value={currentPost.id} />

            <div class="input-wrapper" style="margin-bottom:0;">
              <label class="input-label" for="content-modal-reason">
                {opsAction === 'reject' ? '驳回理由（必填，作者可见并写入审计日志）' : '通过说明（必填，写入审计日志）'}
              </label>
              <input
                id="content-modal-reason"
                type="text"
                name="reason"
                bind:value={opsReason}
                class="input-field"
                placeholder={opsAction === 'reject' ? '请填写明确的驳回理由，例如：排版格式混乱，请调整后再试' : '例如：内容合规，同意发布'}
                required
              />
            </div>

            <div class="audit-form-actions">
              <button type="button" class="btn ghost sm" onclick={closeReviewModal}>
                取消
              </button>
              <Button
                text={submitting ? '提交中...' : opsAction === 'reject' ? '确认驳回并退回' : '确认通过并发布'}
                variant={opsAction === 'reject' ? 'danger' : 'primary'}
                size="sm"
                type="submit"
                disabled={submitting}
                formaction={opsAction === 'reject' ? '?/reject' : '?/approve'}
              />
            </div>
          </form>
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
  /* 表格行交互提升 */
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

  /* diff 图例 */
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

  /* 修改前/后对比块（SSR 兼容回退） */
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

  /* 审核入口禁用态占位 */
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

  /* 对比不可用告警 */
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

  /* ── 弹窗内审核管理面板样式 ── */
  .audit-modal-body {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .audit-modal-diff {
    max-height: 520px;
  }

  .audit-modal-panel {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg-subtle);
    padding: var(--space-3) var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .audit-panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .audit-panel-title {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-weight: 600;
    font-size: 13px;
    color: var(--color-text-primary);
  }

  .audit-action-tabs {
    display: inline-flex;
    gap: 6px;
  }

  .audit-tab-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--color-border);
    background: var(--color-bg-card);
    color: var(--color-text-secondary);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: all var(--duration-fast) ease;
  }

  .audit-tab-btn:hover {
    border-color: var(--color-border-strong);
    color: var(--color-text-primary);
  }

  .audit-tab-btn--approve.is-active {
    background: var(--color-success-soft);
    color: var(--color-success);
    border-color: var(--color-success);
    font-weight: 600;
  }

  .audit-tab-btn--reject.is-active {
    background: var(--color-danger-soft);
    color: var(--color-danger);
    border-color: var(--color-danger);
    font-weight: 600;
  }

  .audit-quick-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    font-size: 11px;
  }

  .audit-quick-title {
    color: var(--color-text-tertiary);
    font-weight: 500;
  }

  .audit-quick-chips {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .audit-quick-chip {
    padding: 2px 8px;
    border-radius: 12px;
    border: 1px solid var(--color-border);
    background: var(--color-bg-card);
    color: var(--color-text-secondary);
    font-size: 11px;
    cursor: pointer;
    transition: all var(--duration-fast) ease;
  }

  .audit-quick-chip:hover {
    background: var(--color-bg-subtle);
    border-color: var(--color-brand);
    color: var(--color-brand);
  }

  .audit-form {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .audit-form-actions {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 8px;
    margin-top: 2px;
  }
</style>
