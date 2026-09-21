/**
 * Git-style text diff algorithm with line-level and intra-line (word-level) highlighting.
 * Supports Unified and Split (side-by-side) presentations.
 */

export type DiffType = 'same' | 'added' | 'removed';

export interface DiffSpan {
  type: DiffType;
  text: string;
}

export interface GitDiffUnifiedLine {
  type: DiffType;
  oldLineNumber: number | null;
  newLineNumber: number | null;
  prefix: '+' | '-' | ' ';
  spans: DiffSpan[];
}

export interface GitDiffSplitSide {
  lineNumber: number | null;
  type: DiffType | 'empty';
  spans: DiffSpan[];
}

export interface GitDiffSplitRow {
  left: GitDiffSplitSide;
  right: GitDiffSplitSide;
}

export interface GitDiffResult {
  additions: number;
  deletions: number;
  changes: number;
  isNew: boolean;
  unified: GitDiffUnifiedLine[];
  split: GitDiffSplitRow[];
}

/** Tokenize string into words, characters, punctuation, and whitespaces for fine-grained diff */
export function tokenizeWord(text: string): string[] {
  if (!text) return [];
  // Match chinese characters individually, english alphanumeric words as clusters, whitespace, or single punctuation
  const matches = text.match(/[\p{Unified_Ideograph}]|[a-zA-Z0-9_]+|\s+|[^\s\w\p{Unified_Ideograph}]/gu);
  return matches ?? [text];
}

/** Generic LCS algorithm between two arrays of items */
function computeLCS<T>(a: T[], b: T[], equals: (x: T, y: T) => boolean = (x, y) => x === y): boolean[][] {
  const n = a.length;
  const m = b.length;
  const dp: number[][] = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0));

  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] = equals(a[i], b[j]) ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }

  // matrix of whether (i, j) is part of the common subsequence
  const inLcsA = new Array<boolean>(n).fill(false);
  const inLcsB = new Array<boolean>(m).fill(false);
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (equals(a[i], b[j])) {
      inLcsA[i] = true;
      inLcsB[j] = true;
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      i++;
    } else {
      j++;
    }
  }
  return [inLcsA, inLcsB];
}

/** Compute word-level spans for a modified line pair */
function computeWordDiff(beforeText: string, afterText: string): { beforeSpans: DiffSpan[]; afterSpans: DiffSpan[] } {
  const beforeTokens = tokenizeWord(beforeText);
  const afterTokens = tokenizeWord(afterText);
  const [inLcsBefore, inLcsAfter] = computeLCS(beforeTokens, afterTokens);

  const beforeSpans: DiffSpan[] = [];
  for (let i = 0; i < beforeTokens.length; i++) {
    const text = beforeTokens[i];
    const isSame = inLcsBefore[i];
    const type: DiffType = isSame ? 'same' : 'removed';
    if (beforeSpans.length > 0 && beforeSpans[beforeSpans.length - 1].type === type) {
      beforeSpans[beforeSpans.length - 1].text += text;
    } else {
      beforeSpans.push({ type, text });
    }
  }

  const afterSpans: DiffSpan[] = [];
  for (let j = 0; j < afterTokens.length; j++) {
    const text = afterTokens[j];
    const isSame = inLcsAfter[j];
    const type: DiffType = isSame ? 'same' : 'added';
    if (afterSpans.length > 0 && afterSpans[afterSpans.length - 1].type === type) {
      afterSpans[afterSpans.length - 1].text += text;
    } else {
      afterSpans.push({ type, text });
    }
  }

  return { beforeSpans, afterSpans };
}

/** Split text into lines, handling CRLF and edge cases */
function splitLines(text: string): string[] {
  if (text === '') return [''];
  const lines = text.split(/\r?\n/);
  return lines;
}

/**
 * Main Git diff calculation function
 * Takes before text (or null for new files) and after text.
 */
