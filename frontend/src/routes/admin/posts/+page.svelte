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
  import SafeHtml from '$lib/components/SafeHtml.svelte';
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
    'pin',
    'unpin',
    'lock',
    'unlock',
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
      const opts: OpsOption[] = [];
      opts.push(
        item.is_featured
          ? { action: 'unfeature', label: '取消精华' }
          : { action: 'feature', label: '设为精华' }
      );
      opts.push(
        item.is_pinned
          ? { action: 'unpin', label: '取消置顶' }
          : { action: 'pin', label: '置顶' }
      );
      opts.push(
        item.is_locked
          ? { action: 'unlock', label: '解除锁定' }
          : { action: 'lock', label: '锁定（禁评）' }
      );
      opts.push({ action: 'hide', label: '隐藏', danger: true });
      return opts;
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
  let submitting = $state(false);
  let selectedIds = $state<string[]>([]);
  const allSelected = $derived(
    displayedItems.length > 0 && displayedItems.every((item) => selectedIds.includes(item.id))
  );
  const someSelected = $derived(
    displayedItems.some((item) => selectedIds.includes(item.id))
  );
  function toggleAll() {
    if (allSelected) {
      selectedIds = selectedIds.filter((id) => !displayedItems.some((i) => i.id === id));
    } else {
      const currentIds = displayedItems.map((i) => i.id);
      selectedIds = Array.from(new Set([...selectedIds, ...currentIds]));
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

  /** 帖子预览 Dialog 状态（点击标题在后台弹窗内直接预览，免去跳转前台） */
  let previewTarget = $state<AdminPostItem | null>(null);
  let previewLoading = $state(false);
  let previewData = $state<{
    title?: string;
    body_html?: string | null;
    tags?: string[];
    status?: string;
    created_at?: number;
    reply_count?: number;
    view_count?: number;
    author?: { username?: string; display_name?: string | null };
  } | null>(null);
  let previewError = $state<string | null>(null);

  async function openPreview(item: AdminPostItem): Promise<void> {
    previewTarget = item;
    previewLoading = true;
    previewError = null;
    previewData = null;

    try {
      const res = await fetch(`/api/v1/posts/${encodeURIComponent(item.id)}`, {
        headers: { Accept: 'application/json' }
      });
      if (!res.ok) {
        // 若端点异常，尝试通过 admin/posts/{id}/revisions 读取最新修订正文
        const revRes = await fetch(`/api/v1/admin/posts/${encodeURIComponent(item.id)}/revisions`, {
          headers: { Accept: 'application/json' }
        });
        if (revRes.ok) {
          const revJson = await revRes.json();
          const items = revJson?.items ?? [];
          const latest = items[items.length - 1];
          previewData = {
            title: item.title,
            body_html: latest?.body_html ?? null,
            created_at: item.created_at,
            status: item.status
          };
        } else {
          previewError = `加载失败（HTTP ${res.status}）`;
        }
      } else {
        const json = await res.json();
        previewData = json;
      }
    } catch (e: any) {
      previewError = e?.message || '网络请求失败';
    } finally {
      previewLoading = false;
    }
  }

  function closePreview(): void {
    previewTarget = null;
    previewData = null;
    previewError = null;
  }

  /** 行「⋮」菜单项（按行状态给出可用动作）；已发布帖的「代改」也收进菜单。 */
  function rowActions(item: AdminPostItem): RowActionItem[] {
    const actions: RowActionItem[] = opsOptionsFor(item).map((option) => ({
      label: option.label,
      danger: option.danger,
      run: () => openOps(item, option.action)
    }));
    if (item.status === 'pending_review') {
      actions.unshift({
        label: '版本对比审核',
        run: () => goto(`/admin/content?post=${encodeURIComponent(item.id)}`)
      });
    }
    if (item.status === 'published') {
      // 代改为 GET 导航（与原 <a> 链接同语义），点菜单项后客户端跳转编辑器。
      actions.push({
        label: '代改',
        run: () => goto(`/editor?post_id=${encodeURIComponent(item.id)}`)
      });
    }
    // 查看原帖：收敛在操作菜单中（支持在新标签页前台预览，即使已删除/待审核，管理员也可正常访问）
    actions.push({
      label: '前台查看原帖',
      run: () => window.open(`/posts/${encodeURIComponent(item.id)}`, '_blank')
    });
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
                  <td style="min-width:240px;">
                    <button
                      type="button"
                      class="admin-post-title-btn"
                      onclick={() => openPreview(item)}
                      style="background:none;border:none;padding:0;font:inherit;text-align:left;cursor:pointer;color:var(--color-text-primary);font-weight:600;display:inline-block;"
                      title="点击在弹窗中预览帖子"
                    >
                      <span class="admin-post-title-text" style="text-decoration:underline;text-underline-offset:3px;">{item.title || '（无标题）'}</span>
                    </button>
                    {#if item.status === 'hidden'}<span class="text-secondary" style="font-size:11px;margin-left:6px;">已被隐藏</span>{/if}
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
                displayedItems.map((item) => ({
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
      submitting = true;
      return async ({ result, update }) => {
        submitting = false;
        toastActionResult(result);
        await update();
        if (result.type === 'success') closeOps();
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
      text={submitting ? '提交中...' : (opsSelected?.danger ? `确认${opsSelected.label}` : '确认执行')}
      variant={opsSelected?.danger ? 'danger' : 'primary'}
      size="sm"
      type="submit"
      disabled={submitting}
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
      submitting = true;
      return async ({ result, update }) => {
        submitting = false;
        toastActionResult(result);
        await update();
        if (result.type === 'success') {
          selectedIds = [];
          batchOpen = false;
        }
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
    <Button text={submitting ? '执行中...' : '执行批量审核'} variant="primary" size="sm" type="submit" disabled={submitting} />
  </form>
</Dialog>

<!-- 帖子内容预览 Dialog（点击列表标题直接在后台弹窗预览，免去跳出后台） -->
<Dialog
  open={previewTarget !== null}
  title={previewTarget?.title || '帖子预览'}
  description={previewTarget ? `作者：${previewTarget.author_username || '匿名'} · 板块：${previewTarget.board_name || previewTarget.board_slug || '综合'} · 状态：${statusBadgeInfo(previewTarget).label}` : ''}
  size="lg"
  onclose={closePreview}
>
  <div class="admin-post-preview-modal" style="display:flex;flex-direction:column;gap:var(--space-4);min-height:220px;max-height:65vh;overflow-y:auto;padding-right:4px;">
    {#if previewLoading}
      <div style="display:flex;flex-direction:column;align-items:center;justify-content:center;padding:48px 0;color:var(--color-text-secondary);gap:12px;">
        <Icon name="loader" size={24} />
        <span>正在加载帖子正文...</span>
      </div>
    {:else if previewError}
      <div style="padding:16px 20px;border-radius:var(--radius-md, 6px);background:rgba(239, 68, 68, 0.08);border:1px solid rgba(239, 68, 68, 0.25);color:var(--color-danger, #ef4444);display:flex;align-items:center;gap:10px;">
        <Icon name="alert-triangle" size={18} />
        <span>{previewError}</span>
      </div>
    {:else if previewData}
      <!-- 元信息卡片 -->
      <div style="display:flex;align-items:center;gap:14px;flex-wrap:wrap;padding:10px 14px;background:var(--color-bg-subtle, rgba(255,255,255,0.04));border-radius:var(--radius-sm, 6px);border:1px solid var(--color-border);font-size:12px;color:var(--color-text-secondary);">
        <span>创建时间：{formatDateTime(previewData.created_at || previewTarget?.created_at)}</span>
        {#if typeof previewData.view_count === 'number'}
          <span>浏览量：{previewData.view_count}</span>
        {/if}
        {#if typeof previewData.reply_count === 'number'}
          <span>回复数：{previewData.reply_count}</span>
        {/if}
        {#if previewData.tags && previewData.tags.length > 0}
          <span style="display:inline-flex;gap:4px;align-items:center;">
            标签：
            {#each previewData.tags as tag}
              <span class="sbadge sb-gray" style="font-size:10px;">{tag}</span>
            {/each}
          </span>
        {/if}
      </div>

      <!-- 正文内容区 -->
      <div class="reading-body post-content" style="padding:16px;background:var(--color-bg-page);border-radius:var(--radius-md, 6px);border:1px solid var(--color-border);line-height:1.7;min-height:120px;">
        {#if previewData.body_html}
          <SafeHtml html={previewData.body_html} />
        {:else}
          <div style="color:var(--color-text-tertiary);font-style:italic;text-align:center;padding:32px 0;">（该帖子暂无正文或内容为空）</div>
        {/if}
      </div>
    {/if}
  </div>

  {#snippet footer()}
    <div style="display:flex;justify-content:space-between;align-items:center;width:100%;">
      <div>
        {#if previewTarget}
          <a
            class="btn secondary sm"
            href={`/posts/${previewTarget.id}`}
            target="_blank"
            rel="noopener noreferrer"
            style="display:inline-flex;align-items:center;gap:6px;"
          >
            <Icon name="external-link" size={14} />
            <span>在新窗口查看原帖</span>
          </a>
        {/if}
      </div>
      <div style="display:flex;gap:8px;">
        {#if previewTarget && previewTarget.status === 'pending_review'}
          <button
            type="button"
            class="btn primary sm"
            onclick={() => {
              const t = previewTarget;
              closePreview();
              if (t) openOps(t, 'approve');
            }}
          >
            审核通过
          </button>
          <button
            type="button"
            class="btn danger sm"
            onclick={() => {
              const t = previewTarget;
              closePreview();
              if (t) openOps(t, 'reject');
            }}
          >
            驳回
          </button>
        {/if}
        <button type="button" class="btn secondary sm" onclick={closePreview}>
          关闭
        </button>
      </div>
    </div>
  {/snippet}
</Dialog>
