<!-- M07-SHOP-STUDIO：装扮样式参数表单（精简为三大核心品类：彩色昵称、Steam动效头像框、个人资料背景） -->
<script lang="ts">
  import {
    NICKNAME_MODES,
    STYLE_LIMITS,
    NICKNAME_GRADIENT_ANIMATIONS,
    NICKNAME_GLOW_ANIMATIONS,
    NICKNAME_SOLID_ANIMATIONS,
    PROFILE_ANIMATIONS,
    type StyleDraft,
    type StyleDraftErrors
  } from './style-draft';
  import GradientEditor from './controls/GradientEditor.svelte';
  import SegmentedSpeed from './controls/SegmentedSpeed.svelte';
  import SliderField from './controls/SliderField.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';

  let {
    draft = $bindable(),
    errors = {},
    idPrefix = 'sd',
    disabled = false,
    onopensteamframe,
    onopensteambackground
  }: {
    draft: StyleDraft;
    errors?: StyleDraftErrors;
    idPrefix?: string;
    disabled?: boolean;
    onopensteamframe?: () => void;
    onopensteambackground?: () => void;
  } = $props();

  function insertCssTemplate(type: 'cyber' | '3d' | 'neon') {
    if (type === 'cyber') {
      draft.css = `/* 赛博流光滤镜与双色投影 */\nfilter: drop-shadow(0 0 8px currentColor) brightness(1.15);\nanimation: cyber-mirage 3s ease-in-out infinite alternate;\n@keyframes cyber-mirage {\n  0% { filter: drop-shadow(0 0 4px #00ffff) hue-rotate(0deg); }\n  100% { filter: drop-shadow(0 0 12px #ff007f) hue-rotate(90deg); }\n}`;
    } else if (type === '3d') {
      draft.css = `/* 3D 浮雕微倾斜效果 */\ntransform: perspective(600px) rotateX(6deg) rotateY(-4deg);\ntransition: transform 0.3s ease;\ntext-shadow: 1px 1px 0px #ffffff, 2px 2px 0px rgba(0,0,0,0.15);`;
    } else if (type === 'neon') {
      draft.css = `/* 霓虹电弧与频闪 */\nbox-shadow: 0 0 12px #00f0ff, inset 0 0 6px #ff003c;\nanimation: neon-flicker 2.5s infinite;\n@keyframes neon-flicker {\n  0%, 100% { opacity: 1; }\n  45% { opacity: 0.85; filter: contrast(1.2); }\n  50% { opacity: 0.6; }\n  55% { opacity: 0.95; }\n}`;
    }
  }

  function insertJsTemplate(type: 'hover' | 'pulse') {
    if (type === 'hover') {
      draft.js = `// el: 挂载的 DOM 目标元素, style: 样式数据配置\nel.style.transition = 'transform 0.25s ease, filter 0.25s ease';\nel.addEventListener('mouseenter', () => {\n  el.style.transform = 'scale(1.08) translateY(-2px)';\n  el.style.filter = 'brightness(1.25)';\n});\nel.addEventListener('mouseleave', () => {\n  el.style.transform = '';\n  el.style.filter = '';\n});`;
    } else if (type === 'pulse') {
      draft.js = `// 动态连续微脉冲\nlet t = 0;\nconst interval = setInterval(() => {\n  if (!document.contains(el)) return clearInterval(interval);\n  t += 0.06;\n  const scale = 1 + Math.sin(t) * 0.035;\n  el.style.transform = 'scale(' + scale.toFixed(3) + ')';\n}, 30);`;
    }
  }
</script>

<input type="hidden" name="mode" value={draft.mode} />
<input type="hidden" name="shadowPx" value={draft.shadowPx} />
<input type="hidden" name="letterSpacing" value={draft.letterSpacing} />
<input type="hidden" name="angle" value={draft.angle} />
<input type="hidden" name="opacity" value={draft.opacity} />

