<script lang="ts">
  // M02-UX-03：登录页（服务端表单 action + 两步登录 MFA）。
  // - 无 JS：原生 form[method=POST] 提交到 +page.server.ts action；
  // - 有 JS：use:enhance 渐进增强（同一 action，无双重实现）；
  // - 启用 TOTP 的账号：第一步返回 challenge，页面切换第二步输入
  //   6 位验证码（或切换到恢复码）；
  // - 失败提示统一（后端 401 不泄漏账号是否存在/密码是否正确）。
  import { enhance } from '$app/forms';
  import { page } from '$app/state';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import type { LoginActionData } from './+page.server';

  let { form }: { form?: LoginActionData } = $props();

  const mfaStep = $derived(form?.mfa_required === true);
  let useRecovery = $state(false);
  let showPassword = $state(false);
  const topMessage = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );

  $effect(() => {
    if (form?.message) {
      showToast(topMessage ?? form.message, 'danger');
    }
  });

  // 登录后回跳目标（?next=，仅本站相对路径）：表单 action="?/xxx" 会替换
  // 整个查询串，POST 时 URL 上的 next 丢失，故经隐藏字段携带（action 端
  // 同规则校验，开放重定向安全）。
  const nextParam = $derived.by(() => {
    const n = page.url.searchParams.get('next') ?? '';
    return n.startsWith('/') && !n.startsWith('//') ? n : '';
  });

  function toggleRecovery() {
    useRecovery = !useRecovery;
  }
</script>

<svelte:head>
  <title>登录 — BBLBB</title>
</svelte:head>

