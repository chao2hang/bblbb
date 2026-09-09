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
  import { resolveSiteCopy, pageTitle, type SiteCopyView } from '$lib/site/copy';
  import type { LoginActionData } from './+page.server';

  // data 可选：隔离渲染（vitest）只传 form；运行时恒有（layout 注入 site）。
  let { data, form }: { data?: { site?: SiteCopyView | null }; form?: LoginActionData } = $props();

  // 全站文案（0065）：登录页眉题/标题/说明来自后台系统设置（layout 注入；
  // 隔离渲染/后端不可达时解析内置兜底）。
  const site = $derived<SiteCopyView>(data?.site ?? resolveSiteCopy(null));

  const mfaStep = $derived(form?.mfa_required === true);
  let useRecovery = $state(false);
  let showPassword = $state(false);

  const oauthErrorParam = $derived(page.url.searchParams.get('error'));
  const oauthErrorMessage = $derived.by(() => {
    switch (oauthErrorParam) {
      case 'cancelled':
        return '已取消第三方授权登录';
      case 'invalid_state':
        return '授权状态已失效，请重新尝试';
      case 'user_blocked':
        return '该账号已被停用，无法登录';
      case 'oauth_disabled':
        return '该第三方登录方式已被管理员禁用';
      case 'oauth_failed':
      case 'oauth_profile_failed':
        return '第三方登录服务暂时不可用，请稍后重试';
      default:
        return oauthErrorParam ? '第三方登录失败，请稍后重试' : null;
    }
  });

  const topMessage = $derived(
    oauthErrorMessage
      ? oauthErrorMessage
      : form?.message
        ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message)
        : null
  );

  $effect(() => {
    if (oauthErrorMessage) {
      showToast(oauthErrorMessage, 'danger');
    } else if (form?.message) {
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
  <title>{pageTitle('登录', site.siteName)}</title>
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
            <p class="login-eyebrow">{site.loginEyebrow}</p>
          {/if}
        </div>
        <h1 tabindex="-1">{mfaStep ? '安全验证' : site.loginTitle}</h1>
        <p class="login-subtitle">
          {mfaStep ? '该账号已启用两步验证保护，请输入 6 位动态验证码。' : site.loginSubtitle}
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

        {#if !mfaStep && (site.googleLoginEnabled || site.githubLoginEnabled)}
          <div class="oauth-divider">
            <span class="oauth-divider-line"></span>
            <span class="oauth-divider-text">其他登录方式</span>
            <span class="oauth-divider-line"></span>
          </div>
          <div class="oauth-buttons">
            {#if site.googleLoginEnabled}
              <a
                href="/api/v1/auth/oauth/google/start{nextParam ? `?next=${encodeURIComponent(nextParam)}` : ''}"
                class="oauth-btn oauth-google"
                data-provider="google"
              >
                <svg class="oauth-icon" viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
                  <path fill="#4285F4" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z"/>
                  <path fill="#34A853" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"/>
                  <path fill="#FBBC05" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.06H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.94l2.85-2.22.81-.63z"/>
                  <path fill="#EA4335" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.06l3.66 2.84c.87-2.6 3.3-4.52 6.16-4.52z"/>
                </svg>
                <span>Google 账号登录</span>
              </a>
            {/if}
            {#if site.githubLoginEnabled}
              <a
                href="/api/v1/auth/oauth/github/start{nextParam ? `?next=${encodeURIComponent(nextParam)}` : ''}"
                class="oauth-btn oauth-github"
                data-provider="github"
              >
                <svg class="oauth-icon" viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
                  <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z"/>
                </svg>
                <span>GitHub 账号登录</span>
              </a>
            {/if}
          </div>
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

  .oauth-divider {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 18px 0 14px;
  }

  .oauth-divider-line {
    flex: 1;
    height: 1px;
    background: var(--color-border, rgba(0, 0, 0, 0.1));
  }

  .oauth-divider-text {
    font-size: 12px;
    color: var(--color-text-secondary, #666);
    white-space: nowrap;
  }

  .oauth-buttons {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-bottom: 6px;
  }

  .oauth-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    width: 100%;
    padding: 8px 16px;
    border-radius: var(--radius-md, 6px);
    border: 1px solid var(--color-border, #e5e5e5);
    background: var(--color-surface, #fff);
    color: var(--color-text-primary, #111);
    font-size: 13px;
    font-weight: 500;
    text-decoration: none;
    transition: background 0.15s ease, border-color 0.15s ease, box-shadow 0.15s ease;
    cursor: pointer;
    box-sizing: border-box;
  }

  .oauth-btn:hover {
    background: var(--color-bg-secondary, #f8f9fa);
    border-color: color-mix(in srgb, var(--color-text-primary) 30%, transparent);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.06);
  }

  .oauth-icon {
    flex-shrink: 0;
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
