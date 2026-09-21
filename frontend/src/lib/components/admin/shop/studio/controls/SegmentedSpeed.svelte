<!-- M07-SHOP-STUDIO-UX：动画速度分段控件。
  说「快/适中/慢」而不是「5000ms」：原生 radio（name="durationMs"）承担
  提交与无 JS 回退，样式化为分段按钮；毫秒概念不再暴露给管理员。 -->
<script lang="ts">
  import { DURATION_OPTIONS } from '../style-draft';

  let {
    value = $bindable(),
    disabled = false,
    name = 'durationMs',
    idPrefix = 'spd'
  }: {
    value: number;
    disabled?: boolean;
    name?: string;
    idPrefix?: string;
  } = $props();

  /** 分段与 DURATION_OPTIONS 对齐（全部落在后端 800–30000 合法域）。 */
  const SEGMENTS: { v: number; label: string }[] = [
    { v: 1500, label: '很快' },
    { v: 2800, label: '快' },
    { v: 5000, label: '适中' },
    { v: 8000, label: '慢' },
    { v: 20000, label: '很慢' }
  ];

  /** 草稿值不在分段点时（如编辑旧样式的 12000ms），选中最近分段展示，
   *  但不擅自改写值——radio 仍是真实值，提交原样。 */
  const nearest = $derived(
    SEGMENTS.reduce((best, s) => (Math.abs(s.v - value) < Math.abs(best.v - value) ? s : best), SEGMENTS[0])
  );
  const inOptions = $derived(value >= DURATION_OPTIONS[0] && value <= DURATION_OPTIONS[DURATION_OPTIONS.length - 1]);
</script>

<div class="spd" role="radiogroup" aria-label="动画速度">
  {#each SEGMENTS as s (s.v)}
    <label class="spd-seg" class:is-active={nearest.v === s.v}>
      <input
        type="radio"
        {name}
        value={s.v}
        checked={nearest.v === s.v}
        {disabled}
        class="spd-radio"
        id="{idPrefix}-{s.v}"
        onchange={() => (value = s.v)}
      />
      <span class="spd-label">{s.label}</span>
    </label>
  {/each}
  {#if !inOptions}
    <span class="spd-note">当前 {value}ms（合法域 {DURATION_OPTIONS[0]}–{DURATION_OPTIONS[DURATION_OPTIONS.length - 1]}ms）</span>
  {/if}
</div>

<style>
  .spd {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    flex-wrap: wrap;
  }
  .spd-seg {
    cursor: pointer;
  }
  .spd-radio {
    position: absolute;
    opacity: 0;
    width: 1px;
    height: 1px;
  }
  .spd-label {
    display: inline-block;
    padding: 0.28rem 0.7rem;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm, 6px);
    font-size: 0.82rem;
    color: var(--color-text-secondary);
    background: var(--color-bg-card);
    transition: background 0.12s ease, border-color 0.12s ease, color 0.12s ease;
  }
  .spd-seg.is-active .spd-label {
    border-color: var(--color-brand);
    color: var(--color-brand);
    background: var(--color-brand-soft);
    font-weight: 600;
  }
  .spd-radio:focus-visible + .spd-label {
    outline: 2px solid var(--color-brand);
    outline-offset: 2px;
  }
  .spd-seg:has(.spd-radio:disabled) {
    cursor: not-allowed;
    opacity: 0.55;
  }
  .spd-note {
    font-size: 0.78rem;
    color: var(--color-warning);
  }
</style>
