<script lang="ts">
  // GAP-FIX（M17-GAPFIX-07·组件封装）/ M18-ADMIN-BATCH-02：服务端流式导出与客户端兜底导出。
  // 支持 exportUrl 后端流式全量导出（带鉴权凭据与 UTF-8 BOM）；未提供 exportUrl 时降级走客户端内存导出。
  import { show as showToast } from '$lib/ui/toast';

  let {
    label = '导出',
    filename = 'export',
    format = 'csv',
    columns = [],
    getData,
    exportUrl,
    variant = 'ghost'
  }: {
    label?: string;
    filename?: string;
    format?: 'csv' | 'json';
    /** CSV 列定义（key 取值、label 为表头）；JSON 格式忽略。 */
    columns?: Array<{ key: string; label: string }>;
    /** 返回当前要导出的行（客户端点击时求值）。 */
    getData?: () => Array<Record<string, unknown>>;
    /** 后端流式导出完整或相对 URL（优先直接触发后端流式导出）。 */
    exportUrl?: string;
    variant?: 'ghost' | 'secondary';
  } = $props();

  function download(): void {
    if (exportUrl) {
      const a = document.createElement('a');
      a.href = exportUrl;
      a.download = `${filename}.${format}`;
      a.click();
      showToast('已开始下载导出文件', 'success');
      return;
    }

    if (!getData) return;
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
