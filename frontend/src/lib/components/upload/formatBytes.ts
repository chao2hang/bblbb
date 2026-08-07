// M06-UI-02：字节数格式化（配额/附件大小展示）。

/** 1024 进制字节 → 人类可读（B/KB/MB/GB）。 */
export function formatBytes(bytes: number | null | undefined): string {
  if (typeof bytes !== 'number' || !Number.isFinite(bytes) || bytes < 0) return '—';
  if (bytes < 1024) return `${bytes} B`;
  const units = ['KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let unit = 'B';
  for (const u of units) {
    if (value < 1024) break;
    value /= 1024;
    unit = u;
  }
  const digits = value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(digits)} ${unit}`;
}

/** 进度百分比 0-100（-1 表示不确定进度）。 */
export function progressLabel(percent: number | null | undefined): string {
  if (percent === null || percent === undefined || percent < 0) return '上传中…';
  return `${Math.round(percent)}%`;
}
