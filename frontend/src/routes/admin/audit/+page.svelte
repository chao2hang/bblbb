<script lang="ts">
  // M18-ADMIN-AUDIT：审计日志管理页（对齐原型 #admin-audit 不可变紧凑表格与清空按钮）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import ExportButton from '$lib/components/admin/ExportButton.svelte';
    import type { AdminAuditPageData } from './+page.server';

  let { data }: { data: AdminAuditPageData } = $props();

  function formatShortTime(ms: number): string {
    const d = new Date(ms);
    return `${d.getMonth() + 1}-${d.getDate()} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  }

  function actionFriendlyLabel(action: string | null | undefined, detail: unknown): string {
    const act = String(action ?? '').trim();
    if (!act) return '—';
    if (typeof detail === 'string' && detail.trim()) return `${act} · ${detail.trim()}`;
    if (detail && typeof detail === 'object') {
      const serialized = JSON.stringify(detail);
      if (serialized && serialized !== '{}') return `${act} · ${serialized}`;
    }
    return act;
  }

  let q = $state('');
  let statusFilter = $state('');
  let objFilter = $state('');
  // 审计日志不可变（audit_logs 只增不改，无单条/批量写端点）：
  // 不提供行选择与批量操作，批量能力 = 导出当前筛选结果 CSV（ExportButton）。

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
    <!-- 工具栏：单行 flex（窄屏自动换行；修复全宽 select 挤压清除按钮的问题） -->
    <div style="display:flex;align-items:center;flex-wrap:wrap;gap:8px;margin-bottom:14px;">
      <input
        type="search"
        bind:value={q}
        class="app-field"
        placeholder="搜索当前列表..."
        aria-label="搜索当前列表"
        style="flex:1 1 220px;min-width:0;"
      />
      <select
        class="app-select"
        bind:value={statusFilter}
        aria-label="状态筛选"
        style="flex:0 0 auto;width:168px;"
      >
        <option value="">全部状态</option>
        <option value="success">成功</option>
        <option value="rejected">已拒绝</option>
      </select>
      {#if q || statusFilter || objFilter}
        <button type="button" class="btn ghost sm" style="flex:0 0 auto;" onclick={() => { q = ''; statusFilter = ''; objFilter = ''; }}>
          清除
        </button>
      {/if}
      <label class="app-search" style="display:flex;align-items:center;gap:6px;flex:1 1 200px;min-width:0;">
        <input
          type="search"
          bind:value={objFilter}
          class="app-field"
          placeholder="按操作或对象过滤"
          aria-label="按操作或对象过滤"
          style="width:100%;"
        />
      </label>
      <span class="app-muted" style="font-size:12px;flex:0 0 auto;">共 {displayedItems.length} 条</span>
    </div>

    <div class="app-table-wrap">
      <table class="app-table" aria-label="审计日志列表">
        <thead>
          <tr>
            <th>时间</th>
            <th>操作人</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          {#if displayedItems.length === 0}
            <tr>
              <td colspan="3" style="text-align:center;padding:24px;color:var(--color-text-secondary);">
                当前筛选下没有审计日志
              </td>
            </tr>
          {:else}
            {#each displayedItems as item (item.id)}
              <tr>
                <td><span style="font-size:12px;white-space:nowrap;color:var(--color-text-secondary);">{formatShortTime(item.created_at)}</span></td>
                <td><b style="font-size:13px;">{item.actor_username || 'system'}</b></td>
                <td>
                  <span style="font-size:13px;line-height:1.4;">{actionFriendlyLabel(item.action, item.detail)}</span>
                </td>
              </tr>
            {/each}
          {/if}
        </tbody>
      </table>
    </div>

    <nav aria-label="分页" style="display:flex;gap:var(--space-2);align-items:center;margin-top:var(--space-3);">
      {#if data.after}
        <a class="btn btn-secondary btn-sm" href={data.q ? `/admin/audit?q=${encodeURIComponent(data.q)}` : '/admin/audit'}>回到第一页</a>
      {/if}
      {#if data.nextCursor}
        <a class="btn btn-secondary btn-sm" href={`/admin/audit?${data.q ? `q=${encodeURIComponent(data.q)}&` : ''}after=${encodeURIComponent(data.nextCursor)}`}>下一页 →</a>
      {:else if displayedItems.length > 0}
        <span class="text-secondary" style="font-size:var(--text-sm);">没有更多日志了</span>
      {/if}
    </nav>

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
            actor: item.actor_username || 'system',
            action: actionFriendlyLabel(item.action, item.detail)
          }))}
      />
      <button
        type="button"
        class="btn secondary sm"
        disabled
        title="审计日志不可变，清理必须由运维流程执行"
      >
        审计日志不可清空
      </button>
    </footer>
  </div>
</section>
