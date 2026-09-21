<script lang="ts">
  import { computeGitDiff, type GitDiffResult } from '$lib/utils/git-diff';
  import Icon from '$lib/components/ui/Icon.svelte';

  let {
    beforeBody = null,
    afterBody,
    fromVersion = null,
    toVersion,
    reason = null,
    defaultMode = 'split'
  }: {
    beforeBody: string | null;
    afterBody: string;
    fromVersion: number | null;
    toVersion: number;
    reason?: string | null;
    defaultMode?: 'split' | 'unified';
  } = $props();

  let mode = $state<'split' | 'unified'>('split');
  $effect(() => {
    mode = defaultMode;
  });

  const diffResult: GitDiffResult = $derived(computeGitDiff(beforeBody, afterBody));

  // Compute GitHub-style 5-block diff square colors
  const diffSquares = $derived.by(() => {
    const total = diffResult.changes;
    if (total === 0) return ['neutral', 'neutral', 'neutral', 'neutral', 'neutral'];
    const addRatio = Math.round((diffResult.additions / total) * 5);
    const squares: ('added' | 'removed' | 'neutral')[] = [];
    for (let i = 0; i < 5; i++) {
      if (i < addRatio) {
        squares.push('added');
      } else if (i < (diffResult.additions + diffResult.deletions > 0 ? 5 : 0)) {
        squares.push('removed');
      } else {
        squares.push('neutral');
      }
    }
    return squares;
  });
</script>

