<script lang="ts">
  // M19-COMP-05：统计数字组（MetricGroup）——「一排小盒子」的替代件。
  // 纪律（docs/DESIGN-SYSTEM.md §2.1）：条目不是卡片，盒中盒禁止；
  // 单行时单元格之间只用发丝线（≤4 项契约，当前两处使用均为 4 项），
  // 窄屏退化为 2×2 并改用行发丝线。数值/标签/注记一律 mono + tabular-nums。
  // 颜色只消费语义变量；tone 仅用于状态信号（增长/告警），不做装饰。
  export interface MetricItem {
    label: string;
    value: string | number;
    note?: string;
    /** 值的状态色调（如待审数 > 0 用 warning）。 */
    tone?: 'default' | 'success' | 'warning' | 'danger' | 'info';
    /** 注记的状态色调（如本周 +N 用 success）。 */
    note_tone?: 'default' | 'success' | 'warning' | 'danger' | 'info';
    /** 12px 标签前导图标名（可选，纯扫描锚点）。 */
    icon?: string;
    /** 可选的后台队列入口；有 href 时整个指标成为可聚焦链接。 */
    href?: string;
  }

  import Icon from '$lib/components/ui/Icon.svelte';

  let { items = [] as MetricItem[] }: { items?: MetricItem[] } = $props();
</script>

<div class="metric-group" role="list">
  {#each items as item (item.label)}
    <div class="metric" role="listitem">
      {#if item.href}
        <a class="metric__link" href={item.href} aria-label="{item.label}：{item.value}">
          <span class="metric__label">
            {#if item.icon}<Icon name={item.icon} size={12} />{/if}
            {item.label}
          </span>
          <strong class="metric__value metric__value--{item.tone ?? 'default'}">{item.value}</strong>
          {#if item.note}<span class="metric__note metric__note--{item.note_tone ?? 'default'}">{item.note}</span>{/if}
        </a>
      {:else}
        <span class="metric__label">
          {#if item.icon}<Icon name={item.icon} size={12} />{/if}
          {item.label}
        </span>
        <strong class="metric__value metric__value--{item.tone ?? 'default'}">{item.value}</strong>
        {#if item.note}<span class="metric__note metric__note--{item.note_tone ?? 'default'}">{item.note}</span>{/if}
      {/if}
    </div>
  {/each}
</div>

<style>
  .metric-group {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 0 var(--space-5);
  }

  .metric {
    display: grid;
    min-width: 0;
    align-content: start;
    gap: var(--space-1);
    padding: var(--space-2) 0;
  }

  .metric__link {
    display: grid;
    min-width: 0;
    gap: var(--space-1);
    color: inherit;
    text-decoration: none;
  }

  .metric__link:hover,
  .metric__link:focus-visible {
    text-decoration: none;
  }

  .metric__link:hover .metric__value,
  .metric__link:focus-visible .metric__value {
    color: var(--color-brand);
  }

  /* 单行仪表条：单元格之间发丝线，不是每格一条底线/一个盒子。 */
  .metric + .metric {
    border-left: 1px solid var(--color-border-muted);
    padding-left: var(--space-5);
  }

  .metric__label {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
    overflow: hidden;
    color: var(--color-text-secondary);
    font: var(--text-xs)/1.4 var(--font-family-mono);
    letter-spacing: var(--meta-letter-spacing);
  }

  .metric__icon {
    flex: 0 0 auto;
    color: var(--color-text-tertiary);
  }

  .metric__note {
    min-width: 0;
    overflow: hidden;
    color: var(--color-text-tertiary);
    font: var(--text-xs)/1.4 var(--font-family-mono);
    font-variant-numeric: tabular-nums;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .metric__value {
    color: var(--color-text-primary);
    font: 500 var(--text-2xl)/1.1 var(--font-family-mono);
    font-variant-numeric: tabular-nums;
    letter-spacing: -0.02em;
  }

  .metric__value--success { color: var(--color-success); }
  .metric__value--warning { color: var(--color-warning); }
  .metric__value--danger { color: var(--color-danger); }
  .metric__value--info { color: var(--color-info); }

  .metric__note--success { color: var(--color-success); }
  .metric__note--warning { color: var(--color-warning); }
  .metric__note--danger { color: var(--color-danger); }
  .metric__note--info { color: var(--color-info); }

  /* 窄屏 2×2：行间发丝线 + 偶数列左发丝线（4 项契约下选择器精确）。 */
  @media (max-width: 767px) {
    .metric-group {
      grid-template-columns: repeat(2, minmax(0, 1fr));
      row-gap: 0;
    }

    .metric + .metric {
      border-left: 0;
      padding-left: 0;
    }

    .metric:nth-child(even) {
      border-left: 1px solid var(--color-border-muted);
      padding-left: var(--space-4);
    }

    .metric:nth-child(n + 3) {
      border-top: 1px solid var(--color-border-muted);
      margin-top: var(--space-2);
      padding-top: var(--space-3);
    }
  }
</style>
