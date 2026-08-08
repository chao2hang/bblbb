<script lang="ts">
  // M14-COMPONENTS-01：可访问 Input 基础组件。
  //
  // - label/error/hint 与控件通过稳定 id 关联（aria-describedby / aria-invalid）；
  // - 默认 type="text"，支持 type="email"/"password" 等原生语义；
  // - 只接收白名单 prop（安全投影，无任意属性穿透，M14-COMPONENTS-06）。
  import type { Snippet } from 'svelte';
  import type { HTMLInputAttributes } from 'svelte/elements';

  let {
    label = '',
    hint = '',
    error = '',
    id,
    type = 'text',
    value = '',
    name,
    autocomplete,
    placeholder = '',
    required = false,
    disabled = false,
    maxlength,
    class: klass = '',
    oninput
  }: {
    label?: string;
    hint?: string;
    error?: string;
    id?: string;
    type?: 'text' | 'email' | 'password' | 'search' | 'url' | 'tel';
    value?: string;
    name?: string;
    autocomplete?: HTMLInputAttributes['autocomplete'];
    placeholder?: string;
    required?: boolean;
    disabled?: boolean;
    maxlength?: number;
    class?: string;
    oninput?: (event: Event) => void;
  } = $props();

  const inputId = $derived(id ?? label.toLowerCase().replace(/[^a-z0-9]+/g, '-'));
  const describedBy = $derived(
    [error ? `${inputId}-error` : hint ? `${inputId}-hint` : ''].filter(Boolean).join(' ') || undefined
  );
</script>

<div class="input-wrapper {klass}">
  {#if label}
    <label class="input-label" for={inputId}>{label}</label>
  {/if}
  <input
    id={inputId}
    class="input-field {error ? 'has-error' : ''}"
    {type}
    {name}
    {value}
    {autocomplete}
    {placeholder}
    {required}
    {disabled}
    {maxlength}
    aria-invalid={error ? 'true' : undefined}
    aria-describedby={describedBy}
    oninput={oninput}
  />
  {#if error}
    <p class="input-hint is-error" id="{inputId}-error" role="alert">{error}</p>
  {:else if hint}
    <p class="input-hint" id="{inputId}-hint">{hint}</p>
  {/if}
</div>