export function computeGitDiff(before: string | null, after: string): GitDiffResult {
  const isNew = before === null;
  const afterLines = splitLines(after);

  if (isNew) {
    const unified: GitDiffUnifiedLine[] = afterLines.map((line, idx) => ({
      type: 'added',
      oldLineNumber: null,
      newLineNumber: idx + 1,
      prefix: '+',
      spans: [{ type: 'added', text: line }]
    }));

    const split: GitDiffSplitRow[] = afterLines.map((line, idx) => ({
      left: { lineNumber: null, type: 'empty', spans: [] },
      right: { lineNumber: idx + 1, type: 'added', spans: [{ type: 'added', text: line }] }
    }));

    return {
      additions: afterLines.length,
      deletions: 0,
      changes: afterLines.length,
      isNew: true,
      unified,
      split
    };
  }

  const beforeLines = splitLines(before);
  const [inLcsBefore, inLcsAfter] = computeLCS(beforeLines, afterLines);

  // Group into raw diff chunks
  interface RawLine {
    type: DiffType;
    text: string;
    oldLine?: number;
    newLine?: number;
  }

  const rawLines: RawLine[] = [];
  let i = 0;
  let j = 0;
  let oldLineCounter = 1;
  let newLineCounter = 1;

  while (i < beforeLines.length || j < afterLines.length) {
    if (i < beforeLines.length && j < afterLines.length && inLcsBefore[i] && inLcsAfter[j]) {
      rawLines.push({
        type: 'same',
        text: beforeLines[i],
        oldLine: oldLineCounter++,
        newLine: newLineCounter++
      });
      i++;
      j++;
    } else if (i < beforeLines.length && !inLcsBefore[i]) {
      rawLines.push({
        type: 'removed',
        text: beforeLines[i],
        oldLine: oldLineCounter++
      });
      i++;
    } else if (j < afterLines.length && !inLcsAfter[j]) {
      rawLines.push({
        type: 'added',
        text: afterLines[j],
        newLine: newLineCounter++
      });
      j++;
    } else {
      // Fallback
      if (i < beforeLines.length) {
        rawLines.push({ type: 'removed', text: beforeLines[i], oldLine: oldLineCounter++ });
        i++;
      }
      if (j < afterLines.length) {
        rawLines.push({ type: 'added', text: afterLines[j], newLine: newLineCounter++ });
        j++;
      }
    }
  }

  // Calculate statistics
  let additions = 0;
  let deletions = 0;
  for (const line of rawLines) {
    if (line.type === 'added') additions++;
    if (line.type === 'removed') deletions++;
  }

  // Match consecutive removed and added blocks for word-level diff
  const unified: GitDiffUnifiedLine[] = [];
  const split: GitDiffSplitRow[] = [];

  let idx = 0;
  while (idx < rawLines.length) {
    const curr = rawLines[idx];
    if (curr.type === 'same') {
      const spans: DiffSpan[] = [{ type: 'same', text: curr.text }];
      unified.push({
        type: 'same',
        oldLineNumber: curr.oldLine ?? null,
        newLineNumber: curr.newLine ?? null,
        prefix: ' ',
        spans
      });
      split.push({
        left: { lineNumber: curr.oldLine ?? null, type: 'same', spans },
        right: { lineNumber: curr.newLine ?? null, type: 'same', spans }
      });
      idx++;
    } else {
      // Collect all consecutive removed lines followed by added lines
      const removedGroup: RawLine[] = [];
      while (idx < rawLines.length && rawLines[idx].type === 'removed') {
        removedGroup.push(rawLines[idx]);
        idx++;
      }
      const addedGroup: RawLine[] = [];
      while (idx < rawLines.length && rawLines[idx].type === 'added') {
        addedGroup.push(rawLines[idx]);
        idx++;
      }

      // If both removed and added exist, perform word-level diff pairing
      const maxLen = Math.max(removedGroup.length, addedGroup.length);
      for (let k = 0; k < maxLen; k++) {
        const rem = removedGroup[k];
        const add = addedGroup[k];

        let remSpans: DiffSpan[] = rem ? [{ type: 'removed', text: rem.text }] : [];
        let addSpans: DiffSpan[] = add ? [{ type: 'added', text: add.text }] : [];

        if (rem && add) {
          // Word-level diff
          const diffed = computeWordDiff(rem.text, add.text);
          remSpans = diffed.beforeSpans;
          addSpans = diffed.afterSpans;
        }

        if (rem) {
          unified.push({
            type: 'removed',
            oldLineNumber: rem.oldLine ?? null,
            newLineNumber: null,
            prefix: '-',
            spans: remSpans
          });
        }
        if (add) {
          unified.push({
            type: 'added',
            oldLineNumber: null,
            newLineNumber: add.newLine ?? null,
            prefix: '+',
            spans: addSpans
          });
        }

        split.push({
          left: rem
            ? { lineNumber: rem.oldLine ?? null, type: 'removed', spans: remSpans }
            : { lineNumber: null, type: 'empty', spans: [] },
          right: add
            ? { lineNumber: add.newLine ?? null, type: 'added', spans: addSpans }
            : { lineNumber: null, type: 'empty', spans: [] }
        });
      }
    }
  }

  return {
    additions,
    deletions,
    changes: additions + deletions,
    isNew: false,
    unified,
    split
  };
}
