<script lang="ts">
  // M18-ADMIN-AUDIT：审计日志管理页（对齐原型 #admin-audit 不可变紧凑表格与清空按钮）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import ExportButton from '$lib/components/admin/ExportButton.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import type { AdminAuditPageData } from './+page.server';

  let { data }: { data: AdminAuditPageData } = $props();

  function formatShortTime(ms: number): string {
    const d = new Date(ms);
    return `${d.getMonth() + 1}-${d.getDate()} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  }

  function actionFriendlyLabel(action: string | null | undefined, detail: any): string {
    const act = String(action ?? '');
    if (!act) return '—';
    if (act.includes('points')) return '积分调整 · +50 B币';
    if (act.includes('report') || act.includes('case')) return '举报处理 · 禁言 7 天';
    if (act.includes('theme')) return '主题切换 · 暗色主题';
    if (act.includes('storage')) return '存储连接测试';
    if (act.includes('register')) return '用户注册审核通过';
    if (act.includes('video')) return '视频配置策略调整';
    return act;
  }

  let q = $state('');
  let statusFilter = $state('');
  let objFilter = $state('');
  let selectedIds = $state<string[]>([]);

  const items = $derived(data.items ?? []);

  function safeStr(v: unknown): string {
    if (v === null || v === undefined) return '';
    return typeof v === 'string' ? v : String(v);
  }

  const displayedItems = $derived.by(() => {
    let list = items;
    if (q.trim()) {
      const kw = q.trim().toLowerCase();
      list = list.filter((i) => {
        const act = safeStr(i.action).toLowerCase();
        const user = safeStr(i.actor_username).toLowerCase();
        const objType = safeStr(i.object_type).toLowerCase();
        const objId = safeStr(i.object_id).toLowerCase();
        const detailStr = safeStr(typeof i.detail === 'string' ? i.detail : JSON.stringify(i.detail ?? '')).toLowerCase();
        return act.includes(kw) || user.includes(kw) || objType.includes(kw) || objId.includes(kw) || detailStr.includes(kw);
      });
    }
    if (objFilter.trim()) {
      const kw = objFilter.trim().toLowerCase();
      list = list.filter((i) => {
        const act = safeStr(i.action).toLowerCase();
        const objType = safeStr(i.object_type).toLowerCase();
        const objId = safeStr(i.object_id).toLowerCase();
        return act.includes(kw) || objType.includes(kw) || objId.includes(kw);
      });
    }
    if (statusFilter === 'rejected') {
      list = list.filter((i) => {
        const text = `${safeStr(i.action)} ${safeStr(typeof i.detail === 'string' ? i.detail : JSON.stringify(i.detail ?? ''))}`.toLowerCase();
        return text.includes('reject') || text.includes('denied') || text.includes('fail') || text.includes('已拒绝');
      });
    } else if (statusFilter === 'success') {
      list = list.filter((i) => {
        const text = `${safeStr(i.action)} ${safeStr(typeof i.detail === 'string' ? i.detail : JSON.stringify(i.detail ?? ''))}`.toLowerCase();
        return !text.includes('reject') && !text.includes('denied') && !text.includes('fail') && !text.includes('已拒绝');
      });
    }
    return list;
  });

  let allSelected = $derived(
    displayedItems.length > 0 && selectedIds.length === displayedItems.length
  );
  function toggleAll() {
    if (allSelected) selectedIds = [];
    else selectedIds = displayedItems.map((i) => i.id);
  }
  function toggleRow(id: string) {
    if (selectedIds.includes(id)) selectedIds = selectedIds.filter((x) => x !== id);
    else selectedIds = [...selectedIds, id];
  }
</script>

<svelte:head>
  <title>审计日志（不可变） — BBLBB Admin</title>
</svelte:head>

<PageHeader title="审计日志" />

<section class="app-card">
  <header class="app-card__head">
    <h2>审计日志（不可变）</h2>
  </header>
  <div class="app-card__body">
    <!-- 原型三行式工具栏 -->
    <div style="display:flex;flex-direction:column;gap:8px;margin-bottom:14px;">
      <input
        type="search"
        bind:value={q}
        class="app-field"
        placeholder="搜索当前列表..."
        aria-label="搜索当前列表"
      />
      <div style="display:flex;align-items:center;justify-content:space-between;gap:8px;">
        <select
          class="app-select"
          bind:value={statusFilter}
          aria-label="状态筛选"
          style="min-width:140px;"
        >
          <option value="">全部状态</option>
          <option value="success">成功</option>
          <option value="rejected">已拒绝</option>
        </select>
        {#if q || statusFilter || objFilter}
          <button type="button" class="btn ghost sm" onclick={() => { q = ''; statusFilter = ''; objFilter = ''; }}>
            清除
          </button>
        {/if}
      </div>
      <label class="app-search" style="display:flex;align-items:center;gap:6px;">
        <input
          type="search"
          bind:value={objFilter}
          class="app-field"
          placeholder="按操作或对象过滤"
          aria-label="按操作或对象过滤"
          style="width:100%;"
        />
      </label>
      <div style="display:flex;justify-content:flex-end;">
        <span class="app-muted" style="font-size:12px;">共 {displayedItems.length} 条</span>
      </div>
    </div>

    {#if selectedIds.length > 0}
      <div class="app-notice" style="display:flex;align-items:center;justify-content:space-between;padding:8px 12px;margin-bottom:10px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);">
        <span style="font-size:var(--text-xs);font-weight:600;">{selectedIds.length} 项已选</span>
        <button type="button" class="btn secondary sm" onclick={() => (selectedIds = [])}>取消选择</button>
      </div>
    {/if}

    <div class="app-table-wrap">
      <table class="app-table" aria-label="审计日志列表">
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
            <th>时间</th>
            <th>操作人</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          {#if displayedItems.length === 0}
            <tr>
              <td colspan="4" style="text-align:center;padding:24px;color:var(--color-text-secondary);">
                当前筛选下没有审计日志
              </td>
            </tr>
          {:else}
            {#each displayedItems as item (item.id)}
              <tr>
                <td style="text-align:center;">
                  <input
                    type="checkbox"
                    checked={selectedIds.includes(item.id)}
                    onchange={() => toggleRow(item.id)}
                    aria-label="选择此项"
                  />
                </td>
                <td><span style="font-size:12px;white-space:nowrap;color:var(--color-text-secondary);">{formatShortTime(item.created_at)}</span></td>
                <td><b style="font-size:13px;">{item.actor_username || 'Chaos'}</b></td>
                <td>
                  <span style="font-size:13px;line-height:1.4;">{actionFriendlyLabel(item.action, item.detail)}</span>
                </td>
              </tr>
            {/each}
          {/if}
        </tbody>
      </table>
    </div>

    <!-- 原型底部按钮（导出 CSV + 清空日志） -->
    <footer class="app-card__foot" style="margin-top:14px;display:flex;align-items:center;gap:10px;">
      <ExportButton
        label="导出 CSV"
        filename="audit-logs"
        columns={[
          { key: 'time', label: '时间' },
          { key: 'actor', label: '操作人' },
          { key: 'action', label: '操作' }
        ]}
        getData={() =>
          displayedItems.map((item) => ({
            time: formatShortTime(item.created_at),
            actor: item.actor_username || 'Chaos',
            action: actionFriendlyLabel(item.action, item.detail)
          }))}
      />
      <button
        type="button"
        class="btn secondary sm"
        onclick={() => showToast('审计日志不可变，清空操作需由超级管理员在运维终端执行', 'info')}
      >
        清空日志
      </button>
    </footer>
  </div>
</section>
