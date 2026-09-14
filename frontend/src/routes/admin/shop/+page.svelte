<!-- M07-UI-08 & M18-ADMIN-BATCH：管理端商城——商品列表/新建/编辑/发布/停售 + 订单/退款。
  约定 A：所有写操作 = 按钮 → Dialog（表单在弹层内，reason/version 按 server 契约携带，
  成功后 toastActionResult → update → 关闭弹层并清理 target）。
  约定 B：商品列表选择列 + BatchBar →「批量上架」「批量下架」Dialog（循环单条端点）。
  约定 D：商品行「操作」按钮 → 「⋯」三点菜单（编辑/上架|下架，菜单项决定动作，
  Dialog 按 tab 渲染对应表单节）；订单行「退款」同样收进「⋯」菜单，行内操作有且只有「⋯」触发按钮。
  版本冲突（409）提示刷新；退款要求 reason 必填；后端裁决 403/501/5xx 状态。
-->
<script lang="ts">
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import type { SubmitFunction } from '@sveltejs/kit';
  import { adminStateLabel } from '$lib/admin';
  import { currencyLabel, formatMoney, productKindLabel, productStatusLabel } from '$lib/api/client';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import BatchBar from '$lib/components/admin/BatchBar.svelte';
  import RowActionsMenu from '$lib/components/admin/RowActionsMenu.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import AttachmentUploader from '$lib/components/upload/AttachmentUploader.svelte';
  import { toastActionResult } from '$lib/ui/action-toast';
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

  const PRODUCT_KINDS = [
    'cosmetic_nickname',
    'cosmetic_avatar',
    'cosmetic_badge',
    'profile_effect',
    'post_effect',
    'reaction_pack',
    'utility'
  ] as const;

  /** 弹层表单共用结果处理：toast → update → 成功才关弹层（失败留在弹层改）。 */
  const dialogEnhance = (onSuccess: () => void): SubmitFunction =>
    () => async ({ result, update }) => {
      toastActionResult(result);
      await update();
      if (result.type === 'success') onSuccess();
    };

  // ── 批量选择（约定 B：商品行复选框 + BatchBar）──
  let selectedProductIds = $state<string[]>([]);
  const selectableProducts: ShopProduct[] = $derived(products.state === 'ok' ? products.items : []);
  let allProductsSelected = $derived(
    selectableProducts.length > 0 && selectedProductIds.length === selectableProducts.length
  );
  function toggleAllProducts(): void {
    if (allProductsSelected) selectedProductIds = [];
    else selectedProductIds = selectableProducts.map((p) => p.id);
  }
  function toggleProduct(id: string): void {
    if (selectedProductIds.includes(id)) selectedProductIds = selectedProductIds.filter((x) => x !== id);
    else selectedProductIds = [...selectedProductIds, id];
  }
  function productById(id: string): ShopProduct | undefined {
    return selectableProducts.find((p) => p.id === id);
  }

  // ── 弹层 target 状态（一个 Dialog 服务一类操作，target 区分行）──
  /** 新建商品（?/create）。 */
  let createOpen = $state(false);
  function openCreate(): void {
    createOpen = true;
  }
  function closeCreate(): void {
    createOpen = false;
  }

  /**
   * 行操作弹层（M18-ADMIN-OPS 约定 D：每行一个「⋮」三点菜单，菜单项决定动作）。
   * 商品行动作 = 编辑（?/update，If-Match）/ 上架（?/publish）/ 下架（?/disable，
   * reason 必填）——不同端点 → Dialog 按 tab 渲染对应表单节。
   */
  let opsTarget = $state<ShopProduct | null>(null);
  let opsTab = $state<'edit' | 'publish' | 'disable'>('edit');

  /** 编辑分节表单草稿（?/update：If-Match version + reason）。 */
  let editTarget = $state<ShopProduct | null>(null);
  let editTitle = $state('');
  let editUnitPrice = $state('');
  let editStock = $state('');
  let editLevel = $state('');
  let editLimit = $state('');
  let editSlot = $state('');
  let editValidity = $state('');
  let editAssetAttachmentId = $state('');
  let editPresentationTokens = $state('');
  let editReason = $state('');
  let createAssetAttachmentId = $state('');
  let createPresentationTokens = $state('');

  /** 下架分节表单（?/disable：reason 必填写审计）。 */
  let disableReason = $state('');

  function openOps(p: ShopProduct, tab: 'edit' | 'publish' | 'disable'): void {
    opsTarget = p;
    opsTab = tab;
    // 预填编辑草稿（沿用既有 openEdit 预填逻辑）
    editTarget = p;
    editTitle = p.title;
    editUnitPrice = String(p.unit_price);
    editStock = p.stock_remaining == null ? '' : String(p.stock_remaining);
    editLevel = String(p.required_level ?? 1);
    editLimit = String(p.quantity_limit ?? 1);
    editSlot = p.slot ?? '';
    editValidity = p.validity_seconds == null ? '' : String(p.validity_seconds);
    editAssetAttachmentId = p.asset_attachment_id ?? '';
    editPresentationTokens = p.presentation_tokens?.join(', ') ?? '';
    editReason = '';
  }

  function closeOps(): void {
    opsTarget = null;
  }

  /** 行「⋮」菜单项（约定 D）：编辑 + 上架|下架（按商品状态二选一）。 */
  function rowActions(p: ShopProduct) {
    const actions: { label: string; danger?: boolean; run: () => void }[] = [
      { label: '编辑', run: () => openOps(p, 'edit') }
    ];
    if (p.status !== 'published') actions.push({ label: '上架', run: () => openOps(p, 'publish') });
    else actions.push({ label: '下架', danger: true, run: () => openOps(p, 'disable') });
    return actions;
  }

  /** Dialog 描述随菜单选定的动作变化（单动作确认，约定 D）。 */
  const opsDescription = $derived.by(() => {
    if (!opsTarget) return '';
    if (opsTab === 'publish') return `将「${opsTarget.title}」上架发布；发布后用户可见可购买。`;
    if (opsTab === 'disable') return `将「${opsTarget.title}」下架停售；停售原因写入审计日志。`;
    return `编辑「${opsTarget.title}」（${opsTarget.slug}，v${opsTarget.version}）；保存走 If-Match 乐观锁，操作原因写审计。`;
  });

  /** 订单退款（?/refund：amount 可空=全额，reason 必填）。 */
  let refundTarget = $state<ShopOrder | null>(null);
  let refundAmount = $state('');
  let refundReason = $state('');
  function openRefund(o: ShopOrder): void {
    refundTarget = o;
    refundAmount = '';
    refundReason = '';
  }
  function closeRefund(): void {
    refundTarget = null;
  }

  /** 批量上架（?/batchPublish）/ 批量下架（?/batchDisable）。 */
  let batchPublishOpen = $state(false);
  let batchDisableOpen = $state(false);
  let batchDisableReason = $state('');
  function openBatchPublish(): void {
    batchPublishOpen = true;
  }
  function closeBatchPublish(): void {
    batchPublishOpen = false;
  }
  function openBatchDisable(): void {
    batchDisableReason = '';
    batchDisableOpen = true;
  }
  function closeBatchDisable(): void {
    batchDisableOpen = false;
  }

  function formatTs(ts: number | undefined): string {
    if (!ts) return '—';
    return new Date(ts).toLocaleString('zh-CN', { hour12: false });
  }
