<script lang="ts">
  // M14-COMPONENTS-01：可访问 Table 基础组件。
  //
  // - 语义 <table> + <caption>（标题供读屏/全体用户理解表格内容）；
  // - 表头 <th scope="col">（列头语义，读屏逐列播报）；
  // - 空白格用 scope="col" 的 th（数据）或 td；
  // - 只接收白名单 prop + children snippet（安全投影，M14-COMPONENTS-06）。
  import type { Snippet } from 'svelte';

  export interface TableColumn {
    /** 表头文案（渲染为 <th scope="col">）。 */
    label: string;
    /** 可选列语义类型。 */
    align?: 'left' | 'right' | 'center';
    /** 可选列宽样式（CSS token 值，白名单内）。 */
    width?: string;
  }

  let {
    caption = '',
    columns = [] as TableColumn[],
    rows = [] as Snippet[][],
    class: klass = '',
    emptyText = '暂无数据',
    children
  }: {
    caption?: string;
    columns?: TableColumn[];
    rows?: Snippet[][];
    class?: string;
    emptyText?: string;
    /** 完全自定义 body（覆盖 rows，仍使用 columns 表头）。 */
    children?: Snippet;
  } = $props();

  const alignClass = (align?: 'left' | 'right' | 'center'): string => {
    if (align === 'right') return 'table-cell-right';
    if (align === 'center') return 'table-cell-center';
    return '';
  };
</script>

<div class="data-table-wrapper {klass}">
  <table class="data-table">
    {#if caption}<caption class="u-visually-hidden">{caption}</caption>{/if}
    <thead>
      <tr>
        {#each columns as column}
          <th scope="col" class={alignClass(column.align)} style={column.width ? `width:${column.width};` : ''}>
            {column.label}
          </th>
        {/each}
      </tr>
    </thead>
    <tbody>
      {#if children}
        {@render children()}
      {:else if rows.length > 0}
        {#each rows as row}
          <tr>
            {#each row as cell}
              <td>{@render cell()}</td>
            {/each}
          </tr>
        {/each}
      {:else}
        <tr>
          <td colspan={columns.length || 1} style="text-align:center;color:var(--color-text-tertiary);">
            {emptyText}
          </td>
        </tr>
      {/if}
    </tbody>
  </table>
</div>
