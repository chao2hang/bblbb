<!-- M12-UI-01/02/03：托管 Checkout 确认页。
  - 展示市场身份、物品名称、数量、准确金额、当前余额、扣款后余额、Scope、
    授权有效期；金额/用户/余额不作为隐藏字段提交。
  - 表单为原生 form[method=POST]（无 JS 可用），携带稳定的幂等键
    （重试不重复扣款）。
  - 状态：成功 / 失败 / 处理中 / 重复请求 / 过期 Intent / request ID。
-->
<script lang="ts">
  import { enhance } from '$app/forms';
  import { newClientRequestId } from '$lib/api/client';
  import Icon from '$lib/components/ui/Icon.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import type { CheckoutActionData, CheckoutPageData } from './+page.server';

  let { data, form }: { data: CheckoutPageData; form?: CheckoutActionData | null } = $props();

  const checkout = $derived(data.checkout);
  const loadError = $derived(data.error);
  const idempotencyKey = $state(newClientRequestId());

  const success = $derived(Boolean(form?.ok && form.purchase));
  const denied = $derived(Boolean(form?.ok && !form.purchase));
  const expired = $derived(form?.code === 'checkout_intent_expired');
  const consumed = $derived(form?.code === 'checkout_intent_consumed');
  const insufficient = $derived(form?.code === 'insufficient_funds');
  const processing = $derived(form?.code === 'invalid_request' || (form?.message ?? '').includes('in progress'));
  const failed = $derived(Boolean(form && !form.ok && !expired && !consumed && !insufficient && !processing));

  function authWindow(expiresAt: number): string {
    const left = expiresAt - Date.now();
    if (left <= 0) return '已过期';
    return `剩余约 ${Math.max(1, Math.round(left / 60000))} 分钟`;
  }

  function formatTs(ms: number): string {
    return new Date(ms).toLocaleString('zh-CN', { hour12: false });
  }
</script>

