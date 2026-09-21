<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';

  let {
    text = '',
    variant = 'primary',
    size = 'md',
    icon = '',
    href = '',
    disabled = false,
    type = 'button',
    block = false,
    extraClass = '',
    formaction = '',
    formnovalidate = false,
    onclick,
    children
  }: {
    text?: string;
    variant?: 'primary' | 'secondary' | 'ghost' | 'danger';
    size?: 'sm' | 'md' | 'lg';
    icon?: string;
    href?: string;
    disabled?: boolean;
    type?: 'button' | 'submit';
    block?: boolean;
    extraClass?: string;
    /** 原生 formaction（提交到指定 action，如表单内“测试连接”按钮）。 */
    formaction?: string;
    /** 原生 formnovalidate（提交跳过浏览器原生校验，由服务端分区校验）。 */
    formnovalidate?: boolean;
    onclick?: (event: MouseEvent) => void;
    children?: Snippet;
  } = $props();

  const classes = $derived(
    ['btn', variant, `btn-${variant}`, size, `btn-${size}`, block ? 'btn-block' : '', extraClass]
      .filter(Boolean)
      .join(' ')
  );
</script>

{#if href}
  <a href={href} class={classes} aria-disabled={disabled || undefined}>
    {#if icon}<Icon name={icon} size={size === 'sm' ? 14 : 16} />{/if}
    {#if text}<span>{text}</span>{/if}
    {@render children?.()}
  </a>
{:else}
  <button type={type} class={classes} {disabled} formaction={formaction || undefined} formnovalidate={formnovalidate || undefined} onclick={onclick}>
    {#if icon}<Icon name={icon} size={size === 'sm' ? 14 : 16} />{/if}
    {#if text}<span>{text}</span>{/if}
    {@render children?.()}
  </button>
{/if}
