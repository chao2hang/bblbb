<script lang="ts">
  // M18-ADMIN-BATCH-03：通用批量操作工具条（「N 项已选 + 批量操作按钮 + 取消选择」）。
  //
  // 使用契约：
  // - count > 0 时渲染，否则不渲染任何内容（SSR 基线：未选中时无多余 DOM）；
  // - 批量按钮由页面通过 children 注入（Button variant="secondary"/"danger" size="sm"，
  //   onclick 打开本页的批量 Dialog 填参数后提交批量表单）；
  // - onclear 由页面清空选择状态。
  import type { Snippet } from 'svelte';

  let {
    count,
    noun = '项',
    onclear,
    children
  }: {
    /** 当前选中数量（0 时不渲染）。 */
    count: number;
    /** 计数名词（默认「项」，可传「个用户」等）。 */
    noun?: string;
    onclear: () => void;
    children?: Snippet;
  } = $props();
</script>

{#if count > 0}
  <div class="admin-batch-bar" role="region" aria-label="批量操作" aria-live="polite">
    <span class="admin-batch-bar__count">已选 {count} {noun}</span>
    <div class="admin-batch-bar__actions">
      {@render children?.()}
      <button type="button" class="admin-batch-bar__clear" onclick={onclear}>取消选择</button>
    </div>
  </div>
{/if}
