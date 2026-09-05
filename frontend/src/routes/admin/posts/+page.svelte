<script lang="ts">
  // GAP-FIX（管理域·帖子管理）：1:1 对齐原型 prototype/pages/admin-posts.html
  // - 顶部 .app-filter-tabs 状态筛选 Tab 链接（全部 / 待审核 / 公开 / 精华 / 已隐藏 / 已删除）
  // - .app-toolbar 搜索条
  // - .app-card > .app-card__head + .app-table 数据表格
  // - 简洁一键式操作按钮（td.adm-acts）
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import ExportButton from '$lib/components/admin/ExportButton.svelte';
  import FilterTabs from '$lib/components/admin/FilterTabs.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { adminStateLabel } from '$lib/admin';
  import { show as showToast } from '$lib/ui/toast';
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

  function searchAction(): string {
    const params = new URLSearchParams();
    if (data.status) params.set('status', data.status);
    return params.toString() ? `/admin/posts?${params.toString()}` : '/admin/posts';
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

  // M18：复选框与批量选择状态（对齐原型后台表格）
  let selectedIds = $state<string[]>([]);
  let allSelected = $derived(
    data.items && data.items.length > 0 && selectedIds.length === data.items.length
  );
  function toggleAll() {
    if (allSelected) {
      selectedIds = [];
    } else {
      selectedIds = (data.items ?? []).map((i) => i.id);
    }
  }
  function toggleRow(id: string) {
    if (selectedIds.includes(id)) {
      selectedIds = selectedIds.filter((x) => x !== id);
    } else {
      selectedIds = [...selectedIds, id];
    }
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
  {#if message}
    <p class="input-hint {conflict ? 'is-error' : ''}" role="status">{message}</p>
  {/if}
  {#if conflict}
    <p class="input-hint is-error" role="alert">帖子状态已变化，请刷新后重试。</p>
  {/if}

  <section class="app-card">
    <header class="app-card__head">
      <h2>帖子列表</h2>
    </header>

    <div class="app-card__body">
      <!-- 原型对齐工具条（搜索当前列表 + 全部状态下拉 + 清除） -->
      <form method="GET" action="/admin/posts" style="display:flex;flex-direction:column;gap:8px;margin-bottom:14px;">
        <input
          type="search"
          name="q"
          value={data.q}
          class="app-field"
          placeholder="搜索当前列表..."
          aria-label="搜索当前列表"
        />
        <div style="display:flex;align-items:center;justify-content:space-between;gap:8px;">
          <select
            name="status"
            class="app-select"
            value={data.status}
            aria-label="状态筛选"
            onchange={(e) => (e.currentTarget.form?.submit())}
            style="min-width:140px;"
          >
            <option value="">全部状态</option>
            {#each POST_STATUS_TABS.slice(1) as tab}
              <option value={tab.value}>{tab.label}</option>
            {/each}
          </select>
          {#if data.q || data.status}
            <a href="/admin/posts" class="text-link" style="font-size:var(--text-sm);">清除</a>
          {/if}
        </div>
      </form>

      {#if selectedIds.length > 0}
        <!-- 原型批量操作栏 -->
        <div class="app-notice" style="display:flex;align-items:center;justify-content:space-between;padding:8px 12px;margin-bottom:10px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);">
          <span style="font-size:var(--text-xs);font-weight:600;">{selectedIds.length} 项已选</span>
          <div style="display:flex;gap:6px;">
            <button type="button" class="btn secondary sm" onclick={() => (selectedIds = [])}>取消选择</button>
          </div>
        </div>
      {/if}

      {#if !data.items || data.items.length === 0}
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
                <th style="min-width:180px;">操作</th>
              </tr>
            </thead>
            <tbody>
              {#each data.items as item (item.id)}
                {@const badge = statusBadgeInfo(item)}
                <tr>
                  <td style="text-align:center;">
                    <input
                      type="checkbox"
                      checked={selectedIds.includes(item.id)}
                      onchange={() => toggleRow(item.id)}
                      aria-label="选择此项"
                    />
                  </td>
                  <td>
                    <b>{item.title || '（无标题）'}</b>
                    {#if item.status === 'hidden'}<span class="text-secondary" style="font-size:11px;margin-left:6px;">已被隐藏</span>{/if}
                    <span class="sub" style="display:block;margin-top:3px;">
                      <a class="app-link" href="/posts/{item.id}" target="_blank" style="font-size:11px;">
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
                    {#if item.status === 'pending_review'}
                      <!-- 待审核：通过 / 驳回 -->
                      <form method="POST" action="?/moderate" use:enhance style="display:inline-flex;margin:0;">
                        <input type="hidden" name="id" value={item.id} />
                        <input type="hidden" name="action" value="approve" />
                        <input type="hidden" name="reason" value="审核通过" />
                        <button type="submit" class="btn primary sm">通过</button>
                      </form>
                      <form method="POST" action="?/moderate" use:enhance style="display:inline-flex;margin:0;">
                        <input type="hidden" name="id" value={item.id} />
                        <input type="hidden" name="action" value="reject" />
                        <input type="hidden" name="reason" value="违规驳回" />
                        <button type="submit" class="btn danger sm">驳回</button>
                      </form>
                    {:else if item.status === 'published'}
                      <!-- 公开：加精/取消精华 + 隐藏 -->
                      <form method="POST" action="?/moderate" use:enhance style="display:inline-flex;margin:0;">
                        <input type="hidden" name="id" value={item.id} />
                        <input type="hidden" name="action" value={item.is_featured ? 'unfeature' : 'feature'} />
                        <input type="hidden" name="reason" value={item.is_featured ? '取消加精' : '设为精华'} />
                        <button type="submit" class="btn ghost sm">
                          {item.is_featured ? '取消精华' : '设为精华'}
                        </button>
                      </form>
                      <form method="POST" action="?/moderate" use:enhance style="display:inline-flex;margin:0;">
                        <input type="hidden" name="id" value={item.id} />
                        <input type="hidden" name="action" value="hide" />
                        <input type="hidden" name="reason" value="管理隐藏" />
                        <button type="submit" class="btn ghost sm">隐藏</button>
                      </form>
                    {:else if item.status === 'hidden'}
                      <!-- 已隐藏：恢复 / 删除 -->
                      <form method="POST" action="?/moderate" use:enhance style="display:inline-flex;margin:0;">
                        <input type="hidden" name="id" value={item.id} />
                        <input type="hidden" name="action" value="restore" />
                        <input type="hidden" name="reason" value="恢复展示" />
                        <button type="submit" class="btn ghost sm">恢复</button>
                      </form>
                      <form method="POST" action="?/moderate" use:enhance style="display:inline-flex;margin:0;">
                        <input type="hidden" name="id" value={item.id} />
                        <input type="hidden" name="action" value="delete" />
                        <input type="hidden" name="reason" value="彻底删除" />
                        <button type="submit" class="btn danger sm">删除</button>
                      </form>
                    {:else if item.status === 'deleted'}
                      <!-- 已删除：恢复 -->
                      <form method="POST" action="?/moderate" use:enhance style="display:inline-flex;margin:0;">
                        <input type="hidden" name="id" value={item.id} />
                        <input type="hidden" name="action" value="restore" />
                        <input type="hidden" name="reason" value="恢复展示" />
                        <button type="submit" class="btn ghost sm">恢复</button>
                      </form>
                    {:else}
                      <!-- 草稿或其他：通过 / 隐藏 -->
                      <form method="POST" action="?/moderate" use:enhance style="display:inline-flex;margin:0;">
                        <input type="hidden" name="id" value={item.id} />
                        <input type="hidden" name="action" value="approve" />
                        <input type="hidden" name="reason" value="审核通过" />
                        <button type="submit" class="btn primary sm">发布</button>
                      </form>
                      <form method="POST" action="?/moderate" use:enhance style="display:inline-flex;margin:0;">
                        <input type="hidden" name="id" value={item.id} />
                        <input type="hidden" name="action" value="hide" />
                        <input type="hidden" name="reason" value="管理隐藏" />
                        <button type="submit" class="btn ghost sm">隐藏</button>
                      </form>
                    {/if}
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
