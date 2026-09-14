<script lang="ts">
  // M18：板块图标选择器（IconPicker）——从 lucide 图标目录（成熟图标库
  // lucide-static 的分类精选集，icon-catalog.generated.ts）浏览/搜索选择。
  //
  // 结构（SSR + 无 JS 降级可访问，与全站表单约定一致）：
  // - 常驻原生 <select>（optgroup 按分类分组）：无 JS 时唯一可用且表单可
  //   序列化的控件，name 即提交字段名；
  // - JS 增强（hasJs）：当前选择预览 + 「浏览图标库」展开面板（搜索 +
  //   分类跳转 + 图标网格），点击网格项与 select 双向同步（同一绑定值）；
  // - 值语义与后端一致：'' = 未设置（后端存 NULL），非空 = lucide 图标名。
  import Icon from './Icon.svelte';
  import { ICON_CATEGORIES, catalogIcons } from './icon-catalog.generated';
  import { icons } from './icons';

  let {
    value = $bindable(''),
    name = 'icon',
    id,
    label = '板块图标',
    hint = '从 lucide 图标库选择；不选则使用默认图标。'
  }: {
    /** 绑定值：'' = 未设置（提交空串 = 清除）；非空 = 图标名。 */
    value?: string;
    /** 表单字段名（select 的 name 属性）。 */
    name?: string;
    /** 控件 id（label for）；缺省时由 name 派生。 */
    id?: string;
    label?: string;
    hint?: string;
  } = $props();

  const selectId = $derived(id ?? `${name}-picker`);
  const panelId = $derived(`${selectId}-panel`);

  // JS 增强区只在客户端渲染（SSR 恒无，无 JS 用户只见原生 select）。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  let panelOpen = $state(false);
  let q = $state('');

  const allNames = $derived(ICON_CATEGORIES.flatMap((c) => c.icons.map((i) => i.name)));

  /** 搜索过滤（图标名或分类名匹配；大小写不敏感）。 */
  const filteredCategories = $derived.by(() => {
    const query = q.trim().toLowerCase();
    if (!query) return ICON_CATEGORIES;
    return ICON_CATEGORIES.map((c) => ({
      ...c,
      icons: c.icons.filter(
        (i) => i.name.includes(query) || c.label.toLowerCase().includes(query)
      )
    })).filter((c) => c.icons.length > 0);
  });

  const filteredCount = $derived(filteredCategories.reduce((n, c) => n + c.icons.length, 0));

  function pick(next: string): void {
    value = next;
  }

  /** 当前值是否可渲染（在 allowlist 内；手写表或目录）。 */
  function isKnown(name: string): boolean {
    return Boolean(name && icons[name]);
  }
  const valueKnown = $derived(isKnown(value));
  const valueCategory = $derived(
    ICON_CATEGORIES.find((c) => c.icons.some((i) => i.name === value))?.label ?? ''
  );
</script>

