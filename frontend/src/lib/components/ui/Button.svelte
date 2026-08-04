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
    extraClass = '',
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
    extraClass?: string;
    onclick?: (event: MouseEvent) => void;
    children?: Snippet;
  } = $props();

  const classes = $derived(['btn', `btn-${variant}`, `btn-${size}`, extraClass].filter(Boolean).join(' '));
</script>

{#if href}
  <a href={href} class={classes} aria-disabled={disabled || undefined}>
    {#if icon}<Icon name={icon} size={size === 'sm' ? 14 : 16} />{/if}
    {#if text}<span>{text}</span>{/if}
    {@render children?.()}
  </a>
{:else}
  <button type={type} class={classes} {disabled} onclick={onclick}>
    {#if icon}<Icon name={icon} size={size === 'sm' ? 14 : 16} />{/if}
    {#if text}<span>{text}</span>{/if}
    {@render children?.()}
  </button>
{/if}
