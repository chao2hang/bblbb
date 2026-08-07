// M09-UI-04：Markdown diff 预览——纯文本行级 LCS diff。
//
// AI 格式化/SEO 建议只以 diff 预览呈现，绝不直接改写正文
// （docs/AI.md §4「用户必须手动采纳」）。本模块输出纯文本 diff 行
// （-/+ 前缀），前端按文本插值渲染，绝不注入 HTML/脚本。
export type DiffLineType = 'same' | 'removed' | 'added';

export interface DiffLine {
  type: DiffLineType;
  text: string;
}

/** 行级 LCS diff（朴素 DP；输入为拆分后的行数组）。 */
export function lineDiff(original: string[], proposed: string[]): DiffLine[] {
  const n = original.length;
  const m = proposed.length;
  // dp[i][j] = LCS 长度
  const dp: number[][] = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] = original[i] === proposed[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }
  const out: DiffLine[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (original[i] === proposed[j]) {
      out.push({ type: 'same', text: original[i] });
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      out.push({ type: 'removed', text: original[i] });
      i++;
    } else {
      out.push({ type: 'added', text: proposed[j] });
      j++;
    }
  }
  while (i < n) {
    out.push({ type: 'removed', text: original[i] });
    i++;
  }
  while (j < m) {
    out.push({ type: 'added', text: proposed[j] });
    j++;
  }
  return out;
}

/** 把 Markdown 文本切分为 diff 行（空串 → 单行）。 */
export function splitLines(text: string): string[] {
  const lines = text.split(/\r?\n/);
  return lines.length === 0 ? [''] : lines;
}

/** 文本 diff（返回 -/+ 前缀的纯文本行，供 <pre> 渲染）。 */
export function renderTextDiff(original: string, proposed: string): DiffLine[] {
  return lineDiff(splitLines(original), splitLines(proposed));
}

/** 简单启发：两段文本是否不同（避免无意义的空 diff）。 */
export function hasDiff(original: string | null | undefined, proposed: string | null | undefined): boolean {
  return original !== proposed;
}
