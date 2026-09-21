<script lang="ts">
  // M18-MFA-01：独立两步验证页（对齐原型 #mfa 页面布局与流程）。
  // M02-MFA-PK：Passkey 与 TOTP 共存——列表/注册/撤销管理卡片。
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import { registerPasskey, passkeyErrorMessage, passkeySupported } from '$lib/mfa/passkey';
  import type { MfaActionData, MfaPageData } from './+page.server';
  import PageTitle from '$lib/components/PageTitle.svelte';

  let { data, form }: { data: MfaPageData; form?: MfaActionData | null } = $props();

  const user = $derived(data.user);
  const mfaStep = $derived(form?.mfa);
  let isEnabled = $derived(user?.mfa_enabled === true && mfaStep?.kind !== 'disabled');

  // Passkey 注册（客户端浏览器凭据生成 → /mfa/passkey/confirm 落库）
  let passkeyName = $state('');
  let passkeyBusy = $state(false);

  function formatMs(ms: number | null): string {
    if (!ms) return '—';
    return new Date(ms).toLocaleString();
  }

  async function addPasskey() {
    if (passkeyBusy) return;
    if (!passkeySupported()) {
      showToast('当前浏览器不支持 Passkey（需 HTTPS 与较新浏览器）', 'danger');
      return;
    }
    passkeyBusy = true;
    try {
      const beginRes = await fetch('/mfa/passkey/begin', {
        method: 'POST',
        headers: { Accept: 'application/json' }
      });
      if (!beginRes.ok) {
        const problem = (await beginRes.json().catch(() => null)) as { message?: string } | null;
        throw new Error(problem?.message || '开始注册失败，请重试');
      }
      const credential = await registerPasskey(
        (await beginRes.json()) as Parameters<typeof registerPasskey>[0]
      );
      const confirmRes = await fetch('/mfa/passkey/confirm', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
        body: JSON.stringify({ name: passkeyName.trim() || undefined, credential })
      });
      if (!confirmRes.ok) {
        const problem = (await confirmRes.json().catch(() => null)) as { message?: string } | null;
        throw new Error(problem?.message || '确认注册失败，请重试');
      }
      showToast('Passkey 已添加', 'success');
      passkeyName = '';
      await invalidateAll();
    } catch (e) {
      showToast(passkeyErrorMessage(e), 'danger');
    } finally {
      passkeyBusy = false;
    }
  }
</script>

  <PageTitle title="两步验证" />

