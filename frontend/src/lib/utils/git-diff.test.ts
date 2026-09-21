import { describe, expect, it } from 'vitest';
import { computeGitDiff, tokenizeWord } from './git-diff';

describe('git-diff utility', () => {
  it('tokenizes mixed Chinese and English correctly', () => {
    const tokens = tokenizeWord('正文第一版：SSR、SEO与表单。');
    expect(tokens).toContain('正');
    expect(tokens).toContain('SSR');
    expect(tokens).toContain('表');
    expect(tokens).toContain('与');
  });

  it('handles first submission (new file) correctly', () => {
    const after = 'Line 1\nLine 2';
    const res = computeGitDiff(null, after);
    expect(res.isNew).toBe(true);
    expect(res.additions).toBe(2);
    expect(res.deletions).toBe(0);
    expect(res.unified).toHaveLength(2);
    expect(res.unified[0].prefix).toBe('+');
    expect(res.unified[0].newLineNumber).toBe(1);
    expect(res.unified[0].oldLineNumber).toBeNull();
  });

  it('handles simple line replacement with word-level highlight', () => {
    const before = 'Hello World\nLine two unchanged';
    const after = 'Hello Developer World\nLine two unchanged';
    const res = computeGitDiff(before, after);

    expect(res.isNew).toBe(false);
    expect(res.additions).toBe(1);
    expect(res.deletions).toBe(1);
    expect(res.changes).toBe(2);

    // Check word-level highlight on the added line
    const addedLine = res.unified.find((l) => l.type === 'added');
    expect(addedLine).toBeDefined();
    const addedSpan = addedLine?.spans.find((s) => s.type === 'added');
    expect(addedSpan?.text).toContain('Developer');

    // Same line preserved
    const sameLine = res.unified.find((l) => l.type === 'same');
    expect(sameLine?.spans[0].text).toBe('Line two unchanged');
  });

  it('handles deletion and additions across multiple lines', () => {
    const before = 'Alpha\nBeta\nGamma';
    const after = 'Alpha\nBeta Modified\nDelta\nGamma';
    const res = computeGitDiff(before, after);

    expect(res.additions).toBe(2); // Beta Modified, Delta
    expect(res.deletions).toBe(1); // Beta
    expect(res.split.length).toBeGreaterThanOrEqual(3);
  });
});
