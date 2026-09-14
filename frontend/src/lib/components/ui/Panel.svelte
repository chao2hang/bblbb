<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    title = '',
    description = '',
    variant = 'default',
    class: klass = '',
    actions,
    footer,
    children
  }: {
    title?: string;
    description?: string;
    variant?: 'default' | 'plain' | 'flush';
    class?: string;
    actions?: Snippet;
    footer?: Snippet;
    children?: Snippet;
  } = $props();
</script>

<aui-card class="bblbb-aui-panel panel--{variant} {klass}">
  {#if title || description || actions}
    <span slot="header" class="panel__header">
      <span class="panel__heading">
        {#if title}<span class="panel__title">{title}</span>{/if}
        {#if description}<span class="panel__description">{description}</span>{/if}
      </span>
      {#if actions}<span class="panel__actions">{@render actions()}</span>{/if}
    </span>
  {/if}
  {#if children}{@render children()}{/if}
  {#if footer}<span slot="footer" class="panel__footer">{@render footer()}</span>{/if}
</aui-card>

<style>
  :global(.bblbb-aui-panel) {
    display: block;
    min-width: 0;
  }

  :global(.bblbb-aui-panel.panel--plain) {
    --aui-surface: transparent;
    --aui-border: transparent;
  }

  :global(.bblbb-aui-panel.panel--flush) {
    --aui-card-content-padding: 0;
  }

  .panel__header,
  .panel__footer {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
  }

  .panel__heading {
    display: grid;
    min-width: 0;
    gap: var(--space-1);
  }

  .panel__title {
    color: var(--aui-text-primary);
    font: 700 12px/1.2 var(--aui-font-mono);
    letter-spacing: var(--label-letter-spacing);
    text-transform: uppercase;
  }

  .panel__description {
    color: var(--aui-text-secondary);
    font: var(--text-sm)/1.45 var(--aui-font-ui);
  }

  .panel__actions {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    margin-left: auto;
  }

  .panel__footer {
    justify-content: flex-end;
  }
</style>
