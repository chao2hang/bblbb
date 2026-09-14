<script lang="ts">
  // M18-ADMIN-CONTENT：内容审核页（对齐原型 #admin-article-audit）。
  // T-REVIEW-TABLE（交互重构）：待审队列从「横向胶囊队列 + 内嵌对比」改为与其他
  // 管理页一致的 .app-card > .app-table 表格管理——标题/「审核」均为原生 ?post=
  // 链接（无 JS 可用），当前行高亮（aria-current="page"）；选中帖在下方
  // 「版本对比」详情卡片渲染「修改前/后」差别：
  // - 修改（from_version != null）→ 双栏 before/after 对比；
  // - 首次提交（from_version == null）→ 仅新版正文 + 「无先前版本」说明。
  // 队列列表 API（GET /admin/posts?status=pending_review）不含版本号，行级
  // 「首发/修改」类型仅在详情区对当前帖判定（不为每行额外发 N 次修订请求）。
  // 约定 A/D 保留：approve/reject 写表单收进弹层（理由必填写审计）；diff 不可用
  // 时审核菜单项禁用（避免按错误内容审批）。
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import RowActionsMenu from '$lib/components/admin/RowActionsMenu.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { toastActionResult } from '$lib/ui/action-toast';
  import type { AdminContentActionData, AdminContentPageData } from './+page.server';

  let { data, form }: { data: AdminContentPageData; form?: AdminContentActionData | null } = $props();

  const posts = $derived(data.posts);
  const currentPost = $derived(data.current);
  const diff = $derived(data.diff);

  // 审核决定弹层（「⋮」菜单项决定动作；一个 Dialog 服务当前帖的通过/驳回，约定 D）。
  let opsOpen = $state(false);
  let opsAction = $state<'approve' | 'reject'>('approve');
  let opsReason = $state('');

  function openOps(action: 'approve' | 'reject'): void {
    opsAction = action;
    opsReason = '';
    opsOpen = true;
  }

  function closeOps(): void {
    opsOpen = false;
    opsAction = 'approve';
    opsReason = '';
  }

  /** 「⋮」菜单项（约定 D）：通过/驳回；diff 不可用时禁用（避免按错误内容审批）。 */
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

  // JS 启用：动作结果走全局 Toast 浮窗（成功绿/失败红）；顶部内联横幅仅保留为
  // 无 JS 回退（SSR HTML 仍渲染，见 hasJs）。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  // 确定性格式（UTC，避免 SSR/客户端时区差异）：YYYY-MM-DD HH:mm。
  const stampOf = (ms: number) => new Date(ms).toISOString().slice(0, 16).replace('T', ' ');

  /** 详情区类型徽章：修改（有前版）vs 首次提交（仅 v1）；diff 未知时不渲染。 */
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
  <!-- forbidden / error：明确提示（不再误显示“没有待审核的内容”空态） -->
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
  <!-- 1) 待审队列表格（与其他管理页同款 app-table；?post= 原生链接切换，无 JS 可用） -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head">
      <h2>
        待审队列
        <span class="app-muted" style="font-size:12px;font-weight:400;">（共 {posts.length} 篇 · 点击「审核」进入对比）</span>
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
              <th style="width:90px;">操作</th>
            </tr>
          </thead>
          <tbody>
            {#each posts as p (p.id)}
              {@const isCurrent = p.id === currentPost.id}
              <tr class:is-current={isCurrent} aria-current={isCurrent ? 'page' : undefined}>
                <td>
                  <b>
                    <a class="review-row__title" href="?post={p.id}#review-detail">{p.title || '（无标题）'}</a>
                  </b>
                </td>
                <td><span style="font-weight:500;">{p.author_username ?? '未知作者'}</span></td>
                <td><span class="text-secondary" style="font-size:12px;">{p.board_name ?? '未分区'}</span></td>
                <td><span class="text-secondary" style="font-size:12px;">{stampOf(p.created_at)}</span></td>
                <td class="adm-acts">
                  <a href="?post={p.id}#review-detail" class="btn ghost sm" style="text-decoration:none;">审核</a>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
  </section>

  <!-- 2) 审核详情（选中帖）：修改前/后版本对比 + 审核操作（约定 D 弹层） -->
  <section class="app-card" id="review-detail">
    <header class="app-card__head" style="padding:16px 20px;">
      <h2 style="margin:0;font-size:17px;font-weight:700;line-height:1.4;">
        版本对比 · {currentPost.title}
      </h2>
    </header>

    <div class="app-card__body" style="padding:16px 20px;">
      {#if diff}
        <!-- 类型徽章（首次提交/修改）+ 变动类别图例 + 版本号（对齐原型 app-diff__legend） -->
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

        <!-- 修改前/后 diff 块（正文为修订快照原文 markdown，只读展示） -->
        <div class="review-diff__grid">
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
        {#if diff.reason}
          <p class="review-diff__reason">修订说明：{diff.reason}</p>
        {/if}
      {:else}
        <!-- 对比数据不可用：禁用审核操作，避免按错误内容审批 -->
        <div class="review-data-warning" role="alert">
          <strong>当前无法进行版本对比</strong>
          <span>
            {data.diff_error ?? '没有可用的修订快照数据。为避免按错误内容审批，审核操作暂时禁用。'}
          </span>
        </div>
      {/if}

      <!-- 审核操作区（写操作 = 「⋮」三点菜单 + Dialog，约定 D；diff 不可用 → 菜单项禁用 + 占位提示） -->
      <div style="display:flex;gap:10px;align-items:center;flex-wrap:wrap;">
        <RowActionsMenu label="更多操作：待审帖 {currentPost.title}" actions={reviewMenuActions()} />
        {#if !diff}
          <!-- 对比数据不可用：审核菜单项禁用，避免按错误内容审批 -->
          <span class="review-entry--disabled" title="需要真实版本对比数据后才能审核">审核暂不可用</span>
        {/if}
      </div>
    </div>
  </section>

  <!-- 审核决定 Dialog（约定 D）：动作由「⋮」菜单项决定（通过/驳回），单一 reason 必填；
       提交按钮以原生 formaction 提交到既有 ?/approve / ?/reject（id + reason 契约不变）。 -->
  <Dialog
    open={opsOpen && !!currentPost}
    title={currentPost ? `审核决定 · ${currentPost.title}` : '审核决定'}
    description={!currentPost
      ? ''
      : opsAction === 'reject'
        ? `将把「${currentPost.title}」退回草稿——驳回后作者可修改后重新提交；驳回理由写入审计日志。`
        : `将公开发布「${currentPost.title}」；通过原因写入审计日志。`}
    onclose={closeOps}
  >
    <form
      method="POST"
      action="?/approve"
      use:enhance={() => {
        return async ({ result, update }) => {
          // 动作结果 → 全局 Toast（成功“已通过审核并公开发布 / 已驳回并退回草稿”/
          // 失败服务端文案；失败在 data.error，无 message 字段）。顶部横幅为无 JS 回退。
          toastActionResult(result, { message: (d) => (d?.message ?? d?.error) as string | null });
          await update();
          await invalidateAll();
          // 仅成功时关闭弹层并清空草稿：失败保留理由便于修正（错误经 Toast/横幅呈现）。
          if (result.type === 'success') {
            closeOps();
          }
        };
      }}
      style="display:flex;flex-direction:column;gap:12px;"
    >
      <input type="hidden" name="id" value={currentPost?.id ?? ''} />

      <div class="input-wrapper" style="margin-bottom:0;">
        <label class="input-label" for="content-ops-reason">
          {opsAction === 'reject' ? '驳回理由（审计必填）' : '通过原因（审计必填）'}
        </label>
        <input
          id="content-ops-reason"
          type="text"
          name="reason"
          bind:value={opsReason}
          class="input-field"
          placeholder={opsAction === 'reject' ? '填写驳回理由（必填，写入审计）' : '例如：内容合规，同意发布'}
          required
        />
      </div>
      <div style="display:flex;gap:8px;justify-content:flex-end;">
        <button type="button" class="btn ghost sm" onclick={closeOps}>取消</button>
        <Button
          text={opsAction === 'reject' ? '确认驳回' : '确认通过'}
          variant={opsAction === 'reject' ? 'danger' : 'primary'}
          size="sm"
          type="submit"
          formaction={opsAction === 'reject' ? '?/reject' : '?/approve'}
        />
      </div>
    </form>
  </Dialog>
{/if}

<style>
  /* 待审队列表格（与其他管理页 app-table 同款；当前行高亮） */
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

  /* diff 图例 */
  .review-diff__legend {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    margin-bottom: var(--space-4);
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

  /* 修改前/后对比块（对齐原型 app-diff__grid：双栏，窄屏单栏） */
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
    max-height: 320px;
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
    margin: 0 0 var(--space-4);
    color: var(--color-text-secondary);
    font-size: var(--text-sm);
  }

  /* 审核入口禁用态占位（对比不可用时不放行审核决定弹层） */
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

  /* 对比不可用告警（审核操作禁用时展示） */
  .review-data-warning {
    display: grid;
    gap: var(--space-1);
    margin-bottom: var(--space-4);
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--color-warning);
    background: var(--color-warning-soft);
    color: var(--color-text-primary);
  }

  .review-data-warning span {
    color: var(--color-text-secondary);
    font-size: var(--text-sm);
    line-height: 1.6;
  }
</style>
