<script lang="ts">
  // TablePagination：管理端通用数据表格分页组件。
  // 支持页码切换、每页数量（pageSize）选择、当前条数与总数概览。

  let {
    currentPage = $bindable(1),
    pageSize = $bindable(10),
    totalItems = 0,
    pageSizeOptions = [10, 20, 50],
    noun = '条',
    onchange
  }: {
    currentPage?: number;
    pageSize?: number;
    totalItems: number;
    pageSizeOptions?: number[];
    noun?: string;
    onchange?: (page: number, size: number) => void;
  } = $props();

  const totalPages = $derived(Math.max(1, Math.ceil(totalItems / pageSize)));
  const startItem = $derived(totalItems === 0 ? 0 : (currentPage - 1) * pageSize + 1);
  const endItem = $derived(Math.min(currentPage * pageSize, totalItems));

  $effect(() => {
    if (currentPage > totalPages && totalPages > 0) {
      currentPage = totalPages;
    }
  });

  const pageNumbers = $derived.by(() => {
    if (totalPages <= 7) {
      return Array.from({ length: totalPages }, (_, i) => i + 1);
    }
    if (currentPage <= 4) {
      return [1, 2, 3, 4, 5, '...', totalPages];
    }
    if (currentPage >= totalPages - 3) {
      return [1, '...', totalPages - 4, totalPages - 3, totalPages - 2, totalPages - 1, totalPages];
    }
    return [1, '...', currentPage - 1, currentPage, currentPage + 1, '...', totalPages];
  });

  function setPage(p: number) {
    if (p < 1 || p > totalPages || p === currentPage) return;
    currentPage = p;
    onchange?.(currentPage, pageSize);
  }

  function handleSizeChange(e: Event) {
    const val = Number((e.currentTarget as HTMLSelectElement).value);
    pageSize = val;
    currentPage = 1;
    onchange?.(1, pageSize);
  }
</script>

<footer class="table-pagination" aria-label="表格分页">
  <div class="table-pagination__info">
    <span class="text-secondary" style="font-size:12px;">
      {#if totalItems === 0}
        暂无数据
      {:else}
        显示 <b>{startItem}–{endItem}</b> {noun} · 共 <b>{totalItems}</b> {noun}
        {#if totalPages > 1}
          <span style="margin-left:6px;color:var(--color-text-tertiary);">（第 {currentPage}/{totalPages} 页）</span>
        {/if}
      {/if}
    </span>
  </div>

  {#if totalItems > 0}
    <div class="table-pagination__controls">
      <label class="table-pagination__size">
        <span>每页显示</span>
        <select
          class="app-select"
          value={pageSize}
          onchange={handleSizeChange}
          aria-label="每页显示数量"
        >
          {#each pageSizeOptions as size}
            <option value={size}>{size} {noun}</option>
          {/each}
        </select>
      </label>

      {#if totalPages > 1}
        <div class="table-pagination__pages" role="navigation" aria-label="页码导航">
          <button
            type="button"
            class="page-btn page-btn--nav"
            disabled={currentPage <= 1}
            onclick={() => setPage(currentPage - 1)}
            aria-label="上一页"
          >
            ‹
          </button>
          {#each pageNumbers as p, idx (p === '...' ? `ellipsis-${idx}` : p)}
            {#if p === '...'}
              <span class="page-ellipsis">…</span>
            {:else}
              <button
                type="button"
                class="page-btn"
                class:is-active={p === currentPage}
                aria-current={p === currentPage ? 'page' : undefined}
                onclick={() => typeof p === 'number' && setPage(p)}
              >
                {p}
              </button>
            {/if}
          {/each}
          <button
            type="button"
            class="page-btn page-btn--nav"
            disabled={currentPage >= totalPages}
            onclick={() => setPage(currentPage + 1)}
            aria-label="下一页"
          >
            ›
          </button>
        </div>
      {/if}
    </div>
  {/if}
</footer>

<style>
  .table-pagination {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 12px;
    padding: 12px 16px;
    border-top: 1px solid var(--color-border);
    background: var(--color-bg-card);
  }
  .table-pagination__info {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .table-pagination__controls {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
  }
  .table-pagination__size {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--color-text-secondary);
  }
  .table-pagination__size select {
    height: 28px;
    padding: 1px 6px;
    font-size: 12px;
    width: auto;
    border-radius: var(--radius-sm, 4px);
  }
  .table-pagination__pages {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .page-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 28px;
    height: 28px;
    padding: 0 6px;
    font-size: 12px;
    font-weight: 500;
    color: var(--color-text-secondary);
    background: transparent;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm, 4px);
    cursor: pointer;
    transition: background 0.15s ease, color 0.15s ease, border-color 0.15s ease;
  }
  .page-btn:hover:not(:disabled) {
    color: var(--color-text-primary);
    background: var(--color-bg-subtle);
    border-color: var(--color-border-strong, var(--color-border));
  }
  .page-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }
  .page-btn.is-active {
    color: var(--color-text-on-brand);
    background: var(--color-brand);
    border-color: var(--color-brand);
    font-weight: 600;
  }
  .page-ellipsis {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 20px;
    height: 28px;
    color: var(--color-text-tertiary);
    font-size: 12px;
  }
</style>
