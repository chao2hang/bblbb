<script lang="ts">
  // M18-MFA-01：独立两步验证页（对齐原型 #mfa 页面布局与流程）。
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import type { MfaActionData, MfaPageData } from './+page.server';
  import PageTitle from '$lib/components/PageTitle.svelte';

  let { data, form }: { data: MfaPageData; form?: MfaActionData | null } = $props();

  const user = $derived(data.user);
  const mfaStep = $derived(form?.mfa);
  let isEnabled = $derived(user?.mfa_enabled === true && mfaStep?.kind !== 'disabled');
</script>

  <PageTitle title="两步验证" />

<div class="container page-content">
  <div class="app-route-head">
    <div class="app-route-head__copy">
      <span class="app-kicker">SECURITY / MFA</span>
      <h1 tabindex="-1">两步验证</h1>
      <p>使用 TOTP 身份验证器（如 Google Authenticator、1Password）保护账号安全</p>
    </div>
  </div>

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
</style>
