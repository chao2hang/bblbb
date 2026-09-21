<!-- M07-SHOP-STUDIO R2：发布面板（名称/定价/时效/退款/售卖窗口）。
  R2 重构：字段按成熟电商后台（Shopify 商品编辑器）习惯分区 ——
  商品信息 / 价格与库存 / 时效与售卖 / 合规与审计；末尾为常驻操作栏
  （缺失项清单 + 提交按钮，面板内 sticky —— 主操作永不滚出视野）。
  纯展示组件：值通过 bind:value 与父页面共享，父页面负责表单提交
  （SvelteKit form action → 复合发布端点）。slug 未手动修改时随标题
  自动派生（与后端 slugify 同规则），手动编辑后保持用户输入。 -->
<script lang="ts">
  import AttachmentUploader from '$lib/components/upload/AttachmentUploader.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { formatMoney } from '$lib/api/client';
  import { slugify } from './slugify';
  import { VALIDITY_OPTIONS, formatValidity } from './duration';
  import {
    REFUND_POLICY_OPTIONS,
    publishValiditySeconds,
    type PublishDraft,
    type PublishDraftErrors
  } from './publish-draft';
  import type { StyleKind } from './style-draft';

  let {
    value = $bindable(),
    kind,
    errors = {},
    idPrefix = 'pub',
    disabled = false,
    submitting = false,
    checklist = [],
    submitBlocked = false,
    priceHint = null
  }: {
    value: PublishDraft;
    kind: StyleKind;
    errors?: PublishDraftErrors;
    idPrefix?: string;
    disabled?: boolean;
    submitting?: boolean;
    /** 发布前缺失项实时清单（JS 增强；空数组 = 可提交）。 */
    checklist?: string[];
    /** JS 侧阻断提交（checklist 非空时由父页面传入；无 JS 不阻断，服务端 422 回显）。 */
    submitBlocked?: boolean;
    /** 该类型的建议价（金币；展示在价格提示行，可改）。 */
    priceHint?: number | null;
  } = $props();

  // 标题变化时自动派生 slug（用户手动改过 slug 后停止跟随）。
  // 用 $derived 而非 $effect：$effect 在 SSR 渲染下抛 effect_orphan；
  // 派生值直接喂给输入框，提交时即当前展示值。
  const slugValue = $derived(value.slugTouched ? value.slug : slugify(value.title));

  const validitySeconds = $derived(publishValiditySeconds(value));
  const pricePreview = $derived(formatMoney(Math.max(0, Math.round(value.unitPrice || 0)), { code: 'coin' }, { free: true }));
</script>

