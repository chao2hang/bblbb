<!-- M07-UI-08：管理端商城——商品列表/新建/编辑/发布/停售 + 订单/退款。
  版本冲突（409）提示刷新；退款要求 reason 必填；后端裁决 403/501/5xx 状态。
-->
<script lang="ts">
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import { adminStateLabel } from '$lib/admin';
  import { productKindLabel, productStatusLabel } from '$lib/api/client';
  import Button from '$lib/components/ui/Button.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import { withActionToast, toastActionResult } from '$lib/ui/action-toast';
  import type { ShopProduct, ShopOrder } from '$lib/api/types';
  import type { AdminShopActionData, AdminShopPageData } from './+page.server';

  let { data, form }: { data: AdminShopPageData; form?: AdminShopActionData | null } = $props();

  /** 退款策略本地化（配置行展示用；表单 option 已是中文）。 */
  function refundPolicyLabel(policy: string): string {
    switch (policy) {
      case 'non_refundable':
        return '不可退款';
      case 'compensation_only':
        return '仅补偿';
      case 'refundable':
        return '支持退款';
      default:
        return policy;
    }
  }

  const products = $derived(data.products);
  const orders = $derived(data.orders);
  const config = $derived(data.config);
  const message = $derived(form?.message ?? null);
  const fieldErrors = $derived(form?.fieldErrors ?? {});

  let creating = $state(false);

  // JS 启用：动作结果走全局 Toast 浮窗（成功绿/失败红）；顶部内联横幅仅保留为
  // 无 JS 回退（SSR HTML 仍渲染，见 hasJs）。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  /** 展开的编辑表单（product id）。 */
  let editing = $state<string | null>(null);

  const PRODUCT_KINDS = [
    'cosmetic_nickname',
    'cosmetic_avatar',
    'cosmetic_avatar_attachment',
    'cosmetic_badge',
    'profile_effect',
    'post_effect',
    'reaction_pack',
    'title_prefix',
    'utility'
  ] as const;

  function formatTs(ts: number | undefined): string {
    if (!ts) return '—';
    return new Date(ts).toLocaleString('zh-CN', { hour12: false });
  }
</script>

<svelte:head>
  <title>商城管理 — BBLBB</title>
</svelte:head>

<PageHeader title="商城管理" />

