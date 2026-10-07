<script lang="ts">
  // 审核治理中心（Moderation Center）
  // 聚合「待发内容审核」与「用户举报案件」于同一个工作台。
  import { enhance } from '$app/forms';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import GitDiffViewer from '$lib/components/admin/GitDiffViewer.svelte';
  import TablePagination from '$lib/components/admin/TablePagination.svelte';
  import BatchBar from '$lib/components/admin/BatchBar.svelte';
  import DangerConfirm from '$lib/components/ui/DangerConfirm.svelte';
  import SafeHtml from '$lib/components/SafeHtml.svelte';
  import { formatRelative } from '$lib/utils';
  import { toastActionResult } from '$lib/ui/action-toast';
  import type { ModerationCenterPageData } from './+page.server';
  import type { AdminContentPost } from '../content/+page.server';
  import type { ModerationCaseItem } from '$lib/api/types';

  let { data, form }: { data: ModerationCenterPageData; form?: any } = $props();

  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  // 顶层 Tab 状态：'content' 待发内容审核 | 'cases' 用户举报案件
  let selectedTab = $state<'content' | 'cases' | null>(null);
  const currentTab = $derived(selectedTab ?? data.activeTab ?? 'content');

  function switchTab(tab: 'content' | 'cases') {
    selectedTab = tab;
    try {
      const url = new URL(window.location.href);
      url.searchParams.set('tab', tab);
      void goto(url.toString(), { replaceState: true, noScroll: true, keepFocus: true }).catch(() => {});
    } catch {
      // 兼容非浏览器环境
    }
  }

  // ── 内容审核状态与逻辑 ──
  const posts = $derived(data.content.posts ?? []);
  const currentPost = $derived(data.content.current);
  const diff = $derived(data.content.diff);

  let contentPage = $state(1);
  let contentPageSize = $state(10);
  const pagedPosts = $derived(
    posts.slice((contentPage - 1) * contentPageSize, contentPage * contentPageSize)
  );

  let reviewModalOpen = $state(false);
  let modalLoading = $state(false);
  let reviewSubmitting = $state(false);
  let activeView = $state<'content' | 'diff'>('content');
  let rejecting = $state(false);
  let rejectReason = $state('');

  const REJECT_QUICK_REASONS = [
    '排版混乱，请重新整理段落与格式',
    '含有违规广告或推广内容',
    '内容不完整，请补充详细说明',
    '代码块或格式存在错误',
    '内容与当前板块定位不符',
    '包含违规或不适宜公开内容'
  ];

  async function openReviewModal(post: AdminContentPost, initialView: 'content' | 'diff' = 'content'): Promise<void> {
    activeView = initialView;
    rejecting = false;
    rejectReason = '';
    reviewModalOpen = true;

    if (currentPost?.id !== post.id) {
      modalLoading = true;
      try {
        const url = new URL(window.location.href);
        url.searchParams.set('post', post.id);
        url.searchParams.set('tab', 'content');
        await goto(url.toString(), { replaceState: true, noScroll: true, keepFocus: true }).catch(() => {});
      } finally {
        modalLoading = false;
      }
    }
  }

  function closeReviewModal(): void {
    reviewModalOpen = false;
    rejecting = false;
    rejectReason = '';
    reviewSubmitting = false;
  }

  const stampOf = (ms: number) => new Date(ms).toISOString().slice(0, 16).replace('T', ' ');

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

  // ── 举报案件状态与逻辑 ──
  const allCases = $derived(data.cases.items ?? []);
  function getCasesStatusParam(): string {
    try {
      return page.url.searchParams.get('status') ?? '';
    } catch {
      return '';
    }
  }
  const currentCaseStatus = $derived(getCasesStatusParam());

  const caseCounts = $derived({
    all: allCases.length,
    open: allCases.filter((c) => c.status === 'open').length,
    triaged: allCases.filter((c) => c.status === 'triaged' || c.status === 'investigating').length,
    resolved: allCases.filter((c) => c.status === 'resolved').length,
    rejected: allCases.filter((c) => c.status === 'rejected').length
  });

  const caseStatusTabs = $derived([
    { key: '', label: '全部', count: caseCounts.all },
    { key: 'open', label: '待处理', count: caseCounts.open },
    { key: 'triaged', label: '处理中', count: caseCounts.triaged },
    { key: 'resolved', label: '已处理', count: caseCounts.resolved },
    { key: 'rejected', label: '已驳回', count: caseCounts.rejected }
  ]);

  const displayedCases = $derived.by(() => {
    if (!currentCaseStatus) return allCases;
    if (currentCaseStatus === 'triaged') {
      return allCases.filter((c) => c.status === 'triaged' || c.status === 'investigating');
    }
    return allCases.filter((c) => c.status === currentCaseStatus);
  });

  let casesPage = $state(1);
  let casesPageSize = $state(10);
  $effect(() => {
    void currentCaseStatus;
    casesPage = 1;
  });

  const pagedCases = $derived(
    displayedCases.slice((casesPage - 1) * casesPageSize, casesPage * casesPageSize)
  );

  let selectedCaseIds = $state<string[]>([]);
  let allCasesSelected = $derived(
    pagedCases.length > 0 && pagedCases.every((i) => selectedCaseIds.includes(i.id))
  );

  function toggleAllCases() {
    if (allCasesSelected) {
      const pagedSet = new Set(pagedCases.map((i) => i.id));
      selectedCaseIds = selectedCaseIds.filter((id) => !pagedSet.has(id));
    } else {
      const set = new Set([...selectedCaseIds, ...pagedCases.map((i) => i.id)]);
      selectedCaseIds = Array.from(set);
    }
  }

  function toggleCaseRow(id: string) {
    if (selectedCaseIds.includes(id)) {
      selectedCaseIds = selectedCaseIds.filter((x) => x !== id);
    } else {
      selectedCaseIds = [...selectedCaseIds, id];
    }
  }

  function handleCaseRowClick(e: MouseEvent, id: string) {
    const target = e.target as HTMLElement | null;
    if (target?.closest('input, button, a, label')) return;
    goto(`/admin/moderation/cases/${id}`);
  }

  function clearSelection(): void {
    selectedCaseIds = [];
  }

  let isSubmitting = $state(false);
  let batchStatusOpen = $state(false);
  let batchStatus = $state('triaged');
  let batchCloseOpen = $state(false);
  let batchCloseReason = $state('');
  let batchCloseError = $state('');
  let batchCloseForm: HTMLFormElement | undefined = $state();

  let batchRejectOpen = $state(false);
  let batchRejectReason = $state('');
  let batchRejectError = $state('');
  let batchRejectForm: HTMLFormElement | undefined = $state();

  function priorityBadge(p: string): { label: string; cls: string } {
    switch (p) {
      case 'urgent':
      case 'high':
        return { label: '高优先级', cls: 'sbadge sb-hot' };
      case 'normal':
      case 'medium':
        return { label: '中优先级', cls: 'sbadge sb-brand' };
      default:
        return { label: '低优先级', cls: 'sbadge sb-gray' };
    }
  }

  function caseStatusBadge(s: string): { label: string; cls: string } {
    switch (s) {
      case 'open':
        return { label: '待处理', cls: 'sbadge sb-hot' };
      case 'triaged':
      case 'investigating':
        return { label: '处理中', cls: 'sbadge sb-brand' };
      case 'resolved':
        return { label: '已处理', cls: 'sbadge sb-success' };
      case 'rejected':
        return { label: '已驳回', cls: 'sbadge sb-gray' };
      default:
        return { label: s, cls: 'sbadge sb-gray' };
    }
  }
