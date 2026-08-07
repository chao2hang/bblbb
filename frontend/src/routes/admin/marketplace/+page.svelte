<!-- M12-UI-05/06：管理员 Marketplace 控制台。
  - Client 注册/更新、逐 scope 审批与限额、紧急停用、Webhook 轮换、
    对账运行、requested 退款重试；
  - 高风险操作全部强制 reason（前端必填 + 后端 reason/recent-auth/审计）；
  - 展示商户余额、Webhook 投递记录与审计依据。
-->
<script lang="ts">
  import { enhance } from '$app/forms';
  import { adminStateLabel } from '$lib/admin';
  import Icon from '$lib/components/ui/Icon.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import type { AdminMarketplaceActionData, AdminMarketplacePageData } from './+page.server';
  import type { MarketplaceClientView } from '$lib/api/types';

  let { data, form }: { data: AdminMarketplacePageData; form?: AdminMarketplaceActionData | null } = $props();

  const clients = $derived(data.clients);
  const offers = $derived(data.offers);
  const deliveries = $derived(data.deliveries);
  const balances = $derived(data.balances);

  const scopes = [
    'marketplace.checkout.create',
    'marketplace.purchase',
    'marketplace.offer.write',
    'marketplace.purchases.read',
    'marketplace.refund',
    'marketplace.webhook.manage'
  ] as const;

  function clientVersion(c: MarketplaceClientView): number {
    return typeof c.version === 'number' ? c.version : 1;
  }

  function scopeStatus(c: MarketplaceClientView, scope: string): string {
    const entry = (c.scopes ?? []).find((s) => s.scope === scope);
    return entry?.status ?? 'pending';
  }

  function limitValue(c: MarketplaceClientView, scope: string, key: string): string {
    const entry = (c.scopes ?? []).find((s) => s.scope === scope);
    const v = entry?.limits?.[key];
    return typeof v === 'number' && v > 0 ? String(v) : '';
  }

  function statusBadge(status: string): string {
    switch (status) {
      case 'active':
        return 'badge-neutral';
      case 'pending':
        return 'badge-warning';
      default:
        return 'badge-danger';
    }
  }
</script>