</script>

<svelte:head>
  <title>商城管理 — BBLBB</title>
</svelte:head>

<PageHeader title="商城管理" />

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
          结算货币 {currencyLabel(config.data.currency_id ?? 'coin')} · 默认退款策略 {refundPolicyLabel(config.data.default_refund_policy ?? 'non_refundable')}
        </span>
      </div>
    </div>
  {/if}

  <div class="app-card" style="margin-bottom:var(--space-4);">
    <div class="app-card__head" style="display:flex;align-items:center;justify-content:space-between;gap:8px;flex-wrap:wrap;">
      <h2>商品（{products.state === 'ok' ? products.items.length : '—'}）</h2>
      <Button text="新建商品" variant="primary" size="sm" onclick={openCreate} />
    </div>
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
        <!-- 批量工具条（约定 B：选中后渲染） -->
        <div style="padding:10px 14px 0;">
          <BatchBar count={selectedProductIds.length} noun="件商品" onclear={() => (selectedProductIds = [])}>
            <Button text="批量上架" variant="secondary" size="sm" onclick={openBatchPublish} />
            <Button text="批量下架" variant="danger" size="sm" onclick={openBatchDisable} />
          </BatchBar>
        </div>
        <div style="display:flex;flex-direction:column;">
          {#each products.items as p (p.id)}
            <div class="post-row" style="padding:var(--space-3);border-bottom:var(--border-default);">
              <div style="display:flex;gap:var(--space-3);align-items:center;flex-wrap:wrap;">
                <div style="flex:0 0 auto;">
                  <input
                    type="checkbox"
                    checked={selectedProductIds.includes(p.id)}
                    onchange={() => toggleProduct(p.id)}
                    aria-label="选择商品 {p.title}"
                  />
                </div>
                <div style="min-width:0;flex:1;">
                  <strong>{p.title}</strong>
                  <span class="badge badge-neutral" style="margin-left:var(--space-2);">{productKindLabel(p.kind)}</span>
                  <span class="badge {p.status === 'published' ? 'badge-success' : p.status === 'disabled' ? 'badge-warning' : 'badge-neutral'}">
                    {productStatusLabel(p.status)}
                  </span>
                  <p class="text-secondary" style="font-size:var(--text-xs);margin:2px 0 0;">
                    {p.slug} · {formatMoney(p.unit_price, { id: p.currency_id, code: p.currency_code, name: p.currency_name }, { free: true })} · v{p.version} · 更新于 {formatTs(p.updated_at)}
                  </p>
                </div>
                <div style="display:flex;gap:var(--space-2);flex-wrap:wrap;">
                  <!-- 每行一个「⋮」三点菜单：编辑/上架|下架由菜单项决定动作（约定 D） -->
                  <RowActionsMenu label="更多操作：商品 {p.title}" actions={rowActions(p)} />
                </div>
              </div>
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
                    {o.id} · ×{o.quantity} · {formatMoney(o.total_amount, { id: o.currency_id, code: o.currency_code, name: o.currency_name }, { free: true })} · v{o.product_version} · {formatTs(o.created_at)}
                  </p>
                </div>
                {#if o.status === 'succeeded'}
                  <!-- 行内操作有且只有「⋯」菜单（约定 D）：退款收进菜单 -->
                  <RowActionsMenu
                    label="更多操作：订单 {o.id}"
                    actions={[{ label: '退款', run: () => openRefund(o) }]}
                  />
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  <!-- 新建商品 Dialog（?/create；字段错误保留在弹层内提示） -->
  <Dialog open={createOpen} title="新建商品" description="创建后商品为待上架状态；操作原因写审计。" onclose={closeCreate}>
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
          // 动作结果 → 全局 Toast（成功"商品「x」已创建"/ 失败服务端文案）；
          // 字段级错误仍以弹层内 fieldErrors 提示为主。顶部横幅为无 JS 回退。
          toastActionResult(result);
          if (result.type === 'success') closeCreate();
        };
      }}
    >
      <input type="hidden" name="currency_id" value="coin" />
      <input type="hidden" name="asset_attachment_id" value={createAssetAttachmentId} />
      <input type="hidden" name="presentation_tokens" value={createPresentationTokens} />
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
      <div class="admin-asset-field">
        <label class="input-label" for="np-tokens">展示 Token（逗号分隔）</label>
        <input id="np-tokens" class="input-field" bind:value={createPresentationTokens} placeholder="nickname.color.rainbow" />
        <AttachmentUploader accept="image/png" maxBytes={2 * 1024 * 1024} waitReady={true} showQuota={false} label="上传 PNG 头像框/挂件" onReady={(a) => (createAssetAttachmentId = a.id)} />
        {#if createAssetAttachmentId}<small class="input-hint">已绑定 PNG：{createAssetAttachmentId}</small>{/if}
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
  </Dialog>

  <!-- 行操作 Dialog（约定 D）：动作由「⋮」菜单项决定，弹层内单动作确认——
       编辑（?/update）/ 上架（?/publish）/ 下架（?/disable）按 opsTab 渲染对应表单节 -->
  <Dialog
    open={opsTarget !== null}
    title={opsTarget ? `商品操作：${opsTarget.title}` : '商品操作'}
    description={opsDescription}
    onclose={closeOps}
  >
    {#if opsTarget}
      {#if opsTab === 'edit'}
        <!-- 编辑商品（?/update：If-Match version + reason 审计） -->
        <form method="POST" action="?/update" use:enhance={dialogEnhance(closeOps)}>
          <input type="hidden" name="id" value={opsTarget?.id ?? ''} />
          <input type="hidden" name="version" value={opsTarget?.version ?? ''} />
          <input type="hidden" name="asset_attachment_id" value={editAssetAttachmentId} />
          <input type="hidden" name="presentation_tokens" value={editPresentationTokens} />
          <div class="admin-form-grid" style="display:grid;grid-template-columns:repeat(auto-fit,minmax(160px,1fr));gap:var(--space-2);">
            <div class="input-wrapper">
              <label class="input-label" for="up-title">标题</label>
              <input id="up-title" name="title" class="input-field" bind:value={editTitle} />
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="up-price">价格</label>
              <input id="up-price" name="unit_price" type="number" min="0" class="input-field" bind:value={editUnitPrice} />
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="up-stock">库存（空=不限）</label>
              <input id="up-stock" name="stock_remaining" type="number" min="0" class="input-field" bind:value={editStock} />
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="up-level">等级门槛</label>
              <input id="up-level" name="required_level" type="number" min="1" class="input-field" bind:value={editLevel} />
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="up-limit">限购</label>
              <input id="up-limit" name="quantity_limit" type="number" min="1" class="input-field" bind:value={editLimit} />
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="up-slot">展示槽位</label>
              <input id="up-slot" name="slot" class="input-field" bind:value={editSlot} />
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="up-validity">有效期（秒）</label>
              <input id="up-validity" name="validity_seconds" type="number" min="0" class="input-field" bind:value={editValidity} />
            </div>
          </div>
           <div class="admin-asset-field">
             <label class="input-label" for="up-tokens">展示 Token（逗号分隔）</label>
             <input id="up-tokens" class="input-field" bind:value={editPresentationTokens} placeholder="nickname.color.gradient_aurora" />
             <AttachmentUploader accept="image/png" maxBytes={2 * 1024 * 1024} waitReady={true} showQuota={false} label="替换 PNG 头像框/挂件" onReady={(a) => (editAssetAttachmentId = a.id)} />
             {#if editAssetAttachmentId}<small class="input-hint">已绑定 PNG：{editAssetAttachmentId}</small>{/if}
           </div>
           <div class="input-wrapper" style="margin-top:var(--space-2);">
             <label class="input-label" for="up-reason">操作原因</label>
            <input id="up-reason" name="reason" class="input-field" required bind:value={editReason} placeholder="必填（写审计）" />
          </div>
          <Button text={opsTarget ? `保存（v${opsTarget.version + 1}）` : '保存'} variant="primary" size="sm" type="submit" />
        </form>
      {:else if opsTab === 'publish'}
        <!-- 上架（?/publish：server 契约无 reason/If-Match） -->
        <form method="POST" action="?/publish" use:enhance={dialogEnhance(closeOps)}>
          <input type="hidden" name="id" value={opsTarget?.id ?? ''} />
          <Button text="确认上架" variant="primary" size="sm" type="submit" />
        </form>
      {:else}
        <!-- 下架（?/disable：reason 必填写审计） -->
        <form method="POST" action="?/disable" use:enhance={dialogEnhance(closeOps)}>
          <input type="hidden" name="id" value={opsTarget?.id ?? ''} />
          <div class="input-wrapper" style="margin-bottom:var(--space-3);">
            <label class="input-label" for="sp-disable-reason">停售原因（写审计）</label>
            <input id="sp-disable-reason" name="reason" class="input-field" required bind:value={disableReason} placeholder="必填" />
          </div>
          <Button text="确认下架" variant="danger" size="sm" type="submit" />
        </form>
      {/if}
    {/if}
  </Dialog>

  <!-- 订单退款 Dialog（?/refund：amount 空=全额，reason 必填） -->
  <Dialog
    open={refundTarget !== null}
    title="订单退款补偿"
    description={refundTarget ? `对订单 ${refundTarget.id} 提交补偿退款；金额留空为全额。` : ''}
    onclose={closeRefund}
  >
    <form method="POST" action="?/refund" use:enhance={dialogEnhance(closeRefund)}>
      <input type="hidden" name="id" value={refundTarget?.id ?? ''} />
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="sp-refund-amount">退款金额（coin，空=全额）</label>
        <input id="sp-refund-amount" name="amount" type="number" min="0" class="input-field" bind:value={refundAmount} placeholder="全额" />
      </div>
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="sp-refund-reason">退款原因（写审计）</label>
        <input id="sp-refund-reason" name="reason" class="input-field" required bind:value={refundReason} placeholder="必填" />
      </div>
      <Button text="确认退款" variant="primary" size="sm" type="submit" />
    </form>
  </Dialog>

  <!-- 批量上架 Dialog（?/batchPublish：循环 publish 单条端点） -->
  <Dialog
    open={batchPublishOpen}
    title="批量上架"
    description={`将发布 ${selectedProductIds.length} 件商品；发布后用户可见可购买。`}
    onclose={closeBatchPublish}
  >
    <form
      method="POST"
      action="?/batchPublish"
      use:enhance={() => async ({ result, update }) => {
        toastActionResult(result);
        await update();
        if (result.type === 'success') {
          selectedProductIds = [];
          closeBatchPublish();
        }
      }}
    >
      <input type="hidden" name="ids" value={selectedProductIds.join(',')} />
      <Button text={`批量上架 ${selectedProductIds.length} 件`} variant="primary" size="sm" type="submit" />
    </form>
  </Dialog>

  <!-- 批量下架 Dialog（?/batchDisable：同一原因逐条写审计） -->
  <Dialog
    open={batchDisableOpen}
    title="批量下架"
    description={`将停售 ${selectedProductIds.length} 件商品；原因将逐条写入审计。`}
    onclose={closeBatchDisable}
  >
    <form
      method="POST"
      action="?/batchDisable"
      use:enhance={() => async ({ result, update }) => {
        toastActionResult(result);
        await update();
        if (result.type === 'success') {
          selectedProductIds = [];
          closeBatchDisable();
        }
      }}
    >
      <input type="hidden" name="ids" value={selectedProductIds.join(',')} />
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="sp-batch-disable-reason">停售原因（写审计）</label>
        <input id="sp-batch-disable-reason" name="reason" class="input-field" required bind:value={batchDisableReason} placeholder="必填" />
      </div>
      <Button text={`批量下架 ${selectedProductIds.length} 件`} variant="danger" size="sm" type="submit" />
    </form>
  </Dialog>
