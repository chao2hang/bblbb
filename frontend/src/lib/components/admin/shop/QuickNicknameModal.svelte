<!-- 彩色昵称快速创建与上架弹窗 -->
<script lang="ts">
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import GradientEditor from '$lib/components/admin/shop/studio/controls/GradientEditor.svelte';
  import SegmentedSpeed from '$lib/components/admin/shop/studio/controls/SegmentedSpeed.svelte';
  import CosmeticName from '$lib/components/wardrobe/CosmeticName.svelte';
  import type { SubmitFunction } from '@sveltejs/kit';
  import { enhance } from '$app/forms';

  let {
    open = $bindable(false),
    enhanceHandler
  }: {
    open: boolean;
    enhanceHandler: SubmitFunction;
  } = $props();

  let mode = $state<'solid' | 'gradient' | 'glow'>('gradient');
  let name = $state('');
  let price = $state(200);
  let stock = $state<string | number>('');
  let validityPreset = $state<string>('0');
  let customValidityDays = $state<number>(30);
  let limit = $state<number>(1);
  let level = $state<number>(1);
  let color = $state('#f43f5e');
  let stops = $state<string[]>(['#f43f5e', '#f59e0b', '#8b5cf6']);
  let animate = $state('flow');
  let durationMs = $state(5000);
  let submitting = $state(false);

  const previewPresentation = $derived.by(() => {
    let style: Record<string, unknown>;
    if (mode === 'solid') {
      style = { mode: 'solid', color, animate, durationMs };
    } else if (mode === 'gradient') {
      style = { mode: 'gradient', colors: [...stops], animate, durationMs };
    } else {
      style = { mode: 'glow', color, animate, durationMs };
    }
    return {
      nickname_color_style: style,
      nickname_color: 'preview',
      nickname_color_name: name || '彩色昵称'
    };
  });

  const cosmeticJson = $derived.by(() => {
    let style: Record<string, unknown>;
    if (mode === 'solid') {
      style = { mode: 'solid', color, animate, durationMs };
    } else if (mode === 'gradient') {
      style = { mode: 'gradient', colors: [...stops], animate, durationMs };
    } else {
      style = { mode: 'glow', color, animate, durationMs };
    }
    return JSON.stringify({
      kind: 'nickname_color',
      name: name.trim() || '未命名彩色昵称',
      style
    });
  });

  const productJson = $derived.by(() => {
    let validity_seconds: number | null = null;
    if (validityPreset === 'custom') {
      const days = Number(customValidityDays) || 0;
      if (days > 0) validity_seconds = days * 86400;
    } else {
      const days = Number(validityPreset) || 0;
      if (days > 0) validity_seconds = days * 86400;
    }

    const stockNum = stock === '' || stock === null ? null : Number(stock);
    const stock_remaining = stockNum != null && stockNum > 0 ? stockNum : null;

    return JSON.stringify({
      title: name.trim() || '未命名彩色昵称',
      unit_price: Number(price) || 0,
      stock_remaining,
      validity_seconds,
      quantity_limit: Math.max(1, Number(limit) || 1),
      required_level: Math.max(1, Number(level) || 1),
      status: 'published',
      refund_policy: 'non_refundable'
    });
  });
</script>

<Dialog
  bind:open
  title="创建并上架彩色昵称"
  description="配置昵称渐变色或发光微光，起个好听的名字与售价，即可直接上架到积分商城。"
  size="md"
