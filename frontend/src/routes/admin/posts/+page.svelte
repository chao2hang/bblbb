<script lang="ts">
  // GAP-FIX（管理域·帖子管理）：1:1 对齐原型 prototype/pages/admin-posts.html
  // - 顶部 .app-filter-tabs 状态筛选 Tab 链接（全部 / 待审核 / 公开 / 精华 / 已隐藏 / 已删除）
  // - .app-toolbar 搜索条
  // - .app-card > .app-card__head + .app-table 数据表格
  // - 行操作按钮（td.adm-acts）
  // M18-ADMIN-DIALOG：9 个行内审核 POST 表单收敛为弹层。
  // M18-ADMIN-OPS：行操作进一步收敛为**每行一个「操作」按钮** → 弹层内以动作
  // chips 选择具体操作（通过/驳回/精华/隐藏/恢复/发布/删除）+ reason 必填，
  // 单表单提交到既有 ?/moderate 契约（POST /api/v1/admin/posts/{id}/action）；
  // 危险动作（删除/隐藏）选中时提交按钮转 danger 样式并二次确认文案。
  // 已发布帖的「代改」同样收进「⋮」菜单（goto 客户端 GET 导航跳编辑器）；「查看原帖」保持 <a>。
  // 批量：选择列 + BatchBar + 批量审核 Dialog（选动作 + 理由）→ ?/batchModerate。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { goto } from '$app/navigation';
  import { enhance } from '$app/forms';
  import BatchBar from '$lib/components/admin/BatchBar.svelte';
  import ExportButton from '$lib/components/admin/ExportButton.svelte';
  import FilterTabs from '$lib/components/admin/FilterTabs.svelte';
  import RowActionsMenu, { type RowActionItem } from '$lib/components/admin/RowActionsMenu.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { adminStateLabel } from '$lib/admin';
  import { toastActionResult } from '$lib/ui/action-toast';
  import type { AdminPostItem } from '$lib/api/types';
    import type { AdminPostsActionData, AdminPostsPageData } from './+page.server';

  let { data, form }: { data: AdminPostsPageData; form?: AdminPostsActionData | null } = $props();

  const POST_STATUS_TABS: { value: string; label: string }[] = [
    { value: '', label: '全部' },
    { value: 'pending_review', label: '待审核' },
    { value: 'published', label: '公开' },
    { value: 'featured', label: '精华' },
    { value: 'hidden', label: '已隐藏' },
    { value: 'deleted', label: '已删除' }
  ];

  /** 审核动作白名单（与 ?/moderate 服务端契约一致；删除含在行操作弹层中）。 */
  const MODERATE_ACTIONS = [
    'approve',
    'reject',
    'hide',
    'restore',
    'feature',
    'unfeature',
    'delete'
  ] as const;
  type ModerateAction = (typeof MODERATE_ACTIONS)[number];

  /** 行操作弹层选项（按行状态给出可用动作；danger 动作提交时二次确认文案）。 */
  interface OpsOption {
    action: ModerateAction;
    label: string;
    danger?: boolean;
  }

  function opsOptionsFor(item: AdminPostItem): OpsOption[] {
    if (item.status === 'pending_review') {
      return [
        { action: 'approve', label: '通过（发布）' },
        { action: 'reject', label: '驳回', danger: true }
      ];
    }
    if (item.status === 'published') {
      return item.is_featured
        ? [
            { action: 'unfeature', label: '取消精华' },
            { action: 'hide', label: '隐藏', danger: true }
          ]
        : [
            { action: 'feature', label: '设为精华' },
            { action: 'hide', label: '隐藏', danger: true }
          ];
    }
    if (item.status === 'hidden') {
      return [
        { action: 'restore', label: '恢复' },
        { action: 'delete', label: '删除', danger: true }
      ];
    }
    if (item.status === 'deleted') {
      return [{ action: 'restore', label: '恢复' }];
    }
    return [
      { action: 'approve', label: '发布' },
      { action: 'hide', label: '隐藏', danger: true }
    ];
  }

  /** 批量审核动作选项（label 与行按钮文案一致）。 */
  const BATCH_ACTION_OPTIONS: { value: ModerateAction; label: string }[] = [
    { value: 'approve', label: '通过（发布）' },
    { value: 'reject', label: '驳回' },
    { value: 'hide', label: '隐藏' },
    { value: 'restore', label: '恢复' },
    { value: 'feature', label: '设为精华' },
    { value: 'unfeature', label: '取消精华' }
  ];

  function formatDateTime(ms: number | null | undefined): string {
    if (!ms) return '—';
    const d = new Date(ms);
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${d.getFullYear()}/${pad(d.getMonth() + 1)}/${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  }

  /** 相对时间（原型风格：今天 HH:MM / 昨天 / N 天前 / 日期）。 */
  function relativeTime(ms: number | null | undefined): string {
    if (!ms) return '—';
    const now = new Date();
    const d = new Date(ms);
    const sameDay = d.toDateString() === now.toDateString();
    if (sameDay) {
      return `今天 ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
    }
    const yesterday = new Date(now);
    yesterday.setDate(now.getDate() - 1);
    if (d.toDateString() === yesterday.toDateString()) return '昨天';
    const days = Math.floor((now.getTime() - ms) / 86_400_000);
    if (days > 0 && days < 30) return `${days} 天前`;
    return formatDateTime(ms);
  }

  function tabHref(status: string): string {
    const params = new URLSearchParams();
    if (status) params.set('status', status);
    if (data.q) params.set('q', data.q);
    const qs = params.toString();
    return qs ? `/admin/posts?${qs}` : '/admin/posts';
  }

  const nextHref = $derived(
    data.state === 'ok' && data.nextCursor
      ? (() => {
          const params = new URLSearchParams();
          if (data.status) params.set('status', data.status);
          if (data.q) params.set('q', data.q);
          params.set('after', data.nextCursor);
          return `/admin/posts?${params.toString()}`;
        })()
      : null
  );

  const message = $derived(form?.message ?? null);
  const conflict = $derived(form?.conflict === true);

  // JS 启用：动作结果走全局 Toast 浮窗（成功绿/失败红）；顶部内联横幅仅保留为
  // 无 JS 回退（SSR HTML 仍渲染，见 hasJs）。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  // 只展示服务端返回的数据；空列表必须表达真实空态，不能用演示帖子掩盖 API 故障或权限状态。
  const displayedItems = $derived.by(() => {
    let list = data.items ?? [];
    if (data.status) {
      if (data.status === 'featured') {
        list = list.filter((i) => i.is_featured);
      } else if (data.status === 'deleted') {
        list = list.filter((i) => i.status === 'deleted');
      } else if (data.status === 'pending_review') {
        list = list.filter((i) => i.status === 'pending_review' || (i as any).review_status === 'pending_review');
      } else {
        list = list.filter((i) => i.status === data.status);
      }
    }
    if (data.q) {
      const kw = data.q.trim().toLowerCase();
      list = list.filter((i) => (i.title ?? '').toLowerCase().includes(kw) || (i.author_username ?? '').toLowerCase().includes(kw));
    }
    return list;
  });

  function handleStatusChange(val: string) {
    selectedIds = [];
    goto(tabHref(val), { keepFocus: true });
  }

  // M18：复选框与批量选择状态（对齐原型后台表格）
  let selectedIds = $state<string[]>([]);
  let allSelected = $derived(
    displayedItems.length > 0 && selectedIds.length === displayedItems.length
  );
  function toggleAll() {
    if (allSelected) {
      selectedIds = [];
    } else {
      selectedIds = displayedItems.map((i) => i.id);
    }
  }
  function toggleRow(id: string) {
    if (selectedIds.includes(id)) {
      selectedIds = selectedIds.filter((x) => x !== id);
    } else {
      selectedIds = [...selectedIds, id];
    }
  }

  /** 行操作「⋮」菜单 + 弹层（约定 D：菜单选动作，弹层内确认 + reason）。 */
  let opsTarget: AdminPostItem | null = $state(null);
  let opsAction = $state<ModerateAction>('approve');
  let opsReason = $state('');

  const opsOptions = $derived(opsTarget ? opsOptionsFor(opsTarget) : []);
  const opsSelected = $derived(opsOptions.find((o) => o.action === opsAction) ?? null);

  function openOps(item: AdminPostItem, action: ModerateAction): void {
    opsTarget = item;
    opsAction = action;
    opsReason = '';
  }

  function closeOps(): void {
    opsTarget = null;
  }

  /** 行「⋮」菜单项（按行状态给出可用动作）；已发布帖的「代改」也收进菜单。 */
  function rowActions(item: AdminPostItem): RowActionItem[] {
    const actions: RowActionItem[] = opsOptionsFor(item).map((option) => ({
      label: option.label,
      danger: option.danger,
      run: () => openOps(item, option.action)
    }));
    if (item.status === 'published') {
      // 代改为 GET 导航（与原 <a> 链接同语义），点菜单项后客户端跳转编辑器。
      actions.push({
        label: '代改',
        run: () => goto(`/editor?post_id=${encodeURIComponent(item.id)}`)
      });
    }
    return actions;
  }

  /** 批量审核 Dialog（选中行执行同一动作 + 公共理由）。 */
  let batchOpen = $state(false);
  let batchAction = $state<ModerateAction>('approve');
  let batchReason = $state('');

  function openBatchModerate(): void {
    batchAction = 'approve';
    batchReason = '';
    batchOpen = true;
  }

  function statusBadgeInfo(item: { status: string; is_featured?: boolean }): { cls: string; label: string } {
    if (item.is_featured) return { cls: 'sb-brand', label: '精华' };
    switch (item.status) {
      case 'pending_review':
        return { cls: 'sb-hot', label: '待审核' };
      case 'published':
        return { cls: 'sb-success', label: '公开' };
      case 'hidden':
        return { cls: 'sb-gray', label: '已隐藏' };
      case 'deleted':
        return { cls: 'sb-danger', label: '已删除' };
      default:
        return { cls: 'sb-gray', label: item.status };
    }
  }
</script>

<svelte:head>
  <title>帖子与文章 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="帖子与文章" />

<!-- 原型顶部状态过滤 Tab（带计数，M17-GAPFIX-07） -->
<FilterTabs
  ariaLabel="帖子状态筛选"
  tabs={POST_STATUS_TABS.map((t) => ({
    value: t.value,
    label: t.label,
    href: tabHref(t.value),
    active: data.status === t.value,
    count: data.counts ? (data.counts[t.value as keyof typeof data.counts] ?? 0) : undefined
  }))}
/>

{#if data.state === 'forbidden'}
  <p class="input-hint is-error" role="alert"><Icon name="lock" size={14} /> {adminStateLabel('forbidden')}</p>
{:else if data.state === 'not_implemented'}
  <p class="input-hint" role="note">帖子管理接口开发中。</p>
{:else if data.state === 'error'}
  <p class="input-hint is-error" role="alert">{data.error || adminStateLabel('error')}</p>
{:else if data.state === 'ok'}
  {#if message && !hasJs}
    <p class="input-hint {conflict ? 'is-error' : ''}" role="status">{message}</p>
  {/if}
  {#if conflict && !hasJs}
    <p class="input-hint is-error" role="alert">帖子状态已变化，请刷新后重试。</p>
  {/if}

  <section class="app-card">
    <header class="app-card__head">
      <h2>帖子列表</h2>
    </header>

    <div class="app-card__body">
      <!-- 原型对齐工具条：搜索 + 状态筛选 + 清除，单行 flex（窄屏自动换行；修复全宽 select 挤压清除按钮的问题） -->
      <form method="GET" action="/admin/posts" style="display:flex;align-items:center;flex-wrap:wrap;gap:8px;margin-bottom:14px;">
        <input
          type="search"
          name="q"
          value={data.q}
          class="app-field"
          placeholder="搜索当前列表..."
          aria-label="搜索当前列表"
          style="flex:1 1 220px;min-width:0;"
        />
        <select
          name="status"
          class="app-select"
          value={data.status}
          aria-label="状态筛选"
          onchange={(e) => handleStatusChange(e.currentTarget.value)}
          style="flex:0 0 auto;width:168px;"
        >
          <option value="">全部状态</option>
          {#each POST_STATUS_TABS.slice(1) as tab}
            <option value={tab.value}>{tab.label}</option>
          {/each}
        </select>
        {#if data.q || data.status}
          <a href="/admin/posts" class="btn ghost sm" style="flex:0 0 auto;">清除</a>
        {/if}
      </form>

      <!-- 批量工具条（选中 > 0 时渲染；批量动作与理由在 Dialog 内填写） -->
      <BatchBar count={selectedIds.length} onclear={() => (selectedIds = [])}>
        <Button text="批量审核" variant="secondary" size="sm" onclick={openBatchModerate} />
      </BatchBar>

      {#if displayedItems.length === 0}
        <EmptyState icon="inbox" title="暂无帖子" desc="当前筛选下没有符合条件的帖子" />
      {:else}
        <div class="app-table-wrap">
          <table class="app-table" aria-label="帖子列表">
            <thead>
              <tr>
                <th style="width:40px;text-align:center;">
                  <input
                    type="checkbox"
                    checked={allSelected}
                    onchange={toggleAll}
                    aria-label="全选当前列表"
                  />
                </th>
                <th style="min-width:240px;">标题</th>
                <th>作者</th>
                <th>板块</th>
                <th>状态</th>
                <th>时间</th>
                <th style="min-width:200px;">操作</th>
              </tr>
            </thead>
            <tbody>
              {#each displayedItems as item (item.id)}
                {@const badge = statusBadgeInfo(item)}
                <tr>
                  <td style="text-align:center;">
                    <input
                      type="checkbox"
                      checked={selectedIds.includes(item.id)}
                      onchange={() => toggleRow(item.id)}
                      aria-label="选择帖子 {item.id}"
                    />
                  </td>
                  <td>
                    <b>{item.title || '（无标题）'}</b>
                    {#if item.status === 'hidden'}<span class="text-secondary" style="font-size:11px;margin-left:6px;">已被隐藏</span>{/if}
                    <span class="sub" style="display:block;margin-top:3px;">
                      <a class="app-link" href="/posts/{item.id}" target="_blank" rel="noopener noreferrer" style="font-size:11px;">
                        查看原帖
                      </a>
                    </span>
                  </td>
                  <td>
                    <span style="font-weight:500;">{item.author_username}</span>
                  </td>
                  <td>
                    <span class="text-secondary" style="font-size:12px;">{item.board_name || item.board_slug}</span>
                  </td>
                  <td>
                    <span class="sbadge {badge.cls}">{badge.label}</span>
                  </td>
                  <td>
                    <span class="text-secondary" style="font-size:12px;">{relativeTime(item.created_at)}</span>
                  </td>
                  <td class="adm-acts">
                    <div style="display:flex;gap:6px;flex-wrap:wrap;align-items:center;">
                      <!-- 每行一个「⋮」动作菜单：审核动作进弹层；已发布帖的「代改」也在菜单内（约定 D） -->
                      <RowActionsMenu
                        label="更多操作：帖子 {item.title || item.id}"
                        actions={rowActions(item)}
                      />
                    </div>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>

        <footer class="app-card__foot" style="margin-top:14px;justify-content:space-between;">
          <div>
            {#if data.after}
              <a class="btn secondary sm" href={tabHref(data.status)}>第一页</a>
            {/if}
          </div>
          <div style="display:flex;gap:8px;align-items:center;">
            {#if nextHref}
              <a class="btn secondary sm" href={nextHref}>下一页</a>
            {/if}
            <ExportButton
              label="导出内容清单"
              filename="admin-posts"
              columns={[
                { key: 'id', label: 'id' },
                { key: 'title', label: '标题' },
                { key: 'author', label: '作者' },
                { key: 'board', label: '板块' },
                { key: 'status', label: '状态' },
                { key: 'review', label: '审核' },
                { key: 'time', label: '时间' }
              ]}
              getData={() =>
                (data.items ?? []).map((item) => ({
                  id: item.id,
                  title: item.title,
                  author: item.author_username,
                  board: item.board_name || item.board_slug,
                  status: item.status,
                  review: item.review_status,
                  time: relativeTime(item.created_at)
                }))}
            />
          </div>
        </footer>
      {/if}
    </div>
  </section>
{/if}

<!-- 行操作 Dialog（约定 D）：动作由「⋮」菜单选定，弹层内确认 + reason 必填 → 既有 ?/moderate 契约。 -->
<Dialog
  open={opsTarget !== null}
  title={opsSelected ? `审核操作：${opsSelected.label}` : '审核操作'}
  description={opsSelected
    ? `将对帖子「${opsTarget?.title || opsTarget?.id}」执行「${opsSelected.label}」${opsSelected.danger ? '——危险操作，请谨慎确认' : ''}；原因写入审计日志。`
    : '填写操作原因（必填，写入审计日志）。'}
  onclose={closeOps}
>
  <form
    method="POST"
    action="?/moderate"
    use:enhance={() => {
      return async ({ result, update }) => {
        toastActionResult(result);
        await update();
        closeOps();
      };
    }}
  >
    <input type="hidden" name="id" value={opsTarget?.id ?? ''} />
    <input type="hidden" name="action" value={opsAction} />

    <div class="input-wrapper" style="margin-bottom:var(--space-3);">
      <label class="input-label" for="post-ops-reason">操作原因（写审计）</label>
      <input
        id="post-ops-reason"
        name="reason"
        class="input-field"
        required
        bind:value={opsReason}
        placeholder="必填"
      />
    </div>
    <Button
      text={opsSelected?.danger ? `确认${opsSelected.label}` : '确认执行'}
      variant={opsSelected?.danger ? 'danger' : 'primary'}
      size="sm"
      type="submit"
    />
  </form>
</Dialog>

<!-- 批量审核 Dialog：ids 隐藏字段 + 公共动作 + reason 必填 → ?/batchModerate -->
<Dialog
  open={batchOpen}
  title="批量审核"
  description={`将对 ${selectedIds.length} 个帖子执行同一审核动作；原因写审计，逐条调用既有审核端点。`}
  onclose={() => (batchOpen = false)}
>
  <form
    method="POST"
    action="?/batchModerate"
    use:enhance={() => {
      return async ({ result, update }) => {
        toastActionResult(result);
        await update();
        selectedIds = [];
        batchOpen = false;
      };
    }}
  >
    <input type="hidden" name="ids" value={selectedIds.join(',')} />
    <div class="input-wrapper" style="margin-bottom:var(--space-3);">
      <label class="input-label" for="post-batch-action">批量动作</label>
      <select id="post-batch-action" name="action" class="input-field" bind:value={batchAction}>
        {#each BATCH_ACTION_OPTIONS as opt (opt.value)}
          <option value={opt.value}>{opt.label}</option>
        {/each}
      </select>
    </div>
    <div class="input-wrapper" style="margin-bottom:var(--space-3);">
      <label class="input-label" for="post-batch-reason">操作原因（写审计）</label>
      <input
        id="post-batch-reason"
        name="reason"
        class="input-field"
        required
        bind:value={batchReason}
        placeholder="必填"
      />
    </div>
    <Button text="执行批量审核" variant="primary" size="sm" type="submit" />
  </form>
</Dialog>
