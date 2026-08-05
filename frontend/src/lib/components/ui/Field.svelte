<script lang="ts">
  import type { Snippet } from 'svelte';

  let fieldSeed = 0;

  // M00-FRONTEND-07：表单错误关联。id 缺省时生成唯一 id
  // （推荐调用方显式传入，保证 SSR 与客户端一致）；
  // 错误/提示元素带稳定的 {fieldId}-error / {fieldId}-hint 锚点，供 aria-describedby 关联。
  function nextFieldId(): string {
    if (typeof crypto !== 'undefined' && typeof (crypto as Crypto).randomUUID === 'function') {
      return `bblbb-${(crypto as Crypto).randomUUID()}`;
    }
    fieldSeed += 1;
    return `bblbb-field-${fieldSeed}`;
  }

  let {
    label = '',
    hint = '',
    error = '',
    id,
    class: klass = '',
    children
  }: {
    label?: string;
    hint?: string;
    error?: string;
    id?: string;
    class?: string;
    children?: Snippet;
  } = $props();

  const fieldId = $derived(id ?? nextFieldId());
</script>

<div class="input-wrapper {klass}">
  {#if label}
    <label class="input-label" for={fieldId}>{label}</label>
  {/if}
  {#if children}{@render children()}{/if}
  {#if error}
    <p class="input-hint is-error" id="{fieldId}-error" role="alert">{error}</p>
  {:else if hint}
    <p class="input-hint" id="{fieldId}-hint">{hint}</p>
  {/if}
</div>