>
  <form
    method="POST"
    action="?/quickPublish"
    use:enhance={async (opts) => {
      submitting = true;
      const fn = await enhanceHandler(opts);
      return async (ctx) => {
        submitting = false;
        if (typeof fn === 'function') await fn(ctx);
      };
    }}
  >
    <input type="hidden" name="cosmetic" value={cosmeticJson} />
    <input type="hidden" name="product" value={productJson} />

    <div style="display:flex;flex-direction:column;gap:var(--space-3);padding:var(--space-2) 0;">
      <!-- 预览小舞台 -->
      <div style="padding:14px;border:1px solid var(--color-border);border-radius:8px;background:var(--color-bg-subtle);display:flex;flex-direction:column;align-items:center;gap:6px;">
        <span style="font-size:11px;color:var(--color-text-tertiary);font-weight:600;">实时效果试穿预览</span>
        <div style="font-size:18px;font-weight:700;">
          <CosmeticName name={name.trim() || 'BBLBB 用户昵称'} presentation={previewPresentation} />
        </div>
      </div>

      <!-- 名称与价格 -->
      <div style="display:grid;grid-template-columns:2fr 1fr;gap:var(--space-2);">
        <div class="input-wrapper">
          <label class="input-label" for="nc-name">装扮商品名称 *</label>
          <input
            id="nc-name"
            class="input-field"
            type="text"
            required
            maxlength="32"
            placeholder="例如: 极光流光昵称"
            bind:value={name}
            disabled={submitting}
          />
        </div>
        <div class="input-wrapper">
          <label class="input-label" for="nc-price">售价 (金币) *</label>
          <input
            id="nc-price"
            class="input-field"
            type="number"
            min="0"
            required
            bind:value={price}
            disabled={submitting}
          />
        </div>
      </div>

      <!-- 库存与有效期 -->
      <div style="display:grid;grid-template-columns:1fr 1fr;gap:var(--space-2);">
        <div class="input-wrapper">
          <label class="input-label" for="nc-stock">上架数量 (库存)</label>
          <input
            id="nc-stock"
            class="input-field"
            type="number"
            min="0"
            placeholder="留空为不限数量"
            bind:value={stock}
            disabled={submitting}
          />
        </div>
        <div class="input-wrapper">
          <label class="input-label" for="nc-validity">有效期限</label>
          <select id="nc-validity" class="input-field select-field" bind:value={validityPreset} disabled={submitting}>
            <option value="0">永久有效 (默认)</option>
            <option value="7">7 天 (体验版)</option>
            <option value="30">30 天 (月度卡)</option>
            <option value="90">90 天 (季度卡)</option>
            <option value="365">365 天 (年度卡)</option>
            <option value="custom">自定义天数...</option>
          </select>
          {#if validityPreset === 'custom'}
            <div style="display:flex;align-items:center;gap:6px;margin-top:6px;">
              <input type="number" class="input-field" min="1" max="3650" placeholder="输入天数" bind:value={customValidityDays} disabled={submitting} style="flex:1;" />
              <span style="font-size:12px;color:var(--color-text-secondary);">天</span>
            </div>
          {/if}
        </div>
      </div>

      <!-- 限购与等级门槛 -->
      <div style="display:grid;grid-template-columns:1fr 1fr;gap:var(--space-2);">
        <div class="input-wrapper">
          <label class="input-label" for="nc-limit">每人限购数量</label>
          <input
            id="nc-limit"
            class="input-field"
            type="number"
            min="1"
            max="999"
            placeholder="默认 1"
            bind:value={limit}
            disabled={submitting}
          />
        </div>
        <div class="input-wrapper">
          <label class="input-label" for="nc-level">最低购买等级门槛 (TL0-4)</label>
          <input
            id="nc-level"
            class="input-field"
            type="number"
            min="1"
            max="10"
            placeholder="默认 1"
            bind:value={level}
            disabled={submitting}
          />
        </div>
      </div>

      <!-- 类型切换 -->
      <div class="input-wrapper">
        <span class="input-label">颜色模式</span>
        <div style="display:flex;gap:var(--space-2);">
          <button
            type="button"
            class="btn sm"
            class:primary={mode === 'gradient'}
            onclick={() => { mode = 'gradient'; animate = 'flow'; }}
          >
            渐变流动
          </button>
          <button
            type="button"
            class="btn sm"
            class:primary={mode === 'glow'}
            onclick={() => { mode = 'glow'; animate = 'breathe'; }}
          >
            呼吸发光
          </button>
          <button
            type="button"
            class="btn sm"
            class:primary={mode === 'solid'}
            onclick={() => { mode = 'solid'; animate = 'none'; }}
          >
            纯色微动
          </button>
        </div>
      </div>

      {#if mode === 'gradient'}
        <div class="input-wrapper">
          <span class="input-label">渐变颜色色标</span>
          <GradientEditor bind:stops idPrefix="q-grad" disabled={submitting} />
        </div>
        <div style="display:grid;grid-template-columns:1fr 1fr;gap:var(--space-2);">
          <div class="input-wrapper">
            <label class="input-label" for="nc-grad-anim">流动动效</label>
            <select id="nc-grad-anim" class="input-field" bind:value={animate} disabled={submitting}>
              <option value="flow">平滑流动</option>
              <option value="wave">波浪起伏</option>
              <option value="shimmer">流光闪烁</option>
              <option value="glitch">赛博故障</option>
              <option value="rainbow">彩虹变幻</option>
              <option value="none">静止渐变</option>
            </select>
          </div>
          <div class="input-wrapper">
            <span class="input-label">流动周期</span>
            <SegmentedSpeed bind:value={durationMs} idPrefix="q-grad-spd" disabled={submitting} />
          </div>
        </div>
      {:else}
        <div class="input-wrapper">
          <label class="input-label" for="nc-color">主体色彩</label>
          <input id="nc-color" type="color" bind:value={color} disabled={submitting} style="width:50px;height:34px;padding:0;border:1px solid var(--color-border);border-radius:6px;background:none;cursor:pointer;" />
        </div>
        <div style="display:grid;grid-template-columns:1fr 1fr;gap:var(--space-2);">
          <div class="input-wrapper">
            <label class="input-label" for="nc-glow-anim">动效模式</label>
            <select id="nc-glow-anim" class="input-field" bind:value={animate} disabled={submitting}>
              {#if mode === 'glow'}
                <option value="breathe">呼吸微光</option>
                <option value="flicker">霓虹闪烁</option>
                <option value="fire">烈焰微光</option>
                <option value="pulse">快速脉冲</option>
                <option value="none">静止发光</option>
              {:else}
                <option value="none">静止纯色</option>
                <option value="breathe">轻微呼吸</option>
                <option value="pulse">脉冲跳动</option>
                <option value="bounce">轻微律动</option>
              {/if}
            </select>
          </div>
          {#if animate !== 'none'}
            <div class="input-wrapper">
              <span class="input-label">动画周期</span>
              <SegmentedSpeed bind:value={durationMs} idPrefix="q-glow-spd" disabled={submitting} />
            </div>
          {/if}
        </div>
      {/if}
    </div>

    <div style="display:flex;justify-content:flex-end;gap:var(--space-2);margin-top:var(--space-4);padding-top:var(--space-3);border-top:1px solid var(--color-border);">
      <Button text="取消" variant="secondary" size="sm" type="button" onclick={() => (open = false)} disabled={submitting} />
      <Button text={submitting ? "正在上架…" : "立即上架到商城"} variant="primary" size="sm" type="submit" disabled={submitting || !name.trim()} />
    </div>
  </form>
</Dialog>