<svelte:head>
  <title>Marketplace 管理 · BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <a href="/admin" class="breadcrumb-link">管理后台</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">Marketplace</span>
  </nav>

  <h1 style="margin:0 0 var(--space-3);">Marketplace 管理</h1>
  {#if form?.message}
    <p class="input-hint {form.code === 'version_conflict' ? 'is-error' : ''}" role="status">
      {form.message}
      {#if form.secret}
        <span class="mono">新 Webhook Secret（仅显示一次）：{form.secret}</span>
      {/if}
    </p>
  {/if}

  {#if clients.state !== 'ok'}
    <p class="input-hint is-error">{adminStateLabel(clients.state)}</p>
  {/if}

  <section class="card" aria-label="Marketplace Client 管理">
    <div class="card-body">
      <h2 style="margin:0 0 var(--space-2);">Client / Scope</h2>
      <p class="input-hint">新 Client：先在「OAuth Clients」创建 Confidential Client，然后在此用其 client_id 注册。所有变更强制 reason + 近期重认证 + If-Match。</p>
      <div style="display:grid;gap:var(--space-3);">
        {#each clients.state === 'ok' ? clients.items : [] as client (client.id)}
          <div class="marketplace-client-form" style="border:1px solid var(--color-border,#d0d7de);border-radius:var(--radius-md,8px);padding:var(--space-3);display:grid;gap:var(--space-2);">
          <form
            method="POST"
            action="?/upsertClient"
            use:enhance={() => ({ update }) => update()}
            style="display:grid;gap:var(--space-2);"
          >
            <div style="display:flex;justify-content:space-between;flex-wrap:wrap;gap:var(--space-2);">
              <strong>{client.name}</strong>
              <span class="badge {statusBadge(client.status)}">{client.status}</span>
            </div>
            <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:var(--space-2);">
              <input type="hidden" name="client_id" value={client.client_id} />
              <input type="hidden" name="version" value={clientVersion(client)} />
              <label class="input-label">名称<input class="input-field" name="name" value={client.name} /></label>
              <label class="input-label">Owner User ID<input class="input-field" name="owner_user_id" value={client.owner_user_id} /></label>
              <label class="input-label">Terms URL<input class="input-field" name="terms_url" value={client.terms_url} /></label>
              <label class="input-label">Privacy URL<input class="input-field" name="privacy_url" value={client.privacy_url} /></label>
              <label class="input-label">Webhook URL<input class="input-field" name="webhook_url" value={client.webhook_url ?? ''} placeholder="https://" /></label>
              <label class="input-label">平台费 bps<input class="input-field" type="number" name="fee_bps" value={client.fee_bps} min="0" max="10000" /></label>
              <label class="input-label">Redirect URIs（逗号/换行分隔）<textarea class="input-field" name="redirect_uris" rows="2">{client.redirect_uris.join('\n')}</textarea></label>
            </div>
            <fieldset style="border:1px solid var(--color-border,#d0d7de);border-radius:var(--radius-sm,4px);padding:var(--space-2);">
              <legend class="input-label">Scope 审批与限额</legend>
              <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:var(--space-2);">
                {#each scopes as scope}
                  <div>
                    <label class="input-label" for={`scope-${client.id}-${scope}`}>
                      {scope}
                      <select id={`scope-${client.id}-${scope}`} class="input-field" name={`scope_${scope}`}>
                        <option value="" selected={scopeStatus(client, scope) === 'pending'}>未设置</option>
                        <option value="approved" selected={scopeStatus(client, scope) === 'approved'}>已批准</option>
                        <option value="disabled" selected={scopeStatus(client, scope) === 'disabled'}>禁用</option>
                      </select>
                    </label>
                    <label class="input-label">单笔限额<input class="input-field" type="number" name={`limit_${scope}_per_tx`} value={limitValue(client, scope, 'max_amount_per_transaction')} min="0" placeholder="不限" /></label>
                    <label class="input-label">日累计限额<input class="input-field" type="number" name={`limit_${scope}_daily`} value={limitValue(client, scope, 'max_amount_daily')} min="0" placeholder="不限" /></label>
                  </div>
                {/each}
              </div>
            </fieldset>
            <div style="display:flex;gap:var(--space-2);flex-wrap:wrap;align-items:flex-end;">
              <label class="input-label" style="flex:1;min-width:220px;">操作原因（必填）
                <input class="input-field" name="reason" placeholder="审批/变更原因" />
              </label>
              <Button text="保存 Client/Scope" variant="primary" size="sm" type="submit" />
            </div>
          </form>
            <div style="display:flex;gap:var(--space-2);flex-wrap:wrap;">
              <form method="POST" action="?/rotateWebhook" use:enhance={() => ({ update }) => update()}>
                <input type="hidden" name="client_id" value={client.client_id} />
                <label class="input-label">原因<input class="input-field" name="reason" placeholder="轮换原因" /></label>
                <Button text="轮换 Webhook Secret" variant="secondary" size="sm" type="submit" />
              </form>
              <form method="POST" action="?/emergencyDisable" use:enhance={() => ({ update }) => update()}>
                <input type="hidden" name="client_id" value={client.client_id} />
                <input type="hidden" name="version" value={clientVersion(client)} />
                <label class="input-label">原因<input class="input-field" name="reason" placeholder="紧急停用原因" /></label>
                <Button text="紧急停用" variant="danger" size="sm" type="submit" />
              </form>
            </div>
            <details>
              <summary class="input-hint" style="cursor:pointer;">余额 / 审计</summary>
              <div style="display:grid;grid-template-columns:auto 1fr;gap:var(--space-1) var(--space-3);margin-top:var(--space-2);">
                <span class="input-hint">可用</span><span class="input-hint">{client.balance?.available_balance ?? 0}</span>
                <span class="input-hint">待结算</span><span class="input-hint">{client.balance?.pending_balance ?? 0}</span>
                <span class="input-hint">冻结</span><span class="input-hint">{client.balance?.frozen_balance ?? 0}</span>
                <span class="input-hint">Webhook Secret 版本</span><span class="input-hint">{client.webhook_secret_version}</span>
                <span class="input-hint">批准历史</span>
                <span class="input-hint">{JSON.stringify(client.approval_history ?? [])}</span>
              </div>
            </details>
          </div>
        {/each}
        <p class="input-hint">
          商户余额只站内可追踪（不提现/不兑换）；结算等待期 7 天后由定时任务将 pending 转入 available。
        </p>
      </div>
    </div>
  </section>

  <section class="card" aria-label="商户余额">
    <div class="card-body">
      <h2 style="margin:0 0 var(--space-2);">商户余额</h2>
      <table class="table" style="width:100%;border-collapse:collapse;">
        <thead><tr><th style="text-align:left;">Client</th><th style="text-align:left;">可用</th><th style="text-align:left;">待结算</th><th style="text-align:left;">冻结</th></tr></thead>
        <tbody>
          {#each balances as b (b.client_id)}
            <tr>
              <td class="mono">{b.client_id}</td>
              <td>{b.available_balance}</td>
              <td>{b.pending_balance}</td>
              <td>{b.frozen_balance}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </section>

  <section class="card" aria-label="Offer 列表">
    <div class="card-body">
      <h2 style="margin:0 0 var(--space-2);">Offer（服务端登记）</h2>
      {#if offers.state !== 'ok'}
        <p class="input-hint is-error">{adminStateLabel(offers.state)}</p>
      {:else if offers.items.length === 0}
        <p class="input-hint">暂无报价。报价由已批准 Client 通过服务端接口登记（金额/货币/库存/平台费全部由 BBLBB 保存）。</p>
      {:else}
        <table class="table" style="width:100%;border-collapse:collapse;">
          <thead><tr><th style="text-align:left;">名称</th><th style="text-align:left;">Client</th><th style="text-align:left;">金额</th><th style="text-align:left;">库存</th><th style="text-align:left;">版本</th><th style="text-align:left;">状态</th></tr></thead>
          <tbody>
            {#each offers.items as o (o.id)}
              <tr>
                <td>{o.title}</td>
                <td class="mono">{o.client_id}</td>
                <td>{o.unit_amount} {o.currency_id.toUpperCase()}</td>
                <td>{o.stock_remaining ?? '不限量'}</td>
                <td>v{o.version}</td>
                <td>{o.status}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>
  </section>

  <section class="card" aria-label="对账">
    <div class="card-body">
      <h2 style="margin:0 0 var(--space-2);">对账</h2>
      <div style="display:grid;gap:var(--space-2);">
        {#each clients.state === 'ok' ? clients.items : [] as client (client.id)}
          <form method="POST" action="?/runReconciliation" use:enhance={() => ({ update }) => update()} style="display:flex;gap:var(--space-2);align-items:flex-end;flex-wrap:wrap;">
            <input type="hidden" name="client_id" value={client.client_id} />
            <label class="input-label">起始游标<input class="input-field" name="after_cursor" type="number" value="0" /></label>
            <label class="input-label">原因<input class="input-field" name="reason" placeholder="对账原因" /></label>
            <Button text={`对账 ${client.name}`} variant="secondary" size="sm" type="submit" />
          </form>
        {/each}
      </div>
      <p class="input-hint">对账校验：Purchase 金额 == 账本 operation；恒等式 Σ(delta_balance + delta_pending + delta_frozen) = 0；商户运营余额 == 账本余额。</p>
    </div>
  </section>

  <section class="card" aria-label="Webhook 投递">
    <div class="card-body">
      <h2 style="margin:0 0 var(--space-2);">Webhook 投递记录</h2>
      {#if deliveries.state !== 'ok'}
        <p class="input-hint is-error">{adminStateLabel(deliveries.state)}</p>
      {:else if deliveries.items.length === 0}
        <p class="input-hint">暂无投递记录。</p>
      {:else}
        <table class="table" style="width:100%;border-collapse:collapse;">
          <thead><tr><th style="text-align:left;">事件</th><th style="text-align:left;">类型</th><th style="text-align:left;">状态</th><th style="text-align:left;">尝试</th><th style="text-align:left;">状态码</th><th style="text-align:left;">下次重试</th></tr></thead>
          <tbody>
            {#each deliveries.items as d (d.id)}
              <tr>
                <td class="mono">{d.event_id.slice(0, 8)}</td>
                <td>{d.event_type}</td>
                <td>{d.status}</td>
                <td>{d.attempts}</td>
                <td>{d.last_status_code ?? '—'}</td>
                <td>{d.status === 'pending' ? new Date(d.next_retry_at).toLocaleString('zh-CN', { hour12: false }) : '—'}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
      <p class="input-hint"><Icon name="info" size={14} /> Webhook 由提交后 Outbox 投递；HMAC-SHA-256 签名、5 分钟时间窗、event_id 去重，非 2xx 指数退避，超限进入 dead-letter 并可手动重放。</p>
    </div>
  </section>
</div>
