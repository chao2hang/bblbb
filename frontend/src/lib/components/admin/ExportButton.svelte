<script lang="ts">
  // GAP-FIX（M17-GAPFIX-07·组件封装）：客户端导出按钮（CSV/JSON）。
  // 统一各页「导出」实现：CSV 带 UTF-8 BOM（Excel 兼容），JSON 含导出时间。
  import { show as showToast } from '$lib/ui/toast';

  let {
    label = '导出',
    filename = 'export',
    format = 'csv',
    columns = [],
    getData,
    variant = 'ghost'
  }: {
    label?: string;
    filename?: string;
    format?: 'csv' | 'json';
    /** CSV 列定义（key 取值、label 为表头）；JSON 格式忽略。 */
    columns?: Array<{ key: string; label: string }>;
    /** 返回当前要导出的行（客户端点击时求值）。 */
    getData: () => Array<Record<string, unknown>>;
    variant?: 'ghost' | 'secondary';
  } = $props();

  function download(): void {
    const rows = getData();
    let blob: Blob;
    if (format === 'json') {
      blob = new Blob([JSON.stringify({ exported_at: new Date().toISOString(), items: rows }, null, 2)], {
        type: 'application/json'
      });
    } else {
      const header = columns.map((c) => `"${c.label}"`).join(',');
      const lines = rows.map((r) =>
        columns.map((c) => `"${String(r[c.key] ?? '').replace(/"/g, '""')}"`).join(',')
      );
      blob = new Blob(['\uFEFF' + [header, ...lines].join('\n')], { type: 'text/csv;charset=utf-8' });
    }
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.download = `${filename}-${new Date().toISOString().slice(0, 10)}.${format}`;
    a.href = url;
    a.click();
    URL.revokeObjectURL(url);
    showToast('已导出', 'success');
  }
</script>

<button type="button" class="btn {variant} btn-{variant} sm" onclick={download}>{label}</button>
