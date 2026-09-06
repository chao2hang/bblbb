<script lang="ts">
  // M18-MFA-01：独立两步验证页（对齐原型 #mfa 页面布局与流程）。
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import type { MfaActionData, MfaPageData } from './+page.server';

  let { data, form }: { data: MfaPageData; form?: MfaActionData | null } = $props();

  const user = $derived(data.user);
  const mfaStep = $derived(form?.mfa);
  let isEnabled = $derived(user?.mfa_enabled === true && mfaStep?.kind !== 'disabled');
</script>

<svelte:head>
  <title>两步验证 — BBLBB</title>
</svelte:head>

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
        <div style="background:var(--color-bg-subtle, rgba(0,0,0,0.03));padding:var(--space-4);border-radius:var(--radius-md);display:flex;flex-direction:column;gap:var(--space-3);">
          <strong style="font-size:var(--text-base);">1. 扫描二维码或录入密钥</strong>
          <p class="text-secondary" style="margin:0;font-size:var(--text-sm);">
            在认证器中手工录入以下密钥（Base32）：
          </p>
          <code style="font-size:var(--text-base);letter-spacing:1px;padding:var(--space-2);background:var(--color-bg-card);border:var(--border-default);border-radius:var(--radius-sm);user-select:all;word-break:break-all;">
            {mfaStep.secret_base32}
          </code>
          <p class="text-tertiary" style="margin:0;font-size:var(--text-xs);word-break:break-all;">
            otpauth 链接：{mfaStep.otpauth_uri}
          </p>
        </div>

        <form method="POST" action="?/confirm" use:enhance style="display:flex;flex-direction:column;gap:var(--space-3);">
          <strong style="font-size:var(--text-base);">2. 输入 6 位动态验证码确认</strong>
          <div style="display:flex;gap:var(--space-2);align-items:center;flex-wrap:wrap;">
            <input
              type="text"
              name="code"
              class="input-field"
              placeholder="000000"
              maxlength="6"
              inputmode="numeric"
              pattern="[0-9]{6}"
              required
              style="width:140px;font-size:var(--text-lg);letter-spacing:3px;text-align:center;"
              aria-label="6 位动态验证码"
            />
            <Button text="验证并启用" variant="primary" type="submit" />
          </div>
        </form>

        <form method="POST" action="?/cancel" use:enhance>
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
