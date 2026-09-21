<!-- M07-SHOP-STUDIO-UX：可视化渐变编辑器。
  渐变条 = 主要交互面：点色标选中、拖动重排、双击删除、点空白处按当前
  渐变插值添加；下方一排原生 color input（name="stops"）既是编辑入口
  也是提交字段，无 JS 时它们就是完整回退（与旧面板一致）。
  「类比 / 互补 / 灵感」一键生成协调色组（HSL 空间，见 gradient-utils）。 -->
<script lang="ts">
  import Button from '$lib/components/ui/Button.svelte';
  import { STYLE_LIMITS } from '../style-draft';
  import { harmonyStops, randomHarmonyStops, sampleGradient, type HarmonyMode } from './gradient-utils';

  let {
    stops = $bindable(),
    disabled = false,
    idPrefix = 'ge'
  }: {
    stops: string[];
    disabled?: boolean;
    idPrefix?: string;
  } = $props();

  let selected = $state(0);
  let track = $state<HTMLDivElement | null>(null);
  /** 正在拖动的色标下标（null = 未拖动；用于区分单击与拖拽）。 */
  let dragging = $state<number | null>(null);

  const gradientCss = $derived(`linear-gradient(90deg, ${stops.join(', ')})`);
  const canAdd = $derived(stops.length < STYLE_LIMITS.stopsMax);
  const canRemove = $derived(stops.length > STYLE_LIMITS.stopsMin);
  const selectedSafe = $derived(Math.min(selected, Math.max(0, stops.length - 1)));

  function posOf(i: number): number {
    return stops.length <= 1 ? 0 : (i / (stops.length - 1)) * 100;
  }

  function fractionFromEvent(e: PointerEvent): number {
    if (!track) return 0;
    const rect = track.getBoundingClientRect();
    if (rect.width <= 0) return 0;
    return Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
  }

  function onTrackPointerDown(e: PointerEvent): void {
    if (disabled || !canAdd) return;
    // 点空白：按当前渐变插值色在该位置插入新色标
    const f = fractionFromEvent(e);
    const color = sampleGradient(stops, f);
    const idx = Math.max(0, Math.min(stops.length, Math.round(f * (stops.length - 1))));
    const next = [...stops];
    next.splice(idx, 0, color);
    stops = next;
    selected = idx;
  }

  function onHandlePointerDown(i: number, e: PointerEvent): void {
    if (disabled) return;
    e.stopPropagation();
    selected = i;
    dragging = i;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onHandlePointerMove(i: number, e: PointerEvent): void {
    if (dragging !== i || disabled) return;
    const target = Math.max(0, Math.min(stops.length - 1, Math.round(fractionFromEvent(e) * (stops.length - 1))));
    if (target !== i) {
      const next = [...stops];
      const [moved] = next.splice(i, 1);
      next.splice(target, 0, moved);
      stops = next;
      selected = target;
      dragging = target;
    }
  }

  function onHandlePointerUp(): void {
    dragging = null;
  }

  function removeStop(i: number): void {
    if (disabled || !canRemove) return;
    stops = stops.filter((_, j) => j !== i);
    selected = Math.max(0, Math.min(selectedSafe, stops.length - 1));
  }

  function applyHarmony(mode: HarmonyMode): void {
    stops = harmonyStops(mode, stops[selectedSafe] ?? stops[0] ?? '#f472b6', stops.length);
  }
</script>

<div class="ge">
  <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
  <div
    class="ge-track"
    style:background={gradientCss}
    bind:this={track}
    onpointerdown={onTrackPointerDown}
    title="点击空白处添加色标"
  >
    {#each stops as stop, i (i)}
      <button
        type="button"
        class="ge-handle"
        class:is-active={selectedSafe === i}
        style:left="{posOf(i)}%"
        style:background={stop}
        {disabled}
        aria-label="色标 {i + 1}（{stop}）：点击选中，拖动重排，双击删除"
        onpointerdown={(e) => onHandlePointerDown(i, e)}
        onpointermove={(e) => onHandlePointerMove(i, e)}
        onpointerup={onHandlePointerUp}
        ondblclick={() => removeStop(i)}
      ></button>
    {/each}
  </div>

  <div class="ge-stops-row">
    {#each stops as stop, i (i)}
      <input
        type="color"
        name="stops"
        aria-label="渐变色标 {i + 1}"
        class="ge-color-input"
        class:is-active={selectedSafe === i}
        value={stop}
        onfocus={() => (selected = i)}
        oninput={(e) => {
          const v = (e.currentTarget as HTMLInputElement).value;
          stops = stops.map((s, j) => (j === i ? v : s));
        }}
        {disabled}
      />
    {/each}
    <Button text="+ 加色标" size="sm" type="button" onclick={() => { if (canAdd) { stops = [...stops, sampleGradient(stops, 1)]; selected = stops.length - 1; } }} disabled={disabled || !canAdd} />
    <Button text="− 减色标" size="sm" type="button" variant="ghost" onclick={() => removeStop(stops.length - 1)} disabled={disabled || !canRemove} />
  </div>

  <div class="ge-harmony-row">
    <span class="ge-harmony-label">配色灵感：</span>
    <Button text="类比色" size="sm" type="button" variant="ghost" onclick={() => applyHarmony('analogous')} {disabled} />
    <Button text="互补色" size="sm" type="button" variant="ghost" onclick={() => applyHarmony('complementary')} {disabled} />
    <Button text="随机一个" size="sm" type="button" variant="ghost" onclick={() => (stops = randomHarmonyStops(stops.length))} {disabled} />
    <span class="ge-harmony-hint">基于选中色标生成</span>
  </div>
</div>

<style>
  .ge {
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
  }
  .ge-track {
    position: relative;
    height: 28px;
    border-radius: var(--radius-sm, 6px);
    border: 1px solid var(--color-border);
    cursor: copy;
  }
  .ge-handle {
    position: absolute;
    top: 50%;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: 2px solid #fff;
    box-shadow: 0 1px 4px rgb(0 0 0 / 0.35);
    transform: translate(-50%, -50%);
    cursor: grab;
    padding: 0;
  }
  .ge-handle:active {
    cursor: grabbing;
  }
  .ge-handle.is-active {
    outline: 2px solid var(--color-brand);
    outline-offset: 1px;
  }
  .ge-stops-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    flex-wrap: wrap;
  }
  .ge-color-input {
    width: 30px;
    height: 30px;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm, 6px);
    background: none;
    cursor: pointer;
  }
  .ge-color-input.is-active {
    outline: 2px solid var(--color-brand);
    outline-offset: 1px;
  }
  .ge-harmony-row {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    flex-wrap: wrap;
  }
  .ge-harmony-label,
  .ge-harmony-hint {
    font-size: 0.78rem;
    color: var(--color-text-secondary);
  }
</style>