{#if draft.kind === 'nickname_color'}
  <!-- 1. 彩色昵称 / 特效 -->
  <div class="input-wrapper">
    <span class="input-label">样式类型</span>
    <div class="sd-chip-row">
      {#each NICKNAME_MODES as m (m.value)}
        <button type="button" class="chip" class:is-active={draft.mode === m.value} {disabled} onclick={() => (draft.mode = m.value)}>
          {m.label}
        </button>
      {/each}
    </div>
  </div>

  {#if draft.mode === 'gradient'}
    <div class="input-wrapper">
      <span class="input-label">渐变颜色（{STYLE_LIMITS.stopsMin}–{STYLE_LIMITS.stopsMax} 个色标；点击渐变条空白处添加）</span>
      <GradientEditor bind:stops={draft.stops} {disabled} idPrefix="{idPrefix}-grad" />
      {#if errors.stops}<p class="input-error">{errors.stops}</p>{/if}
    </div>
    <div class="sd-grid">
      <div class="input-wrapper">
        <label class="input-label" for="{idPrefix}-grad-anim">动画效果</label>
        <select id="{idPrefix}-grad-anim" name="animate" class="input-field" bind:value={draft.animate} {disabled}>
          {#each NICKNAME_GRADIENT_ANIMATIONS as a (a.value)}
            <option value={a.value}>{a.label}</option>
          {/each}
        </select>
      </div>
      {#if draft.animate !== 'none'}
        <div class="input-wrapper">
          <span class="input-label">动画周期/速度</span>
          <SegmentedSpeed bind:value={draft.durationMs} {disabled} idPrefix="{idPrefix}-grad-spd" />
          {#if errors.durationMs}<p class="input-error">{errors.durationMs}</p>{/if}
        </div>
      {/if}
    </div>
    <details class="sd-advanced" open={draft.angle !== 90 || draft.shadowPx > 0 || draft.letterSpacing > 0 || undefined}>
      <summary>高阶样式参数（角度、外发光、字间距）</summary>
      <div class="sd-grid">
        <SliderField id="{idPrefix}-grad-angle" label="渐变角度" name="angle" unit="°" min={STYLE_LIMITS.angleMin} max={STYLE_LIMITS.angleMax} step={15} bind:value={draft.angle} {disabled} />
        <SliderField id="{idPrefix}-grad-shadow" label="外发光阴影" name="shadowPx" unit="px" min={STYLE_LIMITS.shadowMin} max={STYLE_LIMITS.shadowMax} bind:value={draft.shadowPx} {disabled} />
        <SliderField id="{idPrefix}-grad-spacing" label="字间距" name="letterSpacing" unit="px" min={STYLE_LIMITS.letterSpacingMin} max={STYLE_LIMITS.letterSpacingMax} bind:value={draft.letterSpacing} {disabled} />
      </div>
    </details>
  {:else if draft.mode === 'glow'}
    <div class="input-wrapper">
      <label class="input-label" for="{idPrefix}-nick-color">发光主体颜色 *</label>
      <input id="{idPrefix}-nick-color" name="color" type="color" bind:value={draft.color} {disabled} class="sd-color-input sd-color-input--lg" />
      {#if errors.color}<p class="input-error">{errors.color}</p>{/if}
    </div>
    <div class="sd-grid">
      <div class="input-wrapper">
        <label class="input-label" for="{idPrefix}-glow-anim">动效模式</label>
        <select id="{idPrefix}-glow-anim" name="animate" class="input-field" bind:value={draft.animate} {disabled}>
          {#each NICKNAME_GLOW_ANIMATIONS as a (a.value)}
            <option value={a.value}>{a.label}</option>
          {/each}
        </select>
      </div>
      {#if draft.animate !== 'none'}
        <div class="input-wrapper">
          <span class="input-label">动效速度</span>
          <SegmentedSpeed bind:value={draft.durationMs} {disabled} idPrefix="{idPrefix}-glow-spd" />
          {#if errors.durationMs}<p class="input-error">{errors.durationMs}</p>{/if}
        </div>
      {/if}
    </div>
    <details class="sd-advanced" open={draft.shadowPx > 0 || draft.letterSpacing > 0 || undefined}>
      <summary>高阶样式参数（光晕强度、字间距）</summary>
      <div class="sd-grid">
        <SliderField id="{idPrefix}-glow-shadow" label="额外发光扩散" name="shadowPx" unit="px" min={STYLE_LIMITS.shadowMin} max={STYLE_LIMITS.shadowMax} bind:value={draft.shadowPx} {disabled} />
        <SliderField id="{idPrefix}-glow-spacing" label="字间距" name="letterSpacing" unit="px" min={STYLE_LIMITS.letterSpacingMin} max={STYLE_LIMITS.letterSpacingMax} bind:value={draft.letterSpacing} {disabled} />
      </div>
    </details>
  {:else}
    <div class="input-wrapper">
      <label class="input-label" for="{idPrefix}-nick-color">字体纯色 *</label>
      <input id="{idPrefix}-nick-color" name="color" type="color" bind:value={draft.color} {disabled} class="sd-color-input sd-color-input--lg" />
      {#if errors.color}<p class="input-error">{errors.color}</p>{/if}
    </div>
    <details class="sd-advanced" open={draft.animate !== 'none' || draft.shadowPx > 0 || draft.letterSpacing > 0 || undefined}>
      <summary>动效与文字排版设置</summary>
      <div class="sd-grid">
        <div class="input-wrapper">
          <label class="input-label" for="{idPrefix}-solid-anim">动态微动</label>
          <select id="{idPrefix}-solid-anim" name="animate" class="input-field" bind:value={draft.animate} {disabled}>
            {#each NICKNAME_SOLID_ANIMATIONS as a (a.value)}
              <option value={a.value}>{a.label}</option>
            {/each}
          </select>
        </div>
        {#if draft.animate !== 'none'}
          <div class="input-wrapper">
            <span class="input-label">动画周期</span>
            <SegmentedSpeed bind:value={draft.durationMs} {disabled} idPrefix="{idPrefix}-solid-spd" />
          </div>
        {/if}
      </div>
      <div class="sd-grid">
        <SliderField id="{idPrefix}-solid-shadow" label="文字阴影/发光" name="shadowPx" unit="px" min={STYLE_LIMITS.shadowMin} max={STYLE_LIMITS.shadowMax} bind:value={draft.shadowPx} {disabled} />
        <SliderField id="{idPrefix}-solid-spacing" label="字间距" name="letterSpacing" unit="px" min={STYLE_LIMITS.letterSpacingMin} max={STYLE_LIMITS.letterSpacingMax} bind:value={draft.letterSpacing} {disabled} />
      </div>
    </details>
  {/if}

{:else if draft.kind === 'avatar_frame'}
  <!-- 2. Steam 动效头像框 -->
  <div class="input-wrapper">
    <div class="sf-steam-hero-box">
      <div class="sf-steam-hero-text">
        <strong style="color: #66c0f4; font-size: 14px;">Steam 官方点数商店全量动效库</strong>
        <p style="margin: 2px 0 0; font-size: 12px; color: var(--color-text-secondary);">
          系统已全面摒弃简陋手绘边框，内置全量 2071 款 Steam 正版高清 APNG 动图，一键点选即可无缝包裹头像！
        </p>
      </div>
      <button type="button" class="btn btn-primary sm" {disabled} onclick={() => onopensteamframe?.()}>
        <Icon name="sparkles" size={14} />
        <span>浏览 2071 款 Steam 头像框库</span>
      </button>
    </div>
  </div>

  <div class="sd-grid" style="margin-top: 12px;">
    <div class="input-wrapper">
      <label class="input-label" for="{idPrefix}-frame-shape">头像边框契合形状</label>
      <select id="{idPrefix}-frame-shape" name="shape" class="input-field" bind:value={draft.shape} {disabled}>
        <option value="rounded">微圆角正方形 (4px Steam 官方规范)</option>
        <option value="circle">正圆形 (50% 适配内嵌圆环框)</option>
        <option value="square">纯直角方形 (0px 纯直角相框)</option>
      </select>
    </div>
    <SliderField id="{idPrefix}-frame-scale" label="边框包裹缩放" name="frameScale" unit="%" min={100} max={180} step={1} bind:value={draft.frameScale} {disabled} />
  </div>

{:else if draft.kind === 'profile_effect'}
  <!-- 3. 个人资料背景 / 封面 -->
  <div class="input-wrapper">
    <div class="sf-steam-hero-box" style="border-color: rgba(168, 85, 247, 0.4);">
      <div class="sf-steam-hero-text">
        <strong style="color: #c084fc; font-size: 14px;">Steam 动态个人资料背景库 (1000 款)</strong>
        <p style="margin: 2px 0 0; font-size: 12px; color: var(--color-text-secondary);">
          已全面收录 Steam 点数商店全量 1000 款动态个人资料背景，一键挑选并设置到商城商品！
        </p>
      </div>
      <button type="button" class="btn btn-secondary sm" {disabled} onclick={() => onopensteambackground?.()}>
        <Icon name="sparkles" size={14} />
        <span>挑选 Steam 动态背景库 (1000款)</span>
      </button>
    </div>
  </div>

  <div class="sd-grid" style="margin-top: 12px;">
    <div class="input-wrapper">
      <label class="input-label" for="{idPrefix}-profile-base">背景基底色</label>
      <input id="{idPrefix}-profile-base" name="baseColor" type="color" bind:value={draft.baseColor} {disabled} class="sd-color-input" />
    </div>
    <div class="input-wrapper">
      <label class="input-label" for="{idPrefix}-profile-accent">光效强调色</label>
      <input id="{idPrefix}-profile-accent" name="accentColor" type="color" bind:value={draft.accentColor} {disabled} class="sd-color-input" />
    </div>
    <div class="input-wrapper">
      <label class="input-label" for="{idPrefix}-profile-animate">天幕动效</label>
      <select id="{idPrefix}-profile-animate" name="animate" class="input-field" bind:value={draft.animate} {disabled}>
        {#each PROFILE_ANIMATIONS as a (a.value)}
          <option value={a.value}>{a.label}</option>
        {/each}
      </select>
    </div>
  </div>
  <div class="sd-grid">
    <SliderField id="{idPrefix}-profile-opacity" label="背景不透明度" name="opacity" unit="%" min={STYLE_LIMITS.opacityMin} max={STYLE_LIMITS.opacityMax} step={5} bind:value={draft.opacity} {disabled} />
  </div>
{/if}

<!-- 开放代码定义：自定义 CSS & JS 增强 -->
<details class="sd-advanced sd-code-section" open={Boolean(draft.css || draft.js || errors.css || errors.js) || undefined}>
  <summary class="sd-code-summary">
    <span>⚡ 开放代码定义：自定义 CSS 与 JS 动画钩子</span>
  </summary>
  <p class="sd-code-desc">
    装扮工作台已开放自定义 CSS 和 JS：支持输入自定义 CSS 规则（选择器、滤镜、@keyframes 等）以及 JS 挂载钩子脚本，所见即所得。
  </p>

  <div class="input-wrapper">
    <div class="sd-code-header">
      <label class="input-label" for="{idPrefix}-custom-css">自定义 CSS 代码</label>
      <div class="sd-code-actions">
        <span class="sd-template-kicker">预设模板:</span>
        <button type="button" class="btn-xs" {disabled} onclick={() => insertCssTemplate('cyber')}>🌈 赛博流光</button>
        <button type="button" class="btn-xs" {disabled} onclick={() => insertCssTemplate('3d')}>💫 3D 浮雕</button>
        <button type="button" class="btn-xs" {disabled} onclick={() => insertCssTemplate('neon')}>⚡ 霓虹频闪</button>
      </div>
    </div>
    <textarea
      id="{idPrefix}-custom-css"
      name="css"
      class="sd-code-editor"
      rows="4"
      placeholder={"/* 示例：输入自定义 CSS，自动作用于装扮 */\nfilter: drop-shadow(0 0 8px currentColor);\nanimation: my-rotate 4s infinite linear;\n@keyframes my-rotate { to { transform: rotate(360deg); } }"}
      bind:value={draft.css}
      {disabled}
    ></textarea>
    {#if errors.css}<p class="input-error">{errors.css}</p>{/if}
  </div>

  <div class="input-wrapper">
    <div class="sd-code-header">
      <label class="input-label" for="{idPrefix}-custom-js">自定义 JS 脚本动画 Hook</label>
      <div class="sd-code-actions">
        <span class="sd-template-kicker">预设脚本:</span>
        <button type="button" class="btn-xs" {disabled} onclick={() => insertJsTemplate('hover')}>✨ 悬停微动</button>
        <button type="button" class="btn-xs" {disabled} onclick={() => insertJsTemplate('pulse')}>🌊 动态心跳</button>
      </div>
    </div>
    <textarea
      id="{idPrefix}-custom-js"
      name="js"
      class="sd-code-editor"
      rows="4"
      placeholder={"// 挂载时执行：传入目标 HTMLElement 与 style 配置\n// (el, style) => {\nel.addEventListener('mouseenter', () => { el.style.transform = 'scale(1.1)'; });\nel.addEventListener('mouseleave', () => { el.style.transform = ''; });"}
      bind:value={draft.js}
      {disabled}
    ></textarea>
    {#if errors.js}<p class="input-error">{errors.js}</p>{/if}
  </div>
</details>

<style>
  .sf-steam-hero-box {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 12px;
    padding: 12px 14px;
    border-radius: 10px;
    background: linear-gradient(135deg, rgba(23, 26, 33, 0.95), rgba(27, 40, 56, 0.9));
    border: 1px solid rgba(102, 192, 244, 0.4);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  }
  .sf-steam-hero-text {
    flex: 1;
    min-width: 200px;
  }
  .sd-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
  .sd-chip-row {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .sd-advanced {
    margin-top: var(--space-2);
    border: 1px dashed var(--color-border, #ccc);
    border-radius: 8px;
    padding: var(--space-1, 4px) var(--space-2, 8px) var(--space-2, 8px);
  }
  .sd-advanced > summary {
    cursor: pointer;
    font-size: var(--text-xs, 12px);
    font-weight: 600;
    color: var(--color-text-secondary, #666);
    padding: var(--space-1, 4px) 0;
    user-select: none;
  }
  .sd-code-section {
    border-color: var(--color-brand, #6366f1);
    background: rgba(99, 102, 241, 0.03);
    margin-top: var(--space-3, 12px);
  }
  .sd-code-summary {
    color: var(--color-brand, #6366f1) !important;
    font-weight: 700 !important;
  }
  .sd-code-desc {
    font-size: var(--text-xs, 12px);
    color: var(--color-text-secondary, #666);
    margin: var(--space-1, 4px) 0 var(--space-2, 8px);
    line-height: 1.5;
  }
  .sd-code-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 4px;
  }
  .sd-code-actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .sd-template-kicker {
    font-size: 11px;
    color: var(--color-text-tertiary, #999);
  }
  .btn-xs {
    font-size: 11px;
    padding: 2px 7px;
    border-radius: 4px;
    border: 1px solid var(--color-border, #ccc);
    background: var(--color-bg-card, #fff);
    cursor: pointer;
    color: var(--color-text-secondary, #555);
    transition: all 0.15s ease;
  }
  .btn-xs:hover {
    border-color: var(--color-brand, #6366f1);
    color: var(--color-brand, #6366f1);
  }
  .sd-code-editor {
    width: 100%;
    box-sizing: border-box;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 12px;
    line-height: 1.45;
    padding: 8px 10px;
    border: 1px solid var(--color-border, #ccc);
    border-radius: 6px;
    background: #1e1e2e;
    color: #cdd6f4;
    resize: vertical;
  }
  .sd-code-editor:focus {
    outline: none;
    border-color: var(--color-brand, #6366f1);
    box-shadow: 0 0 0 2px rgba(99, 102, 241, 0.2);
  }
  .sd-color-input {
    width: 42px;
    height: 32px;
    padding: 0;
    border: 1px solid var(--color-border, #ccc);
    border-radius: 6px;
    background: none;
    cursor: pointer;
  }
  .sd-color-input--lg {
    width: 64px;
    height: 34px;
  }
  .input-wrapper {
    margin-top: var(--space-2);
  }
  .input-wrapper:first-child {
    margin-top: 0;
  }
  .input-error {
    color: var(--color-danger, #cf222e);
    font-size: var(--text-xs, 12px);
    margin-top: var(--space-1, 4px);
  }
  .chip {
    padding: 6px 14px;
    border: 1px solid var(--color-border, #ccc);
    border-radius: 999px;
    background: var(--color-bg-card, #fff);
    color: inherit;
    cursor: pointer;
    font-size: var(--text-sm, 13px);
    transition: border-color 120ms ease, box-shadow 120ms ease;
  }
  .chip:hover { border-color: var(--color-border-strong, #999); }
  .chip.is-active { border-color: var(--color-brand, #4f6bed); box-shadow: 0 0 0 1px var(--color-brand, #4f6bed); }
  .chip:disabled { opacity: 0.6; cursor: not-allowed; }
</style>