<div class="container page-content">
  <h1 class="u-visually-hidden">两步验证</h1>

  {#if data.error}
    <p class="input-hint is-error" role="alert">{data.error}</p>
  {/if}

  <div class="card" style="margin-top:var(--space-4);">
    <div class="card-header" style="display:flex;align-items:center;justify-content:space-between;">
      <span class="card-title">当前状态</span>
      <span class="badge {isEnabled ? 'badge-success' : 'badge-neutral'}">
        {isEnabled ? '已启用' : '未启用'}
      </span>
    </div>
    <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-4);">
      {#if form?.message}
        <p class="input-hint is-error" role="alert" style="margin:0;">{form.message}</p>
      {/if}

      {#if !isEnabled && (!mfaStep || mfaStep.kind === 'disabled')}
        <p class="text-secondary" style="margin:0;line-height:1.6;">
          开启两步验证后，在输入密码后还需要输入手机认证器生成的 6 位动态验证码，极大增强账户安全。
        </p>
        <form method="POST" action="?/enroll" use:enhance>
          <Button text="立即开启两步验证" variant="primary" type="submit" />
        </form>
      {:else if mfaStep?.kind === 'enroll-challenge'}
        <div class="mfa-steps">
          <div class="mfa-steps__item">
            <p class="mfa-steps__title"><span class="mfa-steps__num">1</span>用手机认证器扫描二维码</p>
            <p class="text-secondary" style="margin:0 0 var(--space-3);font-size:var(--text-sm);">
              打开 Google Authenticator / 1Password / Microsoft Authenticator，选择「扫描二维码」添加账号。
            </p>
            <div class="otp-qr-wrap">
              {#if mfaStep.qr_data_url}
                <img
                  class="otp-qr"
                  src={mfaStep.qr_data_url}
                  alt="两步验证注册二维码（用认证器 App 扫描添加）"
                  width="220"
                  height="220"
                />
              {:else}
                <p class="auth-hint" role="alert">二维码生成失败，请使用下方密钥手工添加。</p>
              {/if}
            </div>
            <details class="mfa-manual">
              <summary>无法扫码？手工录入密钥</summary>
              <p class="text-secondary" style="margin:var(--space-2) 0 var(--space-2);font-size:var(--text-sm);">
                在认证器中手工录入以下密钥（Base32）：
              </p>
              <code class="mfa-manual__secret">{mfaStep.secret_base32}</code>
              <p class="text-tertiary mfa-manual__uri">otpauth 链接：{mfaStep.otpauth_uri}</p>
            </details>
          </div>

          <form method="POST" action="?/confirm" use:enhance class="mfa-steps__item">
            <p class="mfa-steps__title"><span class="mfa-steps__num">2</span>输入 6 位动态验证码确认</p>
            <p class="text-secondary" style="margin:0 0 var(--space-3);font-size:var(--text-sm);">
              扫码后，认证器会为该账号生成 6 位动态验证码（每 30 秒刷新），输入下方完成启用。
            </p>
            <div class="mfa-confirm-row">
              <input
                type="text"
                name="code"
                class="input-field"
                placeholder="000000"
                maxlength="6"
                inputmode="numeric"
                pattern="[0-9]{6}"
                required
                autocomplete="one-time-code"
                aria-label="6 位动态验证码"
              />
              <Button text="验证并启用" variant="primary" type="submit" />
            </div>
          </form>
        </div>

        <form method="POST" action="?/cancel" use:enhance style="margin-top:var(--space-4);">
          <Button text="取消设置" variant="ghost" size="sm" type="submit" />
        </form>
      {:else if mfaStep?.kind === 'enroll-confirmed'}
        <div class="alert alert-success" role="status" style="padding:var(--space-3);background:var(--color-bg-subtle);border-radius:var(--radius-md);border-left:3px solid var(--color-success);">
          <strong>两步验证已成功启用！</strong>
          <p style="margin:4px 0 0;font-size:var(--text-sm);color:var(--color-text-secondary);">
            强烈建议立即生成一次性恢复码，并在手机遗失时用恢复码登录。
          </p>
        </div>
        <form method="POST" action="?/recovery" use:enhance>
          <Button text="生成恢复码" variant="primary" type="submit" />
        </form>
      {:else if mfaStep?.kind === 'recovery-codes'}
        <div style="display:flex;flex-direction:column;gap:var(--space-3);">
          <strong>请保存好以下恢复码（每个只能使用一次）：</strong>
          <div style="display:grid;grid-template-columns:repeat(auto-fill,minmax(140px,1fr));gap:var(--space-2);">
            {#each mfaStep.codes as code}
              <code style="padding:var(--space-2);background:var(--color-bg-subtle);border-radius:var(--radius-sm);text-align:center;font-weight:600;">{code}</code>
            {/each}
          </div>
          <p class="text-secondary" style="margin:0;font-size:var(--text-xs);">
            恢复码遗失将无法自行找回，建议妥善记录在密码管理器中。
          </p>
        </div>
      {:else if mfaStep?.kind === 'step-up'}
        <!-- M02-UX-SEC：step-up 重认证（M02-MFA-07，自 /me 平移）——敏感操作
             前要求重新输入密码确认身份（近期已认证则后端直接放行）。 -->
        <p class="text-secondary" style="margin:0;line-height:1.6;">
          出于安全考虑，此操作需要重新输入密码确认身份（近期已认证则可直接执行）。
        </p>
        <form method="POST" action="?/re-auth" use:enhance novalidate>
          <input type="hidden" name="intent" value={mfaStep.intent} />
          <div class="input-wrapper">
            <label class="input-label" for="reauth-password">密码</label>
            <input
              type="password"
              class="input-field"
              id="reauth-password"
              name="password"
              placeholder="输入当前密码"
              autocomplete="current-password"
            />
          </div>
          <div style="margin-top:var(--space-3);">
            <Button text="验证身份" variant="primary" size="sm" type="submit" />
          </div>
        </form>
      {:else if mfaStep?.kind === 'reauth-done'}
        <p class="input-hint" role="status">身份已验证，请再次点击原操作完成。</p>
        {#if mfaStep.intent === 'disable'}
          <form method="POST" action="?/disable" use:enhance>
            <Button text="停用两步验证" variant="danger" size="sm" type="submit" />
          </form>
        {:else}
          <form method="POST" action="?/recovery" use:enhance>
            <Button text="生成恢复码" variant="primary" size="sm" type="submit" />
          </form>
        {/if}
      {:else}
        <!-- 已启用态 -->
        <p class="text-secondary" style="margin:0;line-height:1.6;">
          登录时需要输入身份验证器 6 位动态验证码。如果丢失设备，可使用恢复码登录。
        </p>
        <div style="display:flex;gap:var(--space-2);flex-wrap:wrap;">
          <form method="POST" action="?/recovery" use:enhance>
            <Button text="生成新恢复码" variant="secondary" type="submit" />
          </form>
          <form method="POST" action="?/disable" use:enhance>
            <Button text="停用两步验证" variant="danger" type="submit" />
          </form>
        </div>
      {/if}
    </div>
  </div>

  {#if data.passkeyEnabled}
    <div class="card" style="margin-top:var(--space-4);">
      <div class="card-header" style="display:flex;align-items:center;gap:var(--space-2);">
        <Icon name="fingerprint" size={16} />
        <span class="card-title">Passkey（指纹 / Face ID / 屏幕锁）</span>
      </div>
      <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-4);">
        <p class="text-secondary" style="margin:0;line-height:1.6;">
          注册 Passkey 后，登录第二步可直接用本设备的指纹 / Face ID / 屏幕锁通过验证，
          无需再输入 6 位动态验证码（两步验证的两种方式任一通过即可）。
        </p>

        {#if form?.message && !form?.mfa}
          <p class="input-hint is-error" role="alert" style="margin:0;">{form.message}</p>
        {/if}
        {#if data.passkeysError}
          <p class="input-hint is-error" role="alert" style="margin:0;">{data.passkeysError}</p>
        {/if}

        {#if data.passkeys.length > 0}
          <ul class="passkey-list" role="list">
            {#each data.passkeys as item (item.id)}
              <li class="passkey-item">
                <div class="passkey-item__meta">
                  <strong>{item.name}</strong>
                  <span class="text-secondary">注册于 {formatMs(item.created_at)}</span>
                  <span class="text-secondary">最近使用 {formatMs(item.last_used_at)}</span>
                  {#if item.backed_up}
                    <span class="badge badge-success">已云同步</span>
                  {/if}
                </div>
                <form method="POST" action="?/passkeyRevoke" use:enhance>
                  <input type="hidden" name="id" value={item.id} />
                  <Button text="撤销" variant="ghost" size="sm" type="submit" />
                </form>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="text-secondary" style="margin:0;">尚未注册任何 Passkey。</p>
        {/if}

        <div class="passkey-add-row">
          <input
            type="text"
            class="input-field passkey-name-input"
            placeholder="名称（可选，如「MacBook 指纹」）"
            maxlength="64"
            bind:value={passkeyName}
            aria-label="Passkey 名称"
          />
          <Button
            text={passkeyBusy ? '等待认证器…' : '添加 Passkey'}
            variant="secondary"
            onclick={addPasskey}
            disabled={passkeyBusy}
          />
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  /* M18-MFA-01：enroll 两步流程（扫码 + 确认码）布局 */
  .mfa-steps {
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
  }
  .mfa-steps__item {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    background: var(--color-bg-subtle);
    padding: var(--space-4);
    border-radius: var(--radius-md);
  }
  .mfa-steps__title {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    font-size: var(--text-base);
  }
  .mfa-steps__num {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--color-brand);
    color: var(--color-on-brand, #fff);
    font-size: var(--text-xs);
    font-weight: 700;
    flex-shrink: 0;
  }
  .otp-qr-wrap {
    display: flex;
    justify-content: center;
    padding: var(--space-3) 0 var(--space-2);
  }
  .otp-qr {
    width: 220px;
    height: 220px;
    padding: 10px;
    background: #fff;
    border: var(--border-default, 1px solid var(--color-border));
    border-radius: var(--radius-md);
  }
  .mfa-manual {
    width: 100%;
    margin-top: var(--space-2);
    font-size: var(--text-sm);
  }
  .mfa-manual summary {
    cursor: pointer;
    color: var(--color-brand);
    user-select: none;
  }
  .mfa-manual__secret {
    display: block;
    padding: var(--space-2) var(--space-3);
    background: var(--color-bg-card);
    border: var(--border-default, 1px solid var(--color-border));
    border-radius: var(--radius-sm);
    font-size: var(--text-base);
    letter-spacing: 1px;
    user-select: all;
    word-break: break-all;
  }
  .mfa-manual__uri {
    margin: var(--space-2) 0 0;
    font-size: var(--text-xs);
    word-break: break-all;
  }
  .mfa-confirm-row {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    flex-wrap: wrap;
    width: 100%;
  }
  .mfa-confirm-row .input-field {
    width: 150px;
    font-size: var(--text-lg);
    letter-spacing: 3px;
    text-align: center;
  }

  /* M02-MFA-PK：Passkey 管理卡片 */
  .passkey-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .passkey-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    background: var(--color-bg-subtle);
    border-radius: var(--radius-sm);
  }
  .passkey-item__meta {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-1) var(--space-3);
    font-size: var(--text-sm);
  }
  .passkey-add-row {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    flex-wrap: wrap;
  }
  .passkey-name-input {
    flex: 1;
    min-width: 200px;
  }
</style>
