<!-- M07-UI-08：管理端商城——商品列表/新建/编辑/发布/停售 + 订单/退款。
  版本冲突（409）提示刷新；退款要求 reason 必填；后端裁决 403/501/5xx 状态。
-->
<script lang="ts">
  import { enhance } from '$app/forms';
  import { adminStateLabel } from '$lib/admin';
  import { productKindLabel, productStatusLabel } from '$lib/api/client';
  import Button from '$lib/components/ui/Button.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import type { ShopProduct, ShopOrder } from '$lib/api/types';
  import type { AdminShopPageData } from './+page.server';

  let { data, form }: { data: AdminShopPageData; form?: { message?: string } | null } = $props();

  const products = $derived(data.products);
  const orders = $derived(data.orders);
  const config = $derived(data.config);
  const message = $derived(form?.message ?? null);

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

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <a href="/admin" class="breadcrumb-link">管理后台</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">商城管理</span>
  </nav>

  {#if message}
    <p class="input-hint is-error" role="alert">{message}</p>
  {/if}

  {#if config.state === 'ok'}
    <div class="card" style="margin-bottom:var(--space-4);">
      <div class="card-body" style="display:flex;flex-wrap:wrap;gap:var(--space-4);align-items:center;">
        <span class="badge {config.data.enabled === false ? 'badge-warning' : 'badge-success'}">
          {config.data.enabled === false ? '商城停用' : '商城启用'}
        </span>
        <span class="text-secondary" style="font-size:var(--text-sm);">
          结算货币 {config.data.currency_id ?? 'coin'.toUpperCase()} · 默认退款策略 {config.data.default_refund_policy ?? 'non_refundable'}
        </span>
      </div>
    </div>
  {/if}

  <div class="card" style="margin-bottom:var(--space-4);">
    <div class="card-header"><span class="card-title">新建商品</span></div>
    <div class="card-body">
      <form method="POST" action="?/create" use:enhance>
        <div class="admin-form-grid" style="display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:var(--space-2);">
          <div class="input-wrapper">
            <label class="input-label" for="np-title">标题</label>
            <input id="np-title" name="title" class="input-field" required />
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="np-kind">类型</label>
            <select id="np-kind" name="kind" class="input-field">
              {#each PRODUCT_KINDS as kind}
                <option value={kind}>{productKindLabel(kind)}</option>
              {/each}
            </select>
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="np-slug">slug</label>
            <input id="np-slug" name="slug" class="input-field" required pattern="[a-z0-9-]+" />
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="np-price">价格（coin）</label>
            <input id="np-price" name="unit_price" type="number" min="0" step="1" class="input-field" required />
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="np-stock">库存（空=不限）</label>
            <input id="np-stock" name="stock_remaining" type="number" min="0" class="input-field" placeholder="不限" />
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
            <label class="input-label" for="np-slot">展示槽位</label>
            <input id="np-slot" name="slot" class="input-field" placeholder="avatar_frame / profile_badges…" />
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
          <label class="input-label" for="np-reason">操作原因</label>
          <input id="np-reason" name="reason" class="input-field" required placeholder="必填（写审计）" />
        </div>
        <Button text="创建商品" variant="primary" size="sm" type="submit" extraClass="mt-2" />
      </form>
    </div>
  </div>

  <div class="card" style="margin-bottom:var(--space-4);">
    <div class="card-header"><span class="card-title">商品（{products.state === 'ok' ? products.items.length : '—'}）</span></div>
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
                    <form method="POST" action="?/publish" use:enhance>
                      <input type="hidden" name="id" value={p.id} />
                      <Button text="发布" variant="secondary" size="sm" type="submit" />
                    </form>
                  {/if}
                  {#if p.status === 'published'}
                    <form method="POST" action="?/disable" use:enhance style="display:flex;gap:var(--space-2);align-items:center;">
                      <input type="hidden" name="id" value={p.id} />
                      <input name="reason" class="input-field" style="width:150px;" required placeholder="停售原因" aria-label="停售原因" />
                      <Button text="停售" variant="ghost" size="sm" type="submit" />
                    </form>
                  {/if}
                  <Button text={editing === p.id ? '收起' : '编辑'} variant="ghost" size="sm" type="button" onclick={() => (editing = editing === p.id ? null : p.id)} />
                </div>
              </div>
              {#if editing === p.id}
                <form method="POST" action="?/update" use:enhance style="margin-top:var(--space-3);">
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

  <div class="card">
    <div class="card-header"><span class="card-title">订单（{orders.state === 'ok' ? orders.items.length : '—'}）</span></div>
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
                  <form method="POST" action="?/refund" use:enhance style="display:flex;gap:var(--space-2);align-items:center;flex-wrap:wrap;">
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