</script>

<svelte:head>
  <title>审核中心 — BBLBB Admin</title>
</svelte:head>

<div class="moderation-header">
  <div class="moderation-header__top">
    <div>
      <h1 class="moderation-title">
        <span style="color:var(--color-primary);display:inline-flex;align-items:center;">
          <Icon name="shield-check" size={24} />
        </span>
        审核治理中心
      </h1>
      <p class="app-muted" style="font-size:13px;margin-top:4px;">
        一站式管理发帖审核与社区违规举报案件
      </p>
    </div>
    <div class="moderation-metrics">
      <div class="metric-chip {data.stats.totalPending > 0 ? 'metric-chip--warn' : ''}">
        <span class="metric-chip__label">待办总计</span>
        <span class="metric-chip__val">{data.stats.totalPending}</span>
      </div>
      <div class="metric-chip">
        <span class="metric-chip__label">待审内容</span>
        <span class="metric-chip__val">{data.stats.pendingContentCount}</span>
      </div>
      <div class="metric-chip">
        <span class="metric-chip__label">待处案件</span>
        <span class="metric-chip__val">{data.stats.openCasesCount}</span>
      </div>
    </div>
  </div>

  <!-- 顶层 Tab 导航条 -->
  <div class="moderation-main-tabs" role="tablist" aria-label="审核工作台分类">
    <button
      type="button"
      role="tab"
      aria-selected={currentTab === 'content'}
      class="main-tab {currentTab === 'content' ? 'is-active' : ''}"
      onclick={() => switchTab('content')}
    >
      <Icon name="file-text" size={16} />
      <span>待发内容审核</span>
      {#if data.stats.pendingContentCount > 0}
        <span class="tab-badge tab-badge--hot">{data.stats.pendingContentCount}</span>
      {/if}
    </button>
    <button
      type="button"
      role="tab"
      aria-selected={currentTab === 'cases'}
      class="main-tab {currentTab === 'cases' ? 'is-active' : ''}"
      onclick={() => switchTab('cases')}
    >
      <Icon name="flag" size={16} />
      <span>用户举报案件</span>
      {#if data.stats.openCasesCount > 0}
        <span class="tab-badge tab-badge--brand">{data.stats.openCasesCount}</span>
      {/if}
    </button>
  </div>
</div>

{#if form?.message && !hasJs}
  <div class="alert alert-success" role="status" style="margin-bottom:14px;">
    {form.message}
  </div>
{/if}
{#if form?.error && !hasJs}
  <div class="alert alert-danger" role="alert" style="margin-bottom:14px;">
    {form.error}
  </div>
{/if}

<!-- ═════════════════════ TAB 1: 待发内容审核 ═════════════════════ -->
{#if currentTab === 'content'}
  {#if data.content.state !== 'ok'}
    <div class="alert alert-danger" role="alert" style="margin-bottom:14px;">
      {data.content.error ?? (data.content.state === 'forbidden' ? '没有审核权限' : '加载失败')}
    </div>
  {:else if posts.length === 0}
    <div class="app-card">
      <div class="app-card__body">
        <EmptyState icon="check-circle" title="全部内容已审核完毕" desc="当前待审队列为空，暂无需要人工复核的内容" />
      </div>
    </div>
  {:else}
    <section class="app-card" style="margin-bottom:14px;">
      <header class="app-card__head" style="display:flex;justify-content:space-between;align-items:center;">
        <h2>
          待审内容队列
          <span class="app-muted" style="font-size:12px;font-weight:400;">
            （共 {posts.length} 篇待处理）
          </span>
        </h2>
      </header>
      <div class="app-card__body">
        <div class="app-table-wrap">
          <table class="app-table review-table" aria-label="待审队列">
            <thead>
              <tr>
                <th style="min-width:240px;">标题</th>
                <th>发帖人</th>
                <th>所属板块</th>
                <th>提交时间</th>
                <th style="width:100px;text-align:right;">操作</th>
              </tr>
            </thead>
            <tbody>
              {#each pagedPosts as p (p.id)}
                {@const isCurrent = currentPost?.id === p.id}
                <tr class="review-row" class:is-current={isCurrent} onclick={() => openReviewModal(p)}>
                  <td>
                    <b>
                      <a
                        class="review-row__title"
                        href="?tab=content&post={p.id}"
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
                  <td class="adm-acts" style="text-align:right;">
                    <button
                      type="button"
                      class="btn primary sm review-action-btn"
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
        <TablePagination
          bind:currentPage={contentPage}
          bind:pageSize={contentPageSize}
          totalItems={posts.length}
          noun="篇"
        />
      </div>
    </section>
  {/if}

  <!-- 内容审核大弹窗 -->
  <Dialog
    open={reviewModalOpen}
    size="xl"
    title={currentPost ? `审核管理 · ${currentPost.title}` : '审核管理'}
    onclose={closeReviewModal}
  >
    {#if currentPost}
      <div class="audit-modal-body">
        <!-- 发帖人信息 -->
        <div class="author-info-card">
          <div class="author-avatar">{authorInitial}</div>
          <div class="author-meta-block">
            <div class="author-primary-row">
              <span class="author-name">{currentPost.author_username ?? '未知发帖人'}</span>
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
              {#if detailType}
                <span>·</span>
                <span class={detailType.cls} style="padding:1px 6px;border-radius:4px;font-size:11px;">{detailType.label}</span>
              {/if}
              {#if diff?.reason}
                <span>·</span>
                <span class="app-muted">修订理由：{diff.reason}</span>
              {/if}
            </div>
          </div>
        </div>

        <!-- 视图切换与图例 -->
        <div class="view-switch-bar">
          <div class="view-switch-buttons">
            <button
              type="button"
              class="view-switch-btn {activeView === 'content' ? 'is-active' : ''}"
              onclick={() => (activeView = 'content')}
            >
              <Icon name="file-text" size={14} />
              <span>帖子内容</span>
            </button>
            <button
              type="button"
              class="view-switch-btn {activeView === 'diff' ? 'is-active' : ''}"
              onclick={() => (activeView = 'diff')}
            >
              <Icon name="git-compare" size={14} />
              <span>版本比对 (Diff)</span>
            </button>
          </div>
          {#if activeView === 'diff' && diff}
            <div class="review-diff__legend">
              <span class="diff-chip diff-chip--added">新增</span>
              <span class="diff-chip diff-chip--removed">删除</span>
              <span class="diff-chip diff-chip--changed">变更</span>
              <span class="review-diff__versions">
                {diff.from_version === null ? '首次提交' : `v${diff.from_version} → v${diff.to_version}`}
              </span>
            </div>
          {/if}
        </div>

        {#if modalLoading}
          <div style="padding:40px;text-align:center;" class="app-muted">正在载入版本数据...</div>
        {:else if activeView === 'content'}
          <div class="post-preview-content">
            <h1 class="post-preview-title">{currentPost.title}</h1>
            <div class="post-preview-body markdown-body">
              {#if diff?.after_html}
                <SafeHtml html={diff.after_html} />
              {:else if diff?.after_body}
                <pre class="plain-body-preview">{diff.after_body}</pre>
              {:else}
                <p class="app-muted">暂无内容预览</p>
              {/if}
            </div>
          </div>
        {:else}
          <div class="post-diff-container">
            {#if diff}
              <GitDiffViewer
                beforeBody={diff.before_body}
                afterBody={diff.after_body}
                fromVersion={diff.from_version}
                toVersion={diff.to_version}
                reason={diff.reason}
                defaultMode="split"
              />
            {:else}
              <div class="alert alert-warning" style="margin:20px;">
                {data.content.diff_error ?? '无法计算版本差异'}
              </div>
            {/if}
          </div>
        {/if}

        <!-- 驳回理由面板 -->
        {#if rejecting}
          <div class="reject-drawer">
            <h4 class="reject-drawer__title">请选择或输入不通过（驳回）原因：</h4>
            <div class="quick-reasons-wrap">
              {#each REJECT_QUICK_REASONS as reason}
                <button
                  type="button"
                  class="quick-reason-chip"
                  onclick={() => (rejectReason = reason)}
                >
                  {reason}
                </button>
              {/each}
            </div>
            <textarea
              class="form-control reject-textarea"
              rows={3}
              placeholder="请输入具体的驳回或修改建议说明（将通知作者并记录审计）..."
              bind:value={rejectReason}
            ></textarea>
          </div>
        {/if}

        <!-- 底部两大操作：通过 vs 不通过 -->
        <div class="audit-modal-footer">
          <div class="footer-left">
            <button type="button" class="btn ghost sm" onclick={closeReviewModal}>
              取消
            </button>
          </div>
          <div class="footer-right">
            {#if !rejecting}
              <button
                type="button"
                class="btn danger"
                onclick={() => (rejecting = true)}
              >
                <Icon name="x-circle" size={16} />
                <span>不通过...</span>
              </button>
              <form method="POST" action="?/approve" use:enhance={() => {
                reviewSubmitting = true;
                return async ({ update, result }) => {
                  reviewSubmitting = false;
                  toastActionResult(result, {
                    message: (d) => (d?.message as string | null) ?? (result.type === 'success' ? '审核通过并公开发布' : '操作失败')
                  });
                  await update();
                  closeReviewModal();
                };
              }}>
                <input type="hidden" name="id" value={currentPost.id} />
                <input type="hidden" name="reason" value="内容合规，同意发布" />
                <button type="submit" class="btn primary" disabled={reviewSubmitting}>
                  <Icon name="check-circle" size={16} />
                  <span>通过审核</span>
                </button>
              </form>
            {:else}
              <button
                type="button"
                class="btn ghost sm"
                onclick={() => { rejecting = false; rejectReason = ''; }}
              >
                返回
              </button>
              <form method="POST" action="?/reject" use:enhance={() => {
                reviewSubmitting = true;
                return async ({ update, result }) => {
                  reviewSubmitting = false;
                  toastActionResult(result, {
                    message: (d) => (d?.message as string | null) ?? (result.type === 'success' ? '已驳回并退回草稿' : '操作失败')
                  });
                  await update();
                  closeReviewModal();
                };
              }}>
                <input type="hidden" name="id" value={currentPost.id} />
                <input type="hidden" name="reason" value={rejectReason} />
                <button
                  type="submit"
                  class="btn danger"
                  disabled={reviewSubmitting || !rejectReason.trim()}
                >
                  <Icon name="x" size={16} />
                  <span>确认驳回</span>
                </button>
              </form>
            {/if}
          </div>
        </div>
      </div>
    {/if}
  </Dialog>
{/if}

<!-- ═════════════════════ TAB 2: 用户举报案件 ═════════════════════ -->
{#if currentTab === 'cases'}
  <!-- 案件状态筛选 Tab -->
  <div class="tabs" role="tablist" aria-label="案件状态筛选" style="margin-bottom:14px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);padding:2px;">
    {#each caseStatusTabs as tab}
      <a
        role="tab"
        aria-selected={currentCaseStatus === tab.key}
        href={tab.key ? `/admin/moderation?tab=cases&status=${tab.key}` : '/admin/moderation?tab=cases'}
        class="tab {currentCaseStatus === tab.key ? 'is-active' : ''}"
        style="padding:8px 14px;font-size:13px;text-decoration:none;"
      >
        {tab.label} {tab.count}
      </a>
    {/each}
  </div>

  {#if data.cases.forbidden}
    <div class="alert alert-danger" role="alert" style="margin-bottom:14px;">
      没有案件审核权限（moderation.review required）。
    </div>
  {:else if data.cases.error}
    <div class="alert alert-danger" role="alert" style="margin-bottom:14px;">
      加载失败：{data.cases.error}
    </div>
  {:else if displayedCases.length === 0}
    <div class="app-card">
      <div class="app-card__body">
        <EmptyState icon="shield-check" title="暂无举报案件" desc="当前筛选状态下没有需要处理的违规举报案件" />
      </div>
    </div>
  {:else}
    <!-- 批量操作栏 -->
    <BatchBar
      count={selectedCaseIds.length}
      noun="个案件"
      onclear={clearSelection}
    >
      <Button text="标记处理中" variant="secondary" size="sm" onclick={() => (batchStatusOpen = true)} />
      <Button text="批量关闭" variant="secondary" size="sm" onclick={() => (batchCloseOpen = true)} />
      <Button text="批量驳回" variant="danger" size="sm" onclick={() => (batchRejectOpen = true)} />
    </BatchBar>

    <section class="app-card" style="margin-bottom:14px;">
      <div class="app-card__body">
        <div class="app-table-wrap">
          <table class="app-table">
            <thead>
              <tr>
                <th style="width:36px;text-align:center;">
                  <input
                    type="checkbox"
                    aria-label="全选本页案件"
                    checked={allCasesSelected}
                    onchange={toggleAllCases}
                  />
                </th>
                <th style="width:140px;">案件编号</th>
                <th style="min-width:240px;">案件标题</th>
                <th style="width:100px;">优先级</th>
                <th style="width:90px;">状态</th>
                <th style="width:120px;">负责人</th>
                <th style="width:110px;">提交时间</th>
                <th style="width:90px;text-align:right;">操作</th>
              </tr>
            </thead>
            <tbody>
              {#each pagedCases as item (item.id)}
                {@const isSelected = selectedCaseIds.includes(item.id)}
                {@const pBadge = priorityBadge(item.priority)}
                {@const sBadge = caseStatusBadge(item.status)}
                <tr
                  class="clickable-row"
                  class:is-selected={isSelected}
                  onclick={(e) => handleCaseRowClick(e, item.id)}
                >
                  <td onclick={(e) => e.stopPropagation()} style="text-align:center;">
                    <input
                      type="checkbox"
                      aria-label="选择案件 {item.id}"
                      checked={isSelected}
                      onchange={() => toggleCaseRow(item.id)}
                    />
                  </td>
                  <td>
                    <b>
                      <a href="/admin/moderation/cases/{item.id}" class="text-link" style="font-family:var(--font-mono, monospace);font-size:12px;">
                        {item.id.length > 16 ? item.id.slice(0, 8) + '…' + item.id.slice(-4) : item.id}
                      </a>
                    </b>
                  </td>
                  <td>
                    <a href="/admin/moderation/cases/{item.id}" class="text-link" style="font-weight:600;font-size:13px;">
                      {item.title || '（无标题案件）'}
                    </a>
                  </td>
                  <td><span class={pBadge.cls} style="padding:2px 6px;border-radius:4px;font-size:11px;">{pBadge.label}</span></td>
                  <td><span class={sBadge.cls} style="padding:2px 6px;border-radius:4px;font-size:11px;">{sBadge.label}</span></td>
                  <td><span class="app-muted" style="font-size:12px;">{item.assigned_to ?? '未指派'}</span></td>
                  <td><span class="text-secondary" style="font-size:12px;">{formatRelative(item.created_at)}</span></td>
                  <td style="text-align:right;">
                    <a href="/admin/moderation/cases/{item.id}" class="btn secondary sm">
                      处理
                    </a>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
        <TablePagination
          bind:currentPage={casesPage}
          bind:pageSize={casesPageSize}
          totalItems={displayedCases.length}
          noun="起"
        />
      </div>
    </section>

    <!-- 批量标记处理中弹窗 -->
    <Dialog open={batchStatusOpen} title="批量标记状态" onclose={() => (batchStatusOpen = false)}>
      <form method="POST" action="?/batchStatus" use:enhance={() => {
        return async ({ update, result }) => {
          toastActionResult(result, {
            message: (d) => (d?.message as string | null) ?? (result.type === 'success' ? '状态已批量更新' : '批量标记失败')
          });
          await update();
          batchStatusOpen = false;
          clearSelection();
        };
      }}>
        {#each selectedCaseIds as id}
          <input type="hidden" name="ids" value={id} />
        {/each}
        <div style="margin-bottom:14px;">
          <label class="form-label" for="batch-status-select">目标状态：</label>
          <select id="batch-status-select" class="form-control" bind:value={batchStatus} name="status">
            <option value="triaged">处理中 (triaged)</option>
            <option value="investigating">深入调查 (investigating)</option>
          </select>
        </div>
        <div style="display:flex;justify-content:flex-end;gap:8px;">
          <button type="button" class="btn ghost sm" onclick={() => (batchStatusOpen = false)}>取消</button>
          <button type="submit" class="btn primary sm">确定更新</button>
        </div>
      </form>
    </Dialog>

    <!-- 批量关闭：表单常驻 DOM，由 DangerConfirm onconfirm 触发 requestSubmit -->
    <form
      id="batch-close-form"
      method="POST"
      action="?/batchClose"
      bind:this={batchCloseForm}
      use:enhance={() => {
        isSubmitting = true;
        return async ({ result, update }) => {
          isSubmitting = false;
          toastActionResult(result, {
            message: (d) => (d?.message as string | null) ?? (result.type === 'success' ? '批量关闭完成' : '批量关闭失败')
          });
          await update();
          if (result.type === 'success') {
            clearSelection();
            batchCloseOpen = false;
            batchCloseReason = '';
          }
        };
      }}
    >
      <input type="hidden" name="ids" value={selectedCaseIds.join(',')} />
      <input type="hidden" name="reason" value={batchCloseReason} />
    </form>

    <DangerConfirm
      open={batchCloseOpen}
      title="批量关闭案件"
      description={`将把 ${selectedCaseIds.length} 个案件标记为已解决（resolved）；原因作为处理结论存档并写审计。`}
      confirmText="确认批量关闭"
      busy={isSubmitting}
      error={batchCloseError}
      oncancel={() => {
        batchCloseOpen = false;
        batchCloseError = '';
      }}
      onconfirm={() => {
        if (!batchCloseReason.trim()) {
          batchCloseError = '关闭原因必填（写审计）';
          return;
        }
        batchCloseError = '';
        batchCloseForm?.requestSubmit();
      }}
    >
      <label class="input-label" for="batch-close-reason-input">关闭原因（写审计）</label>
      <input id="batch-close-reason-input" class="input-field" bind:value={batchCloseReason} placeholder="必填" required />
    </DangerConfirm>

    <!-- 批量驳回：表单常驻 DOM，由 DangerConfirm onconfirm 触发 requestSubmit -->
    <form
      id="batch-reject-form"
      method="POST"
      action="?/batchReject"
      bind:this={batchRejectForm}
      use:enhance={() => {
        isSubmitting = true;
        return async ({ result, update }) => {
          isSubmitting = false;
          toastActionResult(result, {
            message: (d) => (d?.message as string | null) ?? (result.type === 'success' ? '批量驳回完成' : '批量驳回失败')
          });
          await update();
          if (result.type === 'success') {
            clearSelection();
            batchRejectOpen = false;
            batchRejectReason = '';
          }
        };
      }}
    >
      <input type="hidden" name="ids" value={selectedCaseIds.join(',')} />
      <input type="hidden" name="reason" value={batchRejectReason} />
    </form>

    <DangerConfirm
      open={batchRejectOpen}
      title="批量驳回案件"
      description={`将把 ${selectedCaseIds.length} 个案件标记为已驳回（rejected）；原因作为处理结论存档并写审计。`}
      confirmText="确认批量驳回"
      busy={isSubmitting}
      error={batchRejectError}
      oncancel={() => {
        batchRejectOpen = false;
        batchRejectError = '';
      }}
      onconfirm={() => {
        if (!batchRejectReason.trim()) {
          batchRejectError = '驳回原因必填（写审计）';
          return;
        }
        batchRejectError = '';
        batchRejectForm?.requestSubmit();
      }}
    >
      <label class="input-label" for="batch-reject-reason-input">驳回原因（写审计）</label>
      <input id="batch-reject-reason-input" class="input-field" bind:value={batchRejectReason} placeholder="必填" required />
    </DangerConfirm>
  {/if}
{/if}

<style>
  .moderation-header {
    margin-bottom: 18px;
  }
  .moderation-header__top {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 16px;
    margin-bottom: 16px;
  }
  .moderation-title {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .moderation-metrics {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
  }
  .metric-chip {
    display: inline-flex;
    flex-direction: column;
    padding: 6px 14px;
    background: var(--color-bg-subtle);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    min-width: 80px;
  }
  .metric-chip--warn {
    border-color: var(--color-primary);
    background: rgba(var(--color-primary-rgb, 59, 130, 246), 0.08);
  }
  .metric-chip__label {
    font-size: 11px;
    color: var(--color-text-muted);
  }
  .metric-chip__val {
    font-size: 18px;
    font-weight: 700;
    color: var(--color-text);
  }
  .metric-chip--warn .metric-chip__val {
    color: var(--color-primary);
  }

  .moderation-main-tabs {
    display: flex;
    gap: 8px;
    border-bottom: 2px solid var(--color-border-subtle);
    padding-bottom: 0;
  }
  .main-tab {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 10px 18px;
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-muted);
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    margin-bottom: -2px;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .main-tab:hover {
    color: var(--color-text);
  }
  .main-tab.is-active {
    color: var(--color-primary);
    border-bottom-color: var(--color-primary);
  }
  .tab-badge {
    padding: 1px 7px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 700;
    line-height: 1.2;
  }
  .tab-badge--hot {
    background: #ef4444;
    color: #fff;
  }
  .tab-badge--brand {
    background: var(--color-primary);
    color: #fff;
  }

  .review-row {
    cursor: pointer;
    transition: background-color 0.12s;
  }
  .review-row:hover {
    background-color: var(--color-bg-subtle);
  }
  .review-row__title {
    color: var(--color-text);
    text-decoration: none;
  }
  .review-row__title:hover {
    color: var(--color-primary);
  }

  /* 审核大弹窗样式 */
  .audit-modal-body {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .author-info-card {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    background: var(--color-bg-subtle);
    border-radius: var(--radius-md);
  }
  .author-avatar {
    width: 40px;
    height: 40px;
    border-radius: 50%;
    background: var(--color-primary);
    color: #fff;
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 700;
    font-size: 16px;
  }
  .author-meta-block {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .author-primary-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .author-name {
    font-weight: 600;
    font-size: 14px;
  }
  .author-role-tag {
    font-size: 11px;
    background: rgba(0, 0, 0, 0.06);
    padding: 1px 6px;
    border-radius: 4px;
    color: var(--color-text-muted);
  }
  .author-board-tag {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: var(--color-primary);
  }
  .author-secondary-row {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--color-text-muted);
  }
  .status-badge-pending {
    color: #f59e0b;
    font-weight: 600;
  }

  .view-switch-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 0;
  }
  .view-switch-buttons {
    display: flex;
    gap: 4px;
    background: var(--color-bg-subtle);
    padding: 3px;
    border-radius: var(--radius-md);
  }
  .view-switch-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    font-size: 12px;
    font-weight: 500;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
    cursor: pointer;
  }
  .view-switch-btn.is-active {
    background: var(--color-bg);
    color: var(--color-text);
    box-shadow: 0 1px 3px rgba(0,0,0,0.08);
  }

  .post-preview-content {
    background: var(--color-bg);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: 24px;
    max-height: min(65vh, 640px);
    overflow-y: auto;
  }
  .post-preview-title {
    font-size: 20px;
    font-weight: 700;
    margin: 0 0 16px 0;
  }
  .plain-body-preview {
    white-space: pre-wrap;
    word-break: break-word;
    font-family: inherit;
    margin: 0;
  }

  .post-diff-container {
    max-height: min(65vh, 640px);
    overflow-y: auto;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
  }
  .review-diff__legend {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
  }
  .diff-chip {
    padding: 2px 6px;
    border-radius: 4px;
    font-size: 11px;
    font-weight: 600;
  }
  .diff-chip--added { background: rgba(34, 197, 94, 0.15); color: #16a34a; }
  .diff-chip--removed { background: rgba(239, 68, 68, 0.15); color: #dc2626; }
  .diff-chip--changed { background: rgba(234, 179, 8, 0.15); color: #ca8a04; }
  .review-diff__versions { color: var(--color-text-muted); font-size: 11px; }

  .reject-drawer {
    background: rgba(239, 68, 68, 0.05);
    border: 1px solid rgba(239, 68, 68, 0.2);
    border-radius: var(--radius-md);
    padding: 14px;
  }
  .reject-drawer__title {
    margin: 0 0 8px 0;
    font-size: 13px;
    font-weight: 600;
    color: #dc2626;
  }
  .quick-reasons-wrap {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 10px;
  }
  .quick-reason-chip {
    padding: 4px 10px;
    background: var(--color-bg);
    border: 1px solid var(--color-border);
    border-radius: 999px;
    font-size: 12px;
    cursor: pointer;
  }
  .quick-reason-chip:hover {
    border-color: #dc2626;
    color: #dc2626;
  }
  .reject-textarea {
    width: 100%;
    resize: vertical;
  }

  .audit-modal-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    border-top: 1px solid var(--color-border);
    padding-top: 12px;
    margin-top: 4px;
  }
  .footer-left {
    display: flex;
    align-items: center;
  }
  .footer-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .clickable-row {
    cursor: pointer;
  }
  .clickable-row:hover {
    background: var(--color-bg-subtle);
  }
</style>
