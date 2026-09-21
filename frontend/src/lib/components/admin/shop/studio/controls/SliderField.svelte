<!-- M07-SHOP-STUDIO-UX：带数值徽标的滑杆字段。
  原生 range input（具名、带 min/max/step）= 提交字段 + 无 JS 回退；
  右侧徽标实时显示当前值，替代裸数字输入。 -->
<script lang="ts">
  let {
    value = $bindable(),
    min,
    max,
    step = 1,
    name,
    label,
    unit = '',
    disabled = false,
    id
  }: {
    value: number;
    min: number;
    max: number;
    step?: number;
    name: string;
    label: string;
    unit?: string;
    disabled?: boolean;
    id: string;
  } = $props();
</script>

<div class="slf">
  <div class="slf-head">
    <label class="input-label" for={id}>{label}</label>
    <span class="slf-badge">{value}{unit}</span>
  </div>
  <input
    {id}
    type="range"
    {name}
    {min}
    {max}
    {step}
    bind:value
    {disabled}
    class="slf-range"
    aria-label="{label}（{min}–{max}{unit}）"
  />
  <div class="slf-scale" aria-hidden="true">
    <span>{min}{unit}</span>
    <span>{max}{unit}</span>
  </div>
</div>

<style>
  .slf {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }
  .slf-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }
  .slf-badge {
    font-size: 0.78rem;
    font-weight: 600;
    color: var(--color-brand);
    background: var(--color-brand-soft);
    border: 1px solid color-mix(in srgb, var(--color-brand) 30%, transparent);
    border-radius: var(--radius-sm, 6px);
    padding: 0.05rem 0.45rem;
    font-variant-numeric: tabular-nums;
  }
  .slf-range {
    width: 100%;
    accent-color: var(--color-brand);
  }
  .slf-scale {
    display: flex;
    justify-content: space-between;
    font-size: 0.7rem;
    color: var(--color-text-tertiary);
  }
</style>