<div class="publish-panel">
  <input type="hidden" name="validity_choice" value={value.validityChoice} />
  <input type="hidden" name="asset_attachment_id" value={value.assetAttachmentId} />

  <!-- 分区一：商品信息 -->
  <section class="pp-section">
    <h4 class="pp-section__title">商品信息</h4>
    {#if kind === 'avatar_frame'}
      <div class="input-wrapper">
        <span class="input-label">头像框来源</span>
        <div class="pp-chip-row" role="radiogroup" aria-label="头像框来源">
          <button
            type="button"
            class="chip"
            class:is-active={!value.assetAttachmentId}
            role="radio"
            aria-checked={!value.assetAttachmentId}
            {disabled}
            onclick={() => (value.assetAttachmentId = '')}
          >
            纯样式边框（左侧设计）
          </button>
          <button
            type="button"
            class="chip"
            class:is-active={Boolean(value.assetAttachmentId)}
            role="radio"
            aria-checked={Boolean(value.assetAttachmentId)}
            {disabled}
            onclick={() => {
              const fileInput = document.querySelector<HTMLInputElement>('.publish-panel input[type="file"]');
              fileInput?.click();
            }}
          >
            自定义图片（PNG / APNG）
          </button>
        </div>
        <p class="input-hint">
          透明背景 PNG，建议 256×256 以上；动效边框请上传 APNG（动图 PNG），用户侧循环播放。
        </p>
        <AttachmentUploader
          accept="image/png,.png"
          maxBytes={4 * 1024 * 1024}
          waitReady={true}
          showQuota={false}
          autoUpload={true}
          label="上传头像框图片（PNG / APNG，≤4MB）"
          onReady={(a) => (value.assetAttachmentId = a.id)}
        />
        {#if value.assetAttachmentId}
          <small class="input-hint">已绑定图片附件：{value.assetAttachmentId}
            <button type="button" class="pp-clear" {disabled} onclick={() => (value.assetAttachmentId = '')}>移除</button>
          </small>
        {/if}
        {#if errors.asset_attachment_id}<p class="input-error">{errors.asset_attachment_id}</p>{/if}
      </div>
    {/if}

    <div class="input-wrapper">
      <label class="input-label" for="{idPrefix}-title">商品名称 *</label>
      <input id="{idPrefix}-title" name="title" class="input-field" bind:value={value.title} {disabled} required maxlength="120" placeholder="例如：樱花粉渐变昵称" />
      {#if errors.title}<p class="input-error">{errors.title}</p>{/if}
    </div>

    <div class="input-wrapper">
      <label class="input-label" for="{idPrefix}-slug">slug（留空由名称派生）</label>
      <input
        id="{idPrefix}-slug"
        name="slug"
        class="input-field"
        value={slugValue}
        {disabled}
        pattern="[a-z0-9]+(-[a-z0-9]+)*"
        oninput={(e) => {
          value.slugTouched = true;
          value.slug = e.currentTarget.value;
        }}
      />
      {#if errors.slug}<p class="input-error">{errors.slug}</p>{/if}
    </div>

    <div class="input-wrapper">
      <label class="input-label" for="{idPrefix}-desc">商品说明（可选）</label>
      <textarea id="{idPrefix}-desc" name="description_safe" class="input-field" rows="2" maxlength="2000" bind:value={value.descriptionSafe} {disabled} placeholder="会在商品详情页展示"></textarea>
      {#if errors.description_safe}<p class="input-error">{errors.description_safe}</p>{/if}
    </div>
  </section>

  <!-- 分区二：价格与库存 -->
  <section class="pp-section">
    <h4 class="pp-section__title">价格与库存</h4>
    <div class="pp-grid">
      <div class="input-wrapper">
        <label class="input-label" for="{idPrefix}-price">价格（金币）*</label>
        <input id="{idPrefix}-price" name="unit_price" type="number" min="0" step="1" class="input-field" bind:value={value.unitPrice} {disabled} required />
        <p class="input-hint">前台展示：{pricePreview}{#if priceHint} · 此类装扮建议 {priceHint} 金币{/if}</p>
        {#if errors.unit_price}<p class="input-error">{errors.unit_price}</p>{/if}
      </div>
      <div class="input-wrapper">
        <label class="input-label" for="{idPrefix}-stock">库存</label>
        <input
          id="{idPrefix}-stock"
          name="stock_remaining"
          type="number"
          min="0"
          step="1"
          class="input-field"
          value={value.stockRemaining ?? ''}
          placeholder="不限"
          {disabled}
          oninput={(e) => {
            const raw = e.currentTarget.value;
            value.stockRemaining = raw === '' ? null : Math.max(0, Math.round(Number(raw)));
          }}
        />
        {#if errors.stock_remaining}<p class="input-error">{errors.stock_remaining}</p>{/if}
      </div>
      <div class="input-wrapper">
        <label class="input-label" for="{idPrefix}-level">等级门槛</label>
        <input id="{idPrefix}-level" name="required_level" type="number" min="1" step="1" class="input-field" bind:value={value.requiredLevel} {disabled} />
        {#if errors.required_level}<p class="input-error">{errors.required_level}</p>{/if}
      </div>
      <div class="input-wrapper">
        <label class="input-label" for="{idPrefix}-limit">每人限购</label>
        <input id="{idPrefix}-limit" name="quantity_limit" type="number" min="1" step="1" class="input-field" bind:value={value.quantityLimit} {disabled} />
        {#if errors.quantity_limit}<p class="input-error">{errors.quantity_limit}</p>{/if}
      </div>
    </div>
  </section>

  <!-- 分区三：时效与售卖窗口 -->
  <section class="pp-section">
    <h4 class="pp-section__title">时效与售卖</h4>
    <div class="input-wrapper">
      <span class="input-label">时效</span>
      <div class="pp-chip-row" role="radiogroup" aria-label="时效">
        {#each VALIDITY_OPTIONS as opt (opt.value)}
          <button
            type="button"
            class="chip"
            class:is-active={value.validityChoice === opt.value}
            role="radio"
            aria-checked={value.validityChoice === opt.value}
            {disabled}
            onclick={() => (value.validityChoice = opt.value)}
          >
            {opt.label}
          </button>
        {/each}
        <button
          type="button"
          class="chip"
          class:is-active={value.validityChoice === 'custom'}
          role="radio"
          aria-checked={value.validityChoice === 'custom'}
          {disabled}
          onclick={() => (value.validityChoice = 'custom')}
        >
          自定义
        </button>
      </div>
      {#if value.validityChoice === 'custom'}
        <div class="pp-custom-days">
          <input
            type="number"
            min="1"
            step="1"
            class="input-field"
            name="custom_days" bind:value={value.customDays}
            {disabled}
            aria-label="自定义天数"
          />
          <span class="input-hint">天</span>
        </div>
      {/if}
      <p class="input-hint">购买后 {formatValidity(validitySeconds)} 内有效{validitySeconds === null ? '' : '，到期自动失效'}</p>
      {#if errors.validity_seconds}<p class="input-error">{errors.validity_seconds}</p>{/if}
    </div>

    <div class="pp-grid">
      <div class="input-wrapper">
        <label class="input-label" for="{idPrefix}-sale-start">售卖开始（可选）</label>
        <input id="{idPrefix}-sale-start" name="sale_start_at" type="datetime-local" class="input-field" bind:value={value.saleStartAt} {disabled} />
      </div>
      <div class="input-wrapper">
        <label class="input-label" for="{idPrefix}-sale-end">售卖结束（可选）</label>
        <input id="{idPrefix}-sale-end" name="sale_end_at" type="datetime-local" class="input-field" bind:value={value.saleEndAt} {disabled} />
      </div>
    </div>
    {#if errors.sale_window}<p class="input-error">{errors.sale_window}</p>{/if}
  </section>

  <!-- 分区四：合规与审计 -->
  <section class="pp-section">
    <h4 class="pp-section__title">合规与审计</h4>
    <div class="pp-grid">
      <div class="input-wrapper">
        <label class="input-label" for="{idPrefix}-refund">退款策略</label>
        <select id="{idPrefix}-refund" name="refund_policy" class="input-field" bind:value={value.refundPolicy} {disabled}>
          {#each REFUND_POLICY_OPTIONS as o (o.value)}
            <option value={o.value}>{o.label}</option>
          {/each}
        </select>
      </div>
      <div class="input-wrapper">
        <label class="input-label" for="{idPrefix}-status">发布状态</label>
        <select id="{idPrefix}-status" name="status" class="input-field" bind:value={value.status} {disabled}>
          <option value="published">直接上架</option>
          <option value="draft">存为草稿</option>
        </select>
      </div>
    </div>
    <div class="input-wrapper">
      <label class="input-label" for="{idPrefix}-reason">操作原因 *</label>
      <input id="{idPrefix}-reason" name="reason" class="input-field" bind:value={value.reason} {disabled} required placeholder="必填（写审计）" />
      {#if errors.reason}<p class="input-error">{errors.reason}</p>{/if}
    </div>
  </section>

  <!-- 常驻操作栏：缺失项清单 + 提交（面板内 sticky，主操作永不滚出视野） -->
  <div class="pp-actionbar">
    {#if checklist.length > 0}
      <div class="pp-checklist" role="status">
        <span class="pp-checklist__title">还差 {checklist.length} 项：</span>
        {#each checklist as item (item)}
          <span class="pp-checklist__item">{item}</span>
        {/each}
      </div>
    {:else}
      <p class="pp-ready" role="status"><Icon name="check-circle" size={13} /> 信息完整，可以发布</p>
    {/if}
    <Button
      text={submitting ? '发布中…' : value.status === 'draft' ? '保存草稿商品' : '设计完成，发布商品'}
      variant="primary"
      size="sm"
      type="submit"
      disabled={disabled || submitting || submitBlocked}
    />
  </div>
</div>

<style>
  .publish-panel {
    display: flex;
    flex-direction: column;
  }
  .pp-section {
    padding: 12px 0 14px;
    border-bottom: 1px solid var(--color-border);
  }
  .pp-section:first-of-type {
    padding-top: 2px;
  }
  .pp-section__title {
    margin: 0 0 2px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--color-text-tertiary);
  }
  .pp-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: var(--space-2, 8px);
  }
  .pp-chip-row {
    display: flex;
    gap: var(--space-2, 8px);
    flex-wrap: wrap;
  }
  .pp-custom-days {
    display: flex;
    align-items: center;
    gap: var(--space-2, 8px);
    margin-top: var(--space-2, 8px);
    max-width: 160px;
  }
  /* 常驻操作栏：面板内 sticky（出血铺满卡片底部，遮住滚过的字段） */
  .pp-actionbar {
    position: sticky;
    bottom: 0;
    z-index: 2;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    flex-wrap: wrap;
    margin: 14px -16px -16px;
    padding: 12px 16px;
    border-top: 1px solid var(--color-border);
    border-radius: 0 0 var(--radius-md, 8px) var(--radius-md, 8px);
    background: var(--color-bg-card);
    box-shadow: 0 -8px 16px -12px rgba(0, 0, 0, 0.2);
  }
  .pp-checklist {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    font-size: var(--text-xs, 12px);
  }
  .pp-checklist__title {
    color: var(--color-warning);
    font-weight: 600;
  }
  .pp-checklist__item {
    background: var(--color-warning-soft);
    border: 1px solid color-mix(in srgb, var(--color-warning) 30%, transparent);
    color: var(--color-warning);
    border-radius: var(--radius-sm, 4px);
    padding: 1px 8px;
  }
  .pp-ready {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: var(--text-xs, 12px);
    font-weight: 600;
    color: var(--color-success);
  }
  .pp-clear {
    border: none;
    background: none;
    color: var(--color-brand);
    cursor: pointer;
    font-size: inherit;
    padding: 0 2px;
  }
  .input-error {
    color: var(--color-danger);
    font-size: var(--text-xs, 12px);
    margin-top: var(--space-1, 4px);
  }
  .chip {
    padding: 6px 14px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm, 6px);
    background: var(--color-bg-card);
    color: var(--color-text-secondary);
    cursor: pointer;
    font-size: var(--text-sm, 13px);
    transition: border-color 120ms ease, background 120ms ease, color 120ms ease;
  }
  .chip:hover {
    border-color: var(--color-border-strong);
    background: var(--color-surface-hover);
    color: var(--color-text-primary);
  }
  .chip.is-active {
    border-color: var(--color-brand);
    color: var(--color-brand);
    background: var(--color-brand-soft);
  }
  .chip:disabled { opacity: 0.6; cursor: not-allowed; }
  @media (prefers-reduced-motion: reduce) {
    .chip { transition: none; }
  }
  /* 单列文档流下（工作台窄屏断点）取消面板内 sticky，避免覆盖卡片内容 */
  @media (max-width: 1120px) {
    .pp-actionbar {
      position: static;
    }
  }
</style>