<div class="icon-picker">
  <!-- 常驻原生 select：无 JS 唯一可用路径，表单序列化字段 -->
  <div class="input-wrapper">
    <label class="input-label" for={selectId}>{label}</label>
    <select class="input-field" id={selectId} {name} bind:value>
      <option value="">（默认）不使用图标</option>
      {#each ICON_CATEGORIES as category (category.label)}
        <optgroup label={category.label}>
          {#each category.icons as entry (entry.name)}
            <option value={entry.name}>{entry.name}</option>
          {/each}
        </optgroup>
      {/each}
      <!-- 当前值不在目录（历史数据/手写图标）时补一项，保证 select 回显 -->
      {#if value && !allNames.includes(value)}
        <option value={value}>{value}（当前）</option>
      {/if}
    </select>
    {#if hint}
      <p class="input-hint" style="margin:4px 0 0;">{hint}</p>
    {/if}
  </div>

  {#if hasJs}
    <div class="icon-picker__head">
      <span class="icon-picker__preview" class:icon-picker__preview--empty={!value || !valueKnown} aria-hidden="true">
        {#if value && valueKnown}
          <Icon name={value} size={22} />
        {:else}
          <Icon name="shapes" size={16} />
        {/if}
      </span>
      <span class="icon-picker__current">
        {#if value}
          <code>{value}</code>
          {#if valueKnown}
            <span class="icon-picker__cat">{valueCategory || '自定义'}</span>
          {:else}
            <span class="icon-picker__cat">未知名（渲染为默认）</span>
          {/if}
        {:else}
          <span class="text-secondary">未设置图标</span>
        {/if}
      </span>
      <button
        type="button"
        class="btn ghost sm"
        aria-expanded={panelOpen}
        aria-controls={panelId}
        onclick={() => (panelOpen = !panelOpen)}
      >
        <Icon name={panelOpen ? 'chevron-down' : 'search'} size={14} />
        {panelOpen ? '收起图标库' : '浏览图标库'}
      </button>
      {#if value}
        <button type="button" class="btn ghost sm" onclick={() => pick('')}>清除</button>
      {/if}
    </div>

    {#if panelOpen}
      <div class="icon-picker__panel" id={panelId}>
        <input
          type="search"
          class="input-field"
          placeholder="搜索图标名或分类（如 code、自然）…"
          aria-label="搜索图标"
          bind:value={q}
        />
        <p class="icon-picker__meta" role="status">
          {q.trim() ? `${filteredCount} / ${allNames.length} 个图标` : `共 ${allNames.length} 个图标 · ${ICON_CATEGORIES.length} 个分类`}
        </p>
        <div class="icon-picker__scroll">
          {#each filteredCategories as category (category.label)}
            <section aria-label={category.label}>
              <h4>{category.label}</h4>
              <div class="icon-picker__grid">
                {#each category.icons as entry (entry.name)}
                  <button
                    type="button"
                    class="icon-picker__tile"
                    class:is-active={value === entry.name}
                    aria-pressed={value === entry.name}
                    title={entry.name}
                    aria-label="选择图标 {entry.name}"
                    onclick={() => pick(entry.name)}
                  >
                    <Icon name={entry.name} size={18} />
                  </button>
                {/each}
              </div>
            </section>
          {:else}
            <p class="input-hint">没有匹配「{q}」的图标，试试英文关键词（如 mail、flag）。</p>
          {/each}
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .icon-picker {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .icon-picker__head {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .icon-picker__preview {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    flex: none;
    border: 1px solid color-mix(in srgb, var(--color-accent) 32%, var(--color-border));
    background: color-mix(in srgb, var(--color-accent) 10%, transparent);
    color: var(--color-accent);
    border-radius: var(--radius-sm);
  }

  .icon-picker__preview--empty {
    border: 1px dashed var(--color-border);
    background: transparent;
    color: var(--color-text-tertiary);
  }

  .icon-picker__current {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    flex: 1 1 auto;
    font-size: var(--text-sm);
  }

  .icon-picker__current code {
    color: var(--color-text-primary);
    font-family: var(--font-family-mono);
    font-size: 12px;
  }

  .icon-picker__cat {
    color: var(--color-text-tertiary);
    font-size: 12px;
  }

  .icon-picker__panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-bg-subtle);
  }

  .icon-picker__meta {
    margin: 0;
    color: var(--color-text-tertiary);
    font: var(--weight-normal) 12px/18px var(--font-family-mono);
    font-variant-numeric: tabular-nums;
  }

  .icon-picker__scroll {
    max-height: 264px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding-right: 2px;
  }

  .icon-picker__scroll h4 {
    margin: 0 0 var(--space-1);
    color: var(--color-text-tertiary);
    font-size: 12px;
    font-weight: var(--weight-semibold);
    letter-spacing: 0.02em;
  }

  .icon-picker__grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(40px, 1fr));
    gap: 4px;
  }

  .icon-picker__tile {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-bg-card);
    color: var(--color-text-secondary);
    cursor: pointer;
    transition:
      background var(--duration-fast),
      color var(--duration-fast),
      border-color var(--duration-fast);
  }

  .icon-picker__tile:hover {
    border-color: color-mix(in srgb, var(--color-accent) 40%, var(--color-border));
    color: var(--color-accent);
  }

  .icon-picker__tile.is-active,
  .icon-picker__tile[aria-pressed='true'] {
    border-color: var(--color-accent);
    background: color-mix(in srgb, var(--color-accent) 14%, transparent);
    color: var(--color-accent);
  }
</style>