<div class="git-diff-container">
  <!-- Diff Toolbar / Header -->
  <div class="git-diff-header">
    <div class="git-diff-header__meta">
      <span class="git-diff-commit-badge">
        <Icon name="git-commit" size={14} />
        {#if fromVersion === null}
          <span class="git-diff-tag is-new">首次提交 · v{toVersion}</span>
        {:else}
          <span class="git-diff-tag">v{fromVersion} → v{toVersion}</span>
        {/if}
      </span>

      <!-- Stat counters: +N -M -->
      <div class="git-diff-stats" title="{diffResult.additions} 行新增，{diffResult.deletions} 行删除">
        <span class="git-diff-count git-diff-count--add">+{diffResult.additions}</span>
        <span class="git-diff-count git-diff-count--del">-{diffResult.deletions}</span>
        <div class="git-diff-squares" aria-hidden="true">
          {#each diffSquares as sq}
            <span class="diff-sq diff-sq--{sq}"></span>
          {/each}
        </div>
      </div>

      {#if reason}
        <span class="git-diff-reason" title="修订说明">
          <Icon name="message-square" size={12} />
          {reason}
        </span>
      {/if}
    </div>

    <!-- Mode Switcher -->
    <div class="git-diff-header__actions">
      <div class="git-diff-modes" role="tablist" aria-label="差异视图模式">
        <button
          type="button"
          class="git-mode-btn"
          class:is-active={mode === 'split'}
          onclick={() => (mode = 'split')}
          aria-label="分栏对比"
          title="分栏对比 (Side-by-side)"
        >
          <Icon name="columns" size={13} />
          <span>分栏</span>
        </button>
        <button
          type="button"
          class="git-mode-btn"
          class:is-active={mode === 'unified'}
          onclick={() => (mode = 'unified')}
          aria-label="统一单栏对比"
          title="统一单栏对比 (Inline)"
        >
          <Icon name="list" size={13} />
          <span>统一</span>
        </button>
      </div>
    </div>
  </div>

  <!-- Diff Viewer Body -->
  <div class="git-diff-body" class:is-split={mode === 'split'} class:is-unified={mode === 'unified'}>
    {#if diffResult.isNew}
      <div class="git-diff-banner is-new">
        <Icon name="file-plus" size={15} />
        <span>新增文章（首次提交），正文全部为新增内容</span>
      </div>
    {:else if diffResult.changes === 0}
      <div class="git-diff-banner is-same">
        <Icon name="info" size={15} />
        <span>两版正文文本内容完全一致</span>
      </div>
    {/if}

    {#if mode === 'unified'}
      <!-- Unified Inline Diff -->
      <div class="git-diff-table-wrap">
        <table class="git-diff-table git-diff-table--unified">
          <colgroup>
            <col class="col-line-no" />
            <col class="col-line-no" />
            <col class="col-prefix" />
            <col class="col-content" />
          </colgroup>
          <tbody>
            {#each diffResult.unified as line, idx (idx)}
              <tr class="diff-line diff-line--{line.type}">
                <td class="diff-gutter diff-gutter--old" aria-hidden="true">
                  {line.oldLineNumber ?? ''}
                </td>
                <td class="diff-gutter diff-gutter--new" aria-hidden="true">
                  {line.newLineNumber ?? ''}
                </td>
                <td class="diff-prefix" aria-hidden="true">{line.prefix}</td>
                <td class="diff-content">
                  <code>{#each line.spans as span}{#if span.type !== 'same'}<span class="diff-word diff-word--{span.type}">{span.text}</span>{:else}{span.text}{/if}{/each}</code>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else}
      <!-- Split Side-by-Side Diff -->
      <div class="git-diff-table-wrap">
        <div class="split-diff-header">
          <div class="split-col-title split-col-title--before">
            修改前 {fromVersion === null ? '（无）' : `· v${fromVersion}`}
          </div>
          <div class="split-col-title split-col-title--after">
            修改后 · v{toVersion}
          </div>
        </div>
        <table class="git-diff-table git-diff-table--split">
          <colgroup>
            <col class="col-line-no" />
            <col class="col-content-half" />
            <col class="col-line-no" />
            <col class="col-content-half" />
          </colgroup>
          <tbody>
            {#each diffResult.split as row, idx (idx)}
              <tr class="split-row">
                <!-- Left: Before -->
                <td class="diff-gutter diff-gutter--{row.left.type}" aria-hidden="true">
                  {row.left.lineNumber ?? ''}
                </td>
                <td class="diff-content diff-content--{row.left.type}">
                  {#if row.left.type !== 'empty'}
                    <code>{#each row.left.spans as span}{#if span.type === 'removed'}<span class="diff-word diff-word--removed">{span.text}</span>{:else}{span.text}{/if}{/each}</code>
                  {/if}
                </td>
                <!-- Right: After -->
                <td class="diff-gutter diff-gutter--{row.right.type}" aria-hidden="true">
                  {row.right.lineNumber ?? ''}
                </td>
                <td class="diff-content diff-content--{row.right.type}">
                  {#if row.right.type !== 'empty'}
                    <code>{#each row.right.spans as span}{#if span.type === 'added'}<span class="diff-word diff-word--added">{span.text}</span>{:else}{span.text}{/if}{/each}</code>
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </div>
</div>

<style>
  .git-diff-container {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg-page);
    overflow: hidden;
    font-size: 12px;
  }

  /* Header */
  .git-diff-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    background: var(--color-bg-subtle);
    border-bottom: 1px solid var(--color-border);
  }

  .git-diff-header__meta {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-3);
  }

  .git-diff-commit-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .git-diff-tag {
    display: inline-block;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    font-family: var(--font-family-mono);
    font-size: 11px;
  }

  .git-diff-tag.is-new {
    background: var(--color-success-soft);
    color: var(--color-success);
    border-color: transparent;
  }

  /* Stats: +N -M */
  .git-diff-stats {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-family: var(--font-family-mono);
    font-size: 11px;
    font-weight: 600;
  }

  .git-diff-count--add {
    color: var(--color-success);
  }

  .git-diff-count--del {
    color: var(--color-danger);
  }

  .git-diff-squares {
    display: inline-flex;
    gap: 2px;
  }

  .diff-sq {
    width: 6px;
    height: 6px;
    border-radius: 1px;
  }

  .diff-sq--added {
    background: var(--color-success);
  }

  .diff-sq--removed {
    background: var(--color-danger);
  }

  .diff-sq--neutral {
    background: var(--color-border);
  }

  .git-diff-reason {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--color-text-secondary);
    font-size: 11px;
    max-width: 260px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Mode Switcher */
  .git-diff-modes {
    display: inline-flex;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    padding: 2px;
    gap: 2px;
  }

  .git-mode-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    font-size: 11px;
    font-weight: 500;
    border: none;
    background: transparent;
    color: var(--color-text-secondary);
    border-radius: calc(var(--radius-sm) - 2px);
    cursor: pointer;
    transition: all var(--duration-fast) ease;
  }

  .git-mode-btn:hover {
    color: var(--color-text-primary);
  }

  .git-mode-btn.is-active {
    background: var(--color-brand);
    color: #fff;
  }

  /* Body & Tables */
  .git-diff-body {
    max-height: 480px;
    overflow-y: auto;
    overflow-x: auto;
    background: var(--color-bg-card);
  }

  .git-diff-banner {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: var(--space-2) var(--space-3);
    font-size: 11px;
    font-weight: 500;
  }

  .git-diff-banner.is-new {
    background: var(--color-success-soft);
    color: var(--color-success);
    border-bottom: 1px solid rgba(46, 160, 67, 0.2);
  }

  .git-diff-banner.is-same {
    background: var(--color-bg-subtle);
    color: var(--color-text-secondary);
    border-bottom: 1px solid var(--color-border);
  }

  .split-diff-header {
    display: grid;
    grid-template-columns: 1fr 1fr;
    background: var(--color-bg-subtle);
    border-bottom: 1px solid var(--color-border);
    font-size: 11px;
    font-weight: 600;
    font-family: var(--font-family-mono);
  }

  .split-col-title {
    padding: 4px 10px;
  }

  .split-col-title--before {
    color: var(--color-danger);
    border-right: 1px solid var(--color-border);
  }

  .split-col-title--after {
    color: var(--color-success);
  }

  .git-diff-table-wrap {
    width: 100%;
    min-width: 600px;
  }

  .git-diff-table {
    width: 100%;
    border-collapse: collapse;
    font-family: var(--font-family-mono);
    line-height: 1.5;
  }

  .col-line-no {
    width: 44px;
  }

  .col-prefix {
    width: 20px;
  }

  .col-content {
    width: auto;
  }

  .col-content-half {
    width: calc(50% - 44px);
  }

  /* Gutters */
  .diff-gutter {
    text-align: right;
    padding: 1px 8px;
    color: var(--color-text-tertiary);
    user-select: none;
    font-size: 11px;
    border-right: 1px solid var(--color-border);
    white-space: nowrap;
    vertical-align: top;
  }

  .diff-prefix {
    text-align: center;
    user-select: none;
    font-weight: bold;
    font-size: 12px;
    vertical-align: top;
    padding: 1px 2px;
  }

  .diff-content {
    padding: 1px 8px;
    vertical-align: top;
    white-space: pre-wrap;
    word-break: break-all;
    font-size: 12px;
  }

  .diff-content code {
    font-family: inherit;
    font-size: inherit;
    background: transparent;
    padding: 0;
    color: inherit;
  }

  /* Line-level diff colors (GitHub-accurate light & dark palette) */
  .diff-line--removed,
  .diff-gutter--removed,
  .diff-content--removed {
    background-color: rgba(248, 81, 73, 0.12);
  }

  .diff-line--removed .diff-gutter,
  .diff-gutter--removed {
    color: #cf222e;
    background-color: rgba(248, 81, 73, 0.18);
  }

  .diff-line--removed .diff-prefix {
    color: #cf222e;
  }

  .diff-line--added,
  .diff-gutter--added,
  .diff-content--added {
    background-color: rgba(46, 160, 67, 0.12);
  }

  .diff-line--added .diff-gutter,
  .diff-gutter--added {
    color: #1a7f37;
    background-color: rgba(46, 160, 67, 0.18);
  }

  .diff-line--added .diff-prefix {
    color: #1a7f37;
  }

  .diff-content--empty,
  .diff-gutter--empty {
    background-color: var(--color-bg-subtle);
  }

  /* Word-level highlights (Intra-line Git highlight) */
  .diff-word--removed {
    background-color: rgba(248, 81, 73, 0.35);
    border-radius: 2px;
    padding: 0 1px;
    color: #b31d28;
    text-decoration: line-through;
  }

  .diff-word--added {
    background-color: rgba(46, 160, 67, 0.35);
    border-radius: 2px;
    padding: 0 1px;
    color: #116329;
    font-weight: 500;
  }

  /* Dark mode adjustments */
  :global(.dark) .diff-word--removed {
    color: #ffa198;
    background-color: rgba(248, 81, 73, 0.4);
  }

  :global(.dark) .diff-word--added {
    color: #7ee787;
    background-color: rgba(46, 160, 67, 0.4);
  }

  :global(.dark) .diff-line--removed .diff-gutter,
  :global(.dark) .diff-gutter--removed {
    color: #ff7b72;
  }

  :global(.dark) .diff-line--added .diff-gutter,
  :global(.dark) .diff-gutter--added {
    color: #3fb950;
  }

  :global(.dark) .diff-line--removed .diff-prefix {
    color: #ff7b72;
  }

  :global(.dark) .diff-line--added .diff-prefix {
    color: #3fb950;
  }
</style>