<div class="login-page auth-wrapper" id="page-login">
  <section class="login-shell">
    <div class="login-card auth-card">
      <div class="login-header-group">
        <div class="login-header-meta">
          {#if mfaStep}
            <div class="login-hero-badge mfa-badge" aria-hidden="true">
              <Icon name="shield-check" size={16} />
            </div>
            <p class="login-eyebrow">TWO-FACTOR AUTH</p>
          {:else}
            <p class="login-eyebrow">WELCOME BACK</p>
          {/if}
        </div>
        <h1 tabindex="-1">{mfaStep ? '安全验证' : '登录 BBLBB'}</h1>
        <p class="login-subtitle">
          {mfaStep ? '该账号已启用两步验证保护，请输入 6 位动态验证码。' : '探索思想与灵感的中文社区空间'}
        </p>
      </div>

      <div class="auth-body">
        {#if topMessage}
          <div class="auth-alert" role="alert">
            <Icon name="alert-triangle" size={16} class="auth-alert-icon" />
            <span class="auth-alert-text">{topMessage}</span>
          </div>
        {/if}

        {#if mfaStep}
          <form method="POST" action="?/mfa" use:enhance novalidate class="login-form">
            <input type="hidden" name="challenge_token" value={form?.challenge_token ?? ''} />
            <input type="hidden" name="next" value={nextParam} />
            {#if useRecovery}
              <div class="input-wrapper">
                <label class="input-label" for="login-recovery">恢复码</label>
                <div class="input-control">
                  <input
                    type="text"
                    class="input-field recovery-input"
                    id="login-recovery"
                    name="recovery_code"
                    placeholder="16 位恢复码"
                    autocomplete="one-time-code"
                    spellcheck="false"
                  />
                </div>
              </div>
              <div class="mfa-switch-row">
                <button type="button" class="link-btn" onclick={toggleRecovery}>
                  <Icon name="key" size={14} />
                  <span>用验证码登录</span>
                </button>
              </div>
            {:else}
              <div class="input-wrapper">
                <label class="input-label" for="login-totp">验证码</label>
                <div class="input-control totp-input-control">
                  <input
                    type="text"
                    class="input-field totp-input"
                    id="login-totp"
                    name="totp_code"
                    placeholder="6 位验证码"
                    inputmode="numeric"
                    pattern="[0-9]{6}"
                    maxlength="6"
                    autocomplete="one-time-code"
                  />
                </div>
                <div class="totp-tip-box">
                  <Icon name="clock" size={13} />
                  <span>动态验证码每 30 秒自动更新，请以 App 实时值为准</span>
                </div>
              </div>
              <div class="mfa-switch-row">
                <button type="button" class="link-btn" onclick={toggleRecovery}>
                  <Icon name="key" size={14} />
                  <span>使用恢复码</span>
                </button>
              </div>
            {/if}
            <div class="submit-wrap">
              <Button text="验证并登录" variant="primary" size="lg" type="submit" block />
            </div>
          </form>
        {:else}
          <!-- M14-A11Y-08 修复：显式 action="?/login" -->
          <form method="POST" action="?/login" use:enhance novalidate class="login-form">
            <input type="hidden" name="next" value={nextParam} />
            <div class="input-wrapper">
              <label class="input-label" for="login-identifier">用户名或邮箱</label>
              <div class="input-control">
                <input
                  type="text"
                  class="input-field"
                  id="login-identifier"
                  name="identifier"
                  placeholder="用户名或邮箱"
                  autocomplete="username"
                  autocapitalize="none"
                  spellcheck="false"
                />
              </div>
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="login-password">密码</label>
              <div class="input-control password-control">
                <input
                  type={showPassword ? 'text' : 'password'}
                  class="input-field password-input"
                  id="login-password"
                  name="password"
                  placeholder="输入密码"
                  autocomplete="current-password"
                />
                <button
                  type="button"
                  class="toggle-pwd-btn"
                  aria-label={showPassword ? '隐藏密码' : '显示密码'}
                  tabindex="-1"
                  onclick={() => (showPassword = !showPassword)}
                >
                  <Icon name={showPassword ? 'eye-off' : 'eye'} size={18} />
                </button>
              </div>
            </div>
            <div class="login-options">
              <label class="remember-label">
                <input type="checkbox" name="remember" value="on" checked class="remember-checkbox" />
                <span>记住我</span>
              </label>
              <a href="/password-reset" class="forgot-link">忘记密码？</a>
            </div>
            <div class="submit-wrap">
              <Button text="登录" variant="primary" size="lg" type="submit" block />
            </div>
          </form>
        {/if}
      </div>

      <div class="login-footer-zone">
        <p class="login-signup">
          还没有账号？ <a href="/register" class="signup-link">立即注册</a>
        </p>
      </div>
    </div>
  </section>
</div>

<style>
  /* 基础与桌面样式 */
  .login-header-group {
    margin-bottom: var(--space-3, 12px);
  }

  .login-header-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 6px;
  }

  .login-hero-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background: color-mix(in srgb, var(--color-brand) 12%, transparent);
    color: var(--color-brand);
    border: 1px solid color-mix(in srgb, var(--color-brand) 22%, transparent);
    flex-shrink: 0;
  }

  .login-eyebrow {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.12em;
    color: var(--color-brand);
    text-transform: uppercase;
    margin: 0;
  }

  .login-card h1 {
    font-family: var(--font-family-serif, serif);
    font-size: 24px;
    font-weight: 600;
    color: var(--color-text-primary);
    margin: 0 0 4px;
    line-height: 1.25;
  }

  .login-subtitle {
    font-size: 13px;
    color: var(--color-text-secondary);
    line-height: 1.4;
    margin: 0;
  }

  .auth-body {
    margin-top: var(--space-3, 12px);
  }

  .auth-alert {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2, 8px);
    padding: var(--space-2, 8px) var(--space-3, 12px);
    margin-bottom: var(--space-3, 12px);
    background: var(--color-danger-soft, rgba(239, 68, 68, 0.08));
    border: 1px solid var(--color-danger-border, rgba(239, 68, 68, 0.25));
    border-radius: var(--radius-md, 8px);
    color: var(--color-danger, #ef4444);
    font-size: var(--text-sm, 13px);
    line-height: 1.4;
    word-break: break-word;
  }

  :global(.auth-alert-icon) {
    flex-shrink: 0;
    margin-top: 2px;
    color: var(--color-danger, #ef4444);
  }

  .auth-alert-text {
    flex: 1;
  }

  .input-wrapper {
    margin-bottom: var(--space-3, 12px);
  }

  .input-label {
    display: block;
    font-size: 13px;
    font-weight: 500;
    color: var(--color-text-primary);
    margin-bottom: 4px;
  }

  .input-control {
    position: relative;
    width: 100%;
  }

  .input-field {
    width: 100%;
    height: 44px;
    padding: 0 14px;
    font-size: 15px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md, 6px);
    color: var(--color-text-primary);
    box-sizing: border-box;
    transition: border-color var(--duration-fast, 0.15s), box-shadow var(--duration-fast, 0.15s);
  }

  .input-field:focus {
    outline: none;
    border-color: var(--color-brand);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--color-brand) 18%, transparent);
  }

  .password-control {
    display: flex;
    align-items: center;
  }

  .password-input {
    padding-right: 42px;
  }

  .toggle-pwd-btn {
    position: absolute;
    right: 4px;
    top: 50%;
    transform: translateY(-50%);
    width: 36px;
    height: 36px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    color: var(--color-text-tertiary);
    cursor: pointer;
    border-radius: var(--radius-sm, 4px);
    transition: color 0.15s ease;
  }

  .toggle-pwd-btn:hover {
    color: var(--color-text-primary);
  }

  .totp-input {
    font-family: var(--font-family-mono, monospace);
    font-size: 20px;
    font-weight: 600;
    letter-spacing: 0.25em;
    text-align: center;
    height: 46px;
  }

  .recovery-input {
    font-family: var(--font-family-mono, monospace);
    font-size: 14px;
    letter-spacing: 0.08em;
  }

  .totp-tip-box {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 6px;
    padding: 6px 10px;
    background: color-mix(in srgb, var(--color-brand) 6%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-brand) 16%, transparent);
    border-radius: var(--radius-sm, 6px);
    color: var(--color-text-secondary);
    font-size: 12px;
    line-height: 1.35;
  }

  .mfa-switch-row {
    display: flex;
    justify-content: flex-end;
    margin: 4px 0 10px;
  }

  .link-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: transparent;
    border: none;
    color: var(--color-brand);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    padding: 2px 4px;
    border-radius: 4px;
    transition: opacity 0.15s;
  }

  .link-btn:hover {
    opacity: 0.85;
  }

  .login-options {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2, 8px);
    margin: var(--space-1, 4px) 0 var(--space-3, 12px);
  }

  .remember-label {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    color: var(--color-text-secondary);
    cursor: pointer;
    user-select: none;
  }

  .remember-checkbox {
    width: 15px;
    height: 15px;
    accent-color: var(--color-brand);
    cursor: pointer;
  }

  .forgot-link {
    font-size: 13px;
    color: var(--color-text-secondary);
    text-decoration: none;
    transition: color 0.15s;
  }

  .forgot-link:hover {
    color: var(--color-brand);
  }

  .submit-wrap {
    margin-top: var(--space-1, 4px);
  }

  .login-footer-zone {
    margin-top: auto;
    padding-top: var(--space-3, 12px);
    text-align: center;
  }

  .login-signup {
    font-size: 13px;
    color: var(--color-text-secondary);
    margin: 0;
  }

  .signup-link {
    color: var(--color-brand);
    font-weight: 600;
    text-decoration: none;
    margin-left: 4px;
  }

  .signup-link:hover {
    text-decoration: underline;
  }

  /* 移动端单页登录（<= 767px）：严控高度，紧凑布局，单屏全显 */
  @media (max-width: 767px) {
    .login-header-group {
      margin-bottom: 8px;
      flex-shrink: 0;
    }

    .login-header-meta {
      gap: 6px;
      margin-bottom: 4px;
    }

    .login-hero-badge {
      width: 24px;
      height: 24px;
    }

    .login-card h1 {
      font-size: 22px !important;
      margin: 0 0 2px !important;
      line-height: 1.2 !important;
    }

    .login-subtitle {
      font-size: 12px;
      line-height: 1.35;
    }

    .auth-body {
      margin-top: 8px;
      flex-shrink: 0;
    }

    .input-wrapper {
      margin-bottom: 8px;
    }

    .input-label {
      font-size: 13px;
      margin-bottom: 3px;
    }

    /* 移动端输入框防缩放（16px）与紧凑舒适高度（42px） */
    .input-field {
      height: 42px !important;
      font-size: 16px !important;
      padding: 0 12px !important;
      border-radius: var(--radius-md, 6px) !important;
    }

    .password-input {
      padding-right: 42px !important;
    }

    .toggle-pwd-btn {
      width: 38px;
      height: 38px;
      right: 2px;
    }

    .totp-input {
      height: 44px !important;
      font-size: 20px !important;
      letter-spacing: 0.25em !important;
    }

    .login-options {
      margin: 4px 0 10px;
    }

    .remember-label,
    .forgot-link {
      font-size: 13px;
    }

    .login-footer-zone {
      margin-top: auto;
      padding-top: 8px;
      padding-bottom: 2px;
      flex-shrink: 0;
    }

    .login-signup {
      font-size: 13px;
    }
  }
</style>