<div class="container page-content">

  {#if message && !hasJs}
    <p class="input-hint is-error" role="alert">{message}</p>
  {/if}

  {#if config.state === 'ok'}
    <div class="app-card" style="margin-bottom:var(--space-4);">
      <div class="card-body" style="display:flex;flex-wrap:wrap;gap:var(--space-4);align-items:center;">
        <span class="badge {config.data.enabled === false ? 'badge-warning' : 'badge-success'}">
          {config.data.enabled === false ? '商城停用' : '商城启用'}
        </span>
        <span class="text-secondary" style="font-size:var(--text-sm);">
          结算货币 {(config.data.currency_id ?? 'coin').toUpperCase()} · 默认退款策略 {refundPolicyLabel(config.data.default_refund_policy ?? 'non_refundable')}
        </span>
      </div>
    </div>
  {/if}

  <div class="app-card" style="margin-bottom:var(--space-4);">
    <div class="app-card__head"><h2>新建商品</h2></div>
    <div class="app-card__body">
      <form
        method="POST"
        action="?/create"
        use:enhance={({ cancel }) => {
          if (creating) {
            cancel();
            return async () => {};
          }
          creating = true;
          return async ({ result, update }) => {
            creating = false;
            await update();
            // 动作结果 → 全局 Toast（成功“商品「x」已创建”/ 失败服务端文案）；
            // 字段级错误仍以表内 fieldErrors 提示为主。顶部横幅为无 JS 回退。
            toastActionResult(result);
          };
        }}
      >
        <input type="hidden" name="currency_id" value="coin" />
        <div class="admin-form-grid" style="display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:var(--space-2);">
          <div class="input-wrapper">
            <label class="input-label" for="np-title">标题 *</label>
            <input id="np-title" name="title" class="input-field" required />
            {#if fieldErrors.title}
              <span class="app-field-error" role="alert" style="color:var(--color-danger);font-size:11px;display:block;margin-top:2px;">{fieldErrors.title}</span>
            {/if}
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="np-kind">类型 *</label>
            <select id="np-kind" name="kind" class="input-field">
              {#each PRODUCT_KINDS as kind}
                <option value={kind}>{productKindLabel(kind)}</option>
              {/each}
            </select>
            {#if fieldErrors.kind}
              <span class="app-field-error" role="alert" style="color:var(--color-danger);font-size:11px;display:block;margin-top:2px;">{fieldErrors.kind}</span>
            {/if}
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="np-slug">slug *</label>
            <input id="np-slug" name="slug" class="input-field" required pattern="[a-z0-9-]+" />
            {#if fieldErrors.slug}
              <span class="app-field-error" role="alert" style="color:var(--color-danger);font-size:11px;display:block;margin-top:2px;">{fieldErrors.slug}</span>
            {/if}
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="np-price">价格（coin）*</label>
            <input id="np-price" name="unit_price" type="number" min="0" step="1" class="input-field" required />
            {#if fieldErrors.unit_price}
              <span class="app-field-error" role="alert" style="color:var(--color-danger);font-size:11px;display:block;margin-top:2px;">{fieldErrors.unit_price}</span>
            {/if}
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="np-stock">库存（空=不限）</label>
            <input id="np-stock" name="stock_remaining" type="number" min="0" class="input-field" placeholder="不限" />
            {#if fieldErrors.stock_remaining}
              <span class="app-field-error" role="alert" style="color:var(--color-danger);font-size:11px;display:block;margin-top:2px;">{fieldErrors.stock_remaining}</span>
            {/if}
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="np-level">等级门槛</label>
            <input id="np-level" name="required_level" type="number" min="1" value="1" class="input-field" />
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="np-limit">限购</label>
            <input id="np-limit" name="quantity_limit" type="number" min="1" value="1" class="input-field" />
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="np-slot">展示槽位 *</label>
            <input id="np-slot" name="slot" value="avatar_frame" class="input-field" required placeholder="avatar_frame / profile_badges…" />
            {#if fieldErrors.slot}
              <span class="app-field-error" role="alert" style="color:var(--color-danger);font-size:11px;display:block;margin-top:2px;">{fieldErrors.slot}</span>
            {/if}
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="np-icon">图标 Token</label>
            <input id="np-icon" name="icon_token" class="input-field" placeholder="star / shopping-bag…" />
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="np-validity">有效期（秒，空=永久）</label>
            <input id="np-validity" name="validity_seconds" type="number" min="0" class="input-field" placeholder="永久" />
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="np-desc">说明</label>
            <input id="np-desc" name="description_safe" class="input-field" />
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="np-refund">退款策略</label>
            <select id="np-refund" name="refund_policy" class="input-field">
              <option value="non_refundable">不可退款</option>
              <option value="compensation_only">仅补偿</option>
              <option value="full_refund">可退款</option>
            </select>
          </div>
        </div>
        <div class="input-wrapper" style="margin-top:var(--space-2);">
          <label class="input-label" for="np-reason">操作原因 *</label>
          <input id="np-reason" name="reason" class="input-field" required placeholder="必填（写审计）" />
          {#if fieldErrors.reason}
            <span class="app-field-error" role="alert" style="color:var(--color-danger);font-size:11px;display:block;margin-top:2px;">{fieldErrors.reason}</span>
          {/if}
        </div>
        <Button text={creating ? '创建中…' : '创建商品'} variant="primary" size="sm" type="submit" disabled={creating} extraClass="mt-2" />
      </form>
    </div>
  </div>

  <div class="app-card" style="margin-bottom:var(--space-4);">
    <div class="app-card__head"><h2>商品（{products.state === 'ok' ? products.items.length : '—'}）</h2></div>
    <div class="card-body" style="padding:0;">
      {#if products.state !== 'ok'}
        <p class="input-hint is-error" role="alert" style="padding:var(--space-4);">
          {products.state === 'forbidden' || products.state === 'not_implemented' || products.state === 'error'
            ? adminStateLabel(products.state)
            : '加载失败'}
          {#if products.state === 'forbidden' || products.state === 'error' || products.state === 'not_implemented'}
            ：{products.message}
          {/if}
        </p>
      {:else if products.items.length === 0}
        <div style="padding:var(--space-4);"><EmptyState icon="package" title="暂无商品" /></div>
      {:else}
        <div style="display:flex;flex-direction:column;">
          {#each products.items as p (p.id)}
            <div class="post-row" style="padding:var(--space-3);border-bottom:var(--border-default);">
              <div style="display:flex;gap:var(--space-3);align-items:center;flex-wrap:wrap;">
                <div style="min-width:0;flex:1;">
                  <strong>{p.title}</strong>
                  <span class="badge badge-neutral" style="margin-left:var(--space-2);">{productKindLabel(p.kind)}</span>
                  <span class="badge {p.status === 'published' ? 'badge-success' : p.status === 'disabled' ? 'badge-warning' : 'badge-neutral'}">
                    {productStatusLabel(p.status)}
                  </span>
                  <p class="text-secondary" style="font-size:var(--text-xs);margin:2px 0 0;">
                    {p.slug} · {p.unit_price} {p.currency.toUpperCase()} · v{p.version} · 更新于 {formatTs(p.updated_at)}
                  </p>
                </div>
                <div style="display:flex;gap:var(--space-2);flex-wrap:wrap;">
                  {#if p.status !== 'published'}
                    <form method="POST" action="?/publish" use:enhance={withActionToast()}>
                      <input type="hidden" name="id" value={p.id} />
                      <Button text="发布" variant="secondary" size="sm" type="submit" />
                    </form>
                  {/if}
                  {#if p.status === 'published'}
                    <form method="POST" action="?/disable" use:enhance={withActionToast()} style="display:flex;gap:var(--space-2);align-items:center;">
                      <input type="hidden" name="id" value={p.id} />
                      <input name="reason" class="input-field" style="width:150px;" required placeholder="停售原因" aria-label="停售原因" />
                      <Button text="停售" variant="ghost" size="sm" type="submit" />
                    </form>
                  {/if}
                  <Button text={editing === p.id ? '收起' : '编辑'} variant="ghost" size="sm" type="button" onclick={() => (editing = editing === p.id ? null : p.id)} />
                </div>
              </div>
              {#if editing === p.id}
                <form method="POST" action="?/update" use:enhance={withActionToast()} style="margin-top:var(--space-3);">
                  <input type="hidden" name="id" value={p.id} />
                  <input type="hidden" name="version" value={p.version} />
                  <div class="admin-form-grid" style="display:grid;grid-template-columns:repeat(auto-fit,minmax(160px,1fr));gap:var(--space-2);">
                    <div class="input-wrapper">
                      <label class="input-label" for="up-title-{p.id}">标题</label>
                      <input id="up-title-{p.id}" name="title" class="input-field" value={p.title} />
                    </div>
                    <div class="input-wrapper">
                      <label class="input-label" for="up-price-{p.id}">价格</label>
                      <input id="up-price-{p.id}" name="unit_price" type="number" min="0" class="input-field" value={p.unit_price} />
                    </div>
                    <div class="input-wrapper">
                      <label class="input-label" for="up-stock-{p.id}">库存（空=不限）</label>
                      <input id="up-stock-{p.id}" name="stock_remaining" type="number" min="0" class="input-field" value={p.stock_remaining ?? ''} />
                    </div>
                    <div class="input-wrapper">
                      <label class="input-label" for="up-level-{p.id}">等级门槛</label>
                      <input id="up-level-{p.id}" name="required_level" type="number" min="1" class="input-field" value={p.required_level} />
                    </div>
                    <div class="input-wrapper">
                      <label class="input-label" for="up-limit-{p.id}">限购</label>
                      <input id="up-limit-{p.id}" name="quantity_limit" type="number" min="1" class="input-field" value={p.quantity_limit} />
                    </div>
                    <div class="input-wrapper">
                      <label class="input-label" for="up-slot-{p.id}">展示槽位</label>
                      <input id="up-slot-{p.id}" name="slot" class="input-field" value={p.slot ?? ''} />
                    </div>
                    <div class="input-wrapper">
                      <label class="input-label" for="up-validity-{p.id}">有效期（秒）</label>
                      <input id="up-validity-{p.id}" name="validity_seconds" type="number" min="0" class="input-field" value={p.validity_seconds ?? ''} />
                    </div>
                  </div>
                  <div class="input-wrapper" style="margin-top:var(--space-2);">
                    <label class="input-label" for="up-reason-{p.id}">操作原因</label>
                    <input id="up-reason-{p.id}" name="reason" class="input-field" required placeholder="必填（写审计）" />
                  </div>
                  <Button text="保存（v{p.version + 1}）" variant="primary" size="sm" type="submit" />
                </form>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  <div class="app-card">
    <div class="app-card__head"><h2>订单（{orders.state === 'ok' ? orders.items.length : '—'}）</h2></div>
    <div class="card-body" style="padding:0;">
      {#if orders.state !== 'ok'}
        <p class="input-hint is-error" role="alert" style="padding:var(--space-4);">
          {orders.state === 'forbidden' || orders.state === 'not_implemented' || orders.state === 'error'
            ? adminStateLabel(orders.state)
            : '加载失败'}
        </p>
      {:else if orders.items.length === 0}
        <div style="padding:var(--space-4);"><EmptyState icon="inbox" title="暂无订单" /></div>
      {:else}
        <div style="display:flex;flex-direction:column;">
          {#each orders.items as o (o.id)}
            <div class="post-row" style="padding:var(--space-3);border-bottom:var(--border-default);">
              <div style="display:flex;gap:var(--space-3);align-items:center;flex-wrap:wrap;">
                <div style="min-width:0;flex:1;">
                  <strong>{o.product_title ?? o.product_id}</strong>
                  <span class="badge {o.status === 'succeeded' ? 'badge-success' : 'badge-neutral'}">{o.status}</span>
                  {#if o.entitlement_status === 'pending'}
                    <span class="badge badge-warning">补偿待处理</span>
                  {/if}
                  <p class="text-secondary" style="font-size:var(--text-xs);margin:2px 0 0;">
                    {o.id} · ×{o.quantity} · {o.total_amount} {o.currency.toUpperCase()} · v{o.product_version} · {formatTs(o.created_at)}
                  </p>
                </div>
                {#if o.status === 'succeeded'}
                  <form method="POST" action="?/refund" use:enhance={withActionToast()} style="display:flex;gap:var(--space-2);align-items:center;flex-wrap:wrap;">
                    <input type="hidden" name="id" value={o.id} />
                    <input name="amount" type="number" min="0" class="input-field" style="width:110px;" placeholder="全额/空" aria-label="退款金额（空=全额）" />
                    <input name="reason" class="input-field" style="width:180px;" required placeholder="退款原因（必填）" aria-label="退款原因" />
                    <Button text="退款" variant="ghost" size="sm" type="submit" />
                  </form>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</div>
