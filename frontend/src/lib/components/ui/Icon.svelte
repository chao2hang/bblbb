<script lang="ts">
  import { parseIconNodes } from './icons';

  let {
    name,
    size = 16,
    class: klass = ''
  }: { name: string; size?: number; class?: string } = $props();

  // M04-MARKDOWN-08：{@html} 仅 SafeHtml 可用；图标经结构化节点渲染，
  // 保持 SVG 命名空间（compile-time 已知 SVG 标签）。
  const nodes = $derived(parseIconNodes(name));
</script>

<svg
  class="icon icon-{name} {klass}"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  {#each nodes as node}
    {#if node.tag === 'path'}
      <path {...node.attrs} />
    {:else if node.tag === 'circle'}
      <circle {...node.attrs} />
    {:else if node.tag === 'rect'}
      <rect {...node.attrs} />
    {:else if node.tag === 'line'}
      <line {...node.attrs} />
    {:else if node.tag === 'polyline'}
      <polyline {...node.attrs} />
    {:else if node.tag === 'polygon'}
      <polygon {...node.attrs} />
    {/if}
  {/each}
</svg>