<svelte:head>
  <title>{checkout ? `确认购买 · ${checkout.merchant_name}` : '确认购买'} · BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">Marketplace 结账</span>
  </nav>

  {#if loadError}
    <div class="card">
      <div class="card-body">
        <p class="input-hint is-error" role="alert">{loadError.message}</p>
        {#if loadError.code === 'checkout_intent_expired'}
          <p class="input-hint">授权已过期，请回到商户页面重新发起购买。</p>
        {:else if loadError.code === 'checkout_intent_consumed'}
          <p class="input-hint">该请求已完成，请查看“我的购买”确认结果。</p>
        {:else if loadError.code === 'checkout_user_mismatch'}
          <p class="input-hint">登录账号与发起购买时不一致，请使用原账号或重新发起。</p>
        {/if}
      </div>
    </div>
  {:else if !checkout}
    <p class="input-hint is-error" role="alert">结账信息不存在或不可用</p>
  {:else if success}
    <div class="card">
      <div class="card-body" role="status">
        <h1 style="margin:0 0 var(--space-3);">交易成功</h1>
        <dl class="checkout-meta" style="display:grid;grid-template-columns:auto 1fr;gap:var(--space-2) var(--space-4);margin:0;">
          <dt>商户</dt><dd>{checkout.merchant_name}</dd>
          <dt>商品</dt><dd>{checkout.offer_title} × {checkout.quantity}</dd>
          <dt>金额</dt><dd>{form?.purchase?.amount ?? checkout.amount} {checkout.currency_id.toUpperCase()}</dd>
          <dt>状态</dt><dd>succeeded</dd>
          <dt>请求 ID</dt><dd class="mono">{form?.requestId ?? '—'}</dd>
          <dt>购买 ID</dt><dd class="mono">{form?.purchase?.id ?? '—'}</dd>
        </dl>
        <p class="input-hint">商户将通过其结算回调获得本次交易；也可在“我的购买”中随时查询。</p>
        <a href="/marketplace/purchases" class="btn btn-primary">查看我的购买</a>
      </div>
    </div>
  {:else if denied}
    <div class="card">
      <div class="card-body" role="status">
        <h1 style="margin:0;">已取消授权</h1>
        <p class="input-hint">本次购买未发起，余额未发生变动。</p>
      </div>
    </div>
  {:else if expired}
    <div class="card">
      <div class="card-body" role="status">
        <h1 style="margin:0;">授权已过期</h1>
        <p class="input-hint">请在 5 分钟内完成确认；请回到商户页面重新发起购买。</p>
      </div>
    </div>
  {:else if consumed}
    <div class="card">
      <div class="card-body" role="status">
        <h1 style="margin:0;">该请求已完成</h1>
        <p class="input-hint">重复提交不会重复扣款；请前往“我的购买”查看原结果。</p>
        <a href="/marketplace/purchases" class="btn btn-primary">查看我的购买</a>
      </div>
    </div>
  {:else}
    <div class="card">
      <div class="card-body">
        <div style="display:flex;align-items:center;gap:var(--space-2);flex-wrap:wrap;">
          <Icon name="shield-check" size={28} />
          <h1 style="margin:0;font-size:var(--text-xl);">确认购买</h1>
          <span class="badge badge-neutral">商户：{checkout.merchant_name}</span>
        </div>
        {#if checkout.terms_url || checkout.privacy_url}
          <p class="input-hint">
            通过继续，你同意
            {#if checkout.terms_url}<a href={checkout.terms_url} target="_blank" rel="noopener noreferrer">服务条款</a>{/if}
            {#if checkout.terms_url && checkout.privacy_url}与{/if}
            {#if checkout.privacy_url}<a href={checkout.privacy_url} target="_blank" rel="noopener noreferrer">隐私政策</a>{/if}。
          </p>
        {/if}

        {#if form?.message && !processing}
          <p class="input-hint is-error" role="alert">
            {form.message}
            {#if form.requestId}<span class="mono">（request {form.requestId}）</span>{/if}
            ——本次未扣款；{insufficient ? '余额不足，请充值后重新发起购买。' : '可重新发起或联系商户。'}
          </p>
        {:else if processing}
          <p class="input-hint" role="status">请求处理中，请勿重复提交——同一请求重复提交不会重复扣款。</p>
        {/if}

        <dl class="checkout-meta" style="display:grid;grid-template-columns:auto 1fr;gap:var(--space-2) var(--space-4);margin:0;">
          <dt>商户</dt><dd>{checkout.merchant_name}</dd>
          <dt>商品</dt><dd>{checkout.offer_title}{checkout.offer_description ? ` — ${checkout.offer_description}` : ''}</dd>
          <dt>商品版本</dt><dd>v{checkout.offer_version}</dd>
          <dt>数量</dt><dd>{checkout.quantity}</dd>
          <dt>金额（准确）</dt><dd><strong>{checkout.amount} {checkout.currency_id.toUpperCase()}</strong></dd>
          <dt>当前余额</dt><dd>{checkout.balance} {checkout.currency_id.toUpperCase()}</dd>
          <dt>扣款后余额</dt>
          <dd>
            <strong class={checkout.balance_after < 0 ? 'is-danger' : ''}>
              {checkout.balance_after} {checkout.currency_id.toUpperCase()}
            </strong>
          </dd>
          <dt>请求的权限</dt>
          <dd>{checkout.scopes.map((s) => `<code>${s}</code>`).join('、')}</dd>
          <dt>授权期限</dt><dd>{formatTs(checkout.expires_at)}（{authWindow(checkout.expires_at)}）</dd>
        </dl>

        <div style="display:flex;gap:var(--space-3);margin-top:var(--space-4);flex-wrap:wrap;">
          <form
            method="POST"
            action="?/confirm"
            use:enhance={() => {
              return async ({ update }) => update();
            }}
          >
            <input type="hidden" name="client_request_id" value={idempotencyKey} />
            <input type="hidden" name="expected_intent_version" value={checkout.version} />
            <Button
              text={checkout.balance_after < 0 ? '余额不足' : '确认购买'}
              variant="primary"
              size="md"
              type="submit"
              disabled={checkout.balance_after < 0 || checkout.status !== 'pending' || checkout.expires_at <= Date.now()}
            />
          </form>
          <form
            method="POST"
            action="?/deny"
            use:enhance={() => {
              return async ({ update }) => update();
            }}
          >
            <input type="hidden" name="client_request_id" value={idempotencyKey} />
            <input type="hidden" name="expected_intent_version" value={checkout.version} />
            <Button text="取消" variant="secondary" size="md" type="submit" />
          </form>
        </div>
        <p class="input-hint">金额与余额均由 BBLBB 服务端计算，页面不提交可篡改的价格、用户或余额字段。确认后结果以“我的购买”为准。</p>
      </div>
    </div>
  {/if}
</div>
