<script lang="ts">
  // M14-COMPONENTS-01：可访问 Select 基础组件（原生 <select> 包装）。
  //
  // 用原生 select 而非自定义列表：原生控件自带键盘（上下箭头）、读屏名称、
  // 移动端原生选择器与无 JS 提交能力。组件只补充 label/error/hint 关联。
  import type { Snippet } from 'svelte';

  export interface SelectOption {
    value: string;
    label: string;
    disabled?: boolean;
  }

  let {
    label = '',
    hint = '',
    error = '',
    id,
    value = '',
    name,
    options = [] as SelectOption[],
    placeholder = '',
    required = false,
    disabled = false,
    class: klass = '',
    onchange
  }: {
    label?: string;
    hint?: string;
    error?: string;
    id?: string;
    value?: string;
    name?: string;
    options?: SelectOption[];
    placeholder?: string;
    required?: boolean;
    disabled?: boolean;
    class?: string;
    onchange?: (event: Event) => void;
  } = $props();

  const selectId = $derived(id ?? label.toLowerCase().replace(/[^a-z0-9]+/g, '-'));
  const describedBy = $derived(
    [error ? `${selectId}-error` : hint ? `${selectId}-hint` : ''].filter(Boolean).join(' ') || undefined
  );
</script>

<div class="input-wrapper {klass}">
  {#if label}
    <label class="input-label" for={selectId}>{label}</label>
  {/if}
  <select
    id={selectId}
    class="input-field {error ? 'has-error' : ''}"
    {name}
    {value}
    {required}
    {disabled}
    aria-invalid={error ? 'true' : undefined}
    aria-describedby={describedBy}
    onchange={onchange}
  >
    {#if placeholder}
      <option value="" disabled>{placeholder}</option>
    {/if}
    {#each options as option (option.value)}
      <option value={option.value} disabled={option.disabled}>{option.label}</option>
    {/each}
  </select>
  {#if error}
    <p class="input-hint is-error" id="{selectId}-error" role="alert">{error}</p>
  {:else if hint}
    <p class="input-hint" id="{selectId}-hint">{hint}</p>
  {/if}
</div>
