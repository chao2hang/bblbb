<script lang="ts">
  // M02-UX-01：注册页（服务端表单 action + 字段错误关联 + 统一冲突提示）。
  // - 无 JS：原生 `<form method="POST">` 提交到 +page.server.ts action；
  // - 有 JS：use:enhance 渐进增强（同一 action，避免双实现）；
  // - 字段错误经 action 返回的 fieldErrors 与输入框 aria-describedby 关联
  //   （错误元素 role=alert，M00-FRONTEND-07）；
  // - 用户名/邮箱已存在与成功统一显示成功（后端防枚举返回一致 201）。
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import { resolveSiteCopy, pageTitle, type SiteCopyView } from '$lib/site/copy';
  import type { RegisterActionData } from './+page.server';

  // data 可选：隔离渲染（vitest）只传 form；运行时恒有（layout 注入 site）。
  let { data, form }: { data?: { site?: SiteCopyView | null }; form?: RegisterActionData } = $props();

  // 全站文案（0065）：注册页眉题/标题/说明来自后台系统设置（layout 注入；
  // 隔离渲染/后端不可达时解析内置兜底）。
  const site = $derived<SiteCopyView>(data?.site ?? resolveSiteCopy(null));

  const usernameError = $derived(form?.fieldErrors?.username ?? null);
  const emailError = $derived(form?.fieldErrors?.email ?? null);
  const passwordError = $derived(form?.fieldErrors?.password ?? null);
  const confirmError = $derived(form?.fieldErrors?.confirm ?? null);
  const topMessage = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );

  /** 密码可见性切换 */
  let showPassword = $state(false);
  let showConfirmPassword = $state(false);

  /** M18-MISC-01：同意社区规则（对齐原型表单控件）。 */
  let agreed = $state(true);

  $effect(() => {
    if (form?.message) {
      showToast(topMessage ?? form.message, 'danger');
    }
  });
</script>

<svelte:head>
  <title>{pageTitle('注册', site.siteName)}</title>
</svelte:head>

<div class="login-page auth-wrapper" id="page-register">
  <!-- 柔和环境光晕与细腻网格纹理 -->
  <div class="auth-ambient-glow auth-ambient-glow--1" aria-hidden="true"></div>
  <div class="auth-ambient-glow auth-ambient-glow--2" aria-hidden="true"></div>
  <div class="auth-grid-overlay" aria-hidden="true"></div>

  <section class="login-shell">
    <!-- 顶部返回论坛导航（无全局顶栏时保留清晰出口） -->
    <div class="auth-top-action">
      <a href="/" class="auth-back-link" title="返回论坛首页">
        <Icon name="chevron-left" size={16} />
        <span>返回论坛</span>
      </a>
    </div>

    <div class="login-card auth-card">
      <div class="login-header-group">
        <div class="brand-badge-row">
          <div class="brand-avatar" aria-hidden="true">
            <span class="brand-initial">{(site.siteName || 'B').slice(0, 1)}</span>
          </div>
          <div class="brand-meta">
            <span class="brand-site-tag">{site.siteName}</span>
            <span class="login-eyebrow">{site.registerEyebrow}</span>
          </div>
        </div>
        <h1 tabindex="-1">{site.registerTitle}</h1>
        <p class="login-subtitle">{site.registerSubtitle}</p>
      </div>

      <div class="auth-body">
        {#if topMessage}
          <div class="auth-alert" role="alert">
            <Icon name="alert-triangle" size={16} class="auth-alert-icon" />
            <span class="auth-alert-text">{topMessage}</span>
          </div>
        {/if}

        {#if form?.ok}
          <div class="register-success-pane">
            <div class="success-avatar" aria-hidden="true">
              <Icon name="check-circle" size={32} class="success-icon" />
            </div>
            <div class="empty-state-title success-title">注册成功</div>
            <div class="empty-state-desc success-desc">
              验证邮件已发送（本地环境无 SMTP 时，账号需由管理员验证）。
            </div>
            <div class="success-action-wrap">
              <Button text="去登录" variant="primary" size="md" href="/login" block />
            </div>
            <div class="empty-state-desc success-resend-tip">
              没有收到验证邮件？<a href="/verify-email" class="resend-link">重新发送</a>
            </div>
          </div>
        {:else}
          <form method="POST" use:enhance novalidate class="login-form">
            <div class="input-wrapper">
              <label class="input-label" for="reg-username">用户名</label>
              <div class="input-control has-icon">
                <span class="input-leading-icon" aria-hidden="true">
                  <Icon name="user" size={16} />
                </span>
                <input
                  type="text"
                  class="input-field"
                  class:is-invalid={!!usernameError}
                  id="reg-username"
                  name="username"
                  placeholder="3-20 个字符，字母/数字/_/-"
                  value={form?.fieldErrors ? form.values?.username ?? '' : undefined}
                  autocomplete="username"
                  autocapitalize="none"
                  spellcheck="false"
                  aria-invalid={usernameError ? 'true' : undefined}
                  aria-describedby={usernameError ? 'reg-username-error' : undefined}
                />
              </div>
              {#if usernameError}
                <p class="input-hint is-error" id="reg-username-error" role="alert">{usernameError}</p>
              {/if}
            </div>

            <div class="input-wrapper">
              <label class="input-label" for="reg-email">邮箱</label>
              <div class="input-control has-icon">
                <span class="input-leading-icon" aria-hidden="true">
                  <Icon name="mail" size={16} />
                </span>
                <input
                  type="email"
                  class="input-field"
                  class:is-invalid={!!emailError}
                  id="reg-email"
                  name="email"
                  placeholder="用于验证和找回密码"
                  value={form?.fieldErrors ? form.values?.email ?? '' : undefined}
                  autocomplete="email"
                  autocapitalize="none"
                  spellcheck="false"
                  aria-invalid={emailError ? 'true' : undefined}
                  aria-describedby={emailError ? 'reg-email-error' : undefined}
                />
              </div>
              {#if emailError}
                <p class="input-hint is-error" id="reg-email-error" role="alert">{emailError}</p>
              {/if}
            </div>

            <div class="input-wrapper">
              <label class="input-label" for="reg-password">密码</label>
              <div class="input-control has-icon password-control">
                <span class="input-leading-icon" aria-hidden="true">
                  <Icon name="lock" size={16} />
                </span>
                <input
                  type={showPassword ? 'text' : 'password'}
                  class="input-field password-input"
                  class:is-invalid={!!passwordError}
                  id="reg-password"
                  name="password"
                  placeholder="8-128 位，须含字母和数字"
                  autocomplete="new-password"
                  aria-invalid={passwordError ? 'true' : undefined}
                  aria-describedby={passwordError ? 'reg-password-error' : undefined}
                />
                <button
                  type="button"
                  class="toggle-pwd-btn"
                  aria-label={showPassword ? '隐藏输入内容' : '显示输入内容'}
                  title={showPassword ? '隐藏密码' : '显示密码'}
                  tabindex="-1"
                  onclick={() => (showPassword = !showPassword)}
                >
                  <Icon name={showPassword ? 'eye-off' : 'eye'} size={18} />
                </button>
              </div>
              {#if passwordError}
                <p class="input-hint is-error" id="reg-password-error" role="alert">{passwordError}</p>
              {/if}
            </div>

            <div class="input-wrapper">
              <label class="input-label" for="reg-confirm">确认密码</label>
              <div class="input-control has-icon password-control">
                <span class="input-leading-icon" aria-hidden="true">
                  <Icon name="lock" size={16} />
                </span>
                <input
                  type={showConfirmPassword ? 'text' : 'password'}
                  class="input-field password-input"
                  class:is-invalid={!!confirmError}
                  id="reg-confirm"
                  name="confirm"
                  placeholder="再次输入密码"
                  autocomplete="new-password"
                  aria-invalid={confirmError ? 'true' : undefined}
                  aria-describedby={confirmError ? 'reg-confirm-error' : undefined}
                />
                <button
                  type="button"
                  class="toggle-pwd-btn"
                  aria-label={showConfirmPassword ? '隐藏输入内容' : '显示输入内容'}
                  title={showConfirmPassword ? '隐藏密码' : '显示密码'}
                  tabindex="-1"
                  onclick={() => (showConfirmPassword = !showConfirmPassword)}
                >
                  <Icon name={showConfirmPassword ? 'eye-off' : 'eye'} size={18} />
                </button>
              </div>
              {#if confirmError}
                <p class="input-hint is-error" id="reg-confirm-error" role="alert">{confirmError}</p>
              {/if}
            </div>

            <!-- M18-MISC-01：对齐原型注册页「我已阅读并同意 社区规则」 -->
            <div class="agreement-wrapper">
              <label class="agreement-label" for="reg-agree">
                <input type="checkbox" id="reg-agree" name="agree" bind:checked={agreed} class="agreement-checkbox" />
                <span>我已阅读并同意 <a href="/posts/rules" class="rules-link" target="_blank" rel="noopener">社区规则</a></span>
              </label>
            </div>

            <div class="submit-wrap">
              <Button text="注册" variant="primary" size="lg" type="submit" block disabled={!agreed} />
            </div>
          </form>

          {#if site.googleLoginEnabled || site.githubLoginEnabled}
            <div class="oauth-divider">
              <span class="oauth-divider-line"></span>
              <span class="oauth-divider-text">快捷注册方式</span>
              <span class="oauth-divider-line"></span>
            </div>
            <div class="oauth-buttons">
              {#if site.googleLoginEnabled}
                <a href="/api/v1/auth/oauth/google/start" class="oauth-btn oauth-google" data-provider="google">
                  <svg class="oauth-icon" viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
                    <path fill="#4285F4" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z"/>
                    <path fill="#34A853" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"/>
                    <path fill="#FBBC05" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.06H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.94l2.85-2.22.81-.63z"/>
                    <path fill="#EA4335" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.06l3.66 2.84c.87-2.6 3.3-4.52 6.16-4.52z"/>
                  </svg>
                  <span>Google 快捷注册 / 登录</span>
                </a>
              {/if}
              {#if site.githubLoginEnabled}
                <a href="/api/v1/auth/oauth/github/start" class="oauth-btn oauth-github" data-provider="github">
                  <svg class="oauth-icon" viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
                    <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z"/>
                  </svg>
                  <span>GitHub 快捷注册 / 登录</span>
                </a>
              {/if}
            </div>
          {/if}
        {/if}
      </div>

      <div class="login-footer-zone">
        <p class="login-signup">
          已有账号？ <a href="/login" class="signup-link">立即登录</a>
        </p>
        <div class="auth-security-trust">
          <Icon name="shield-check" size={13} />
          <span>密码经高强度 Argon2id 哈希安全保护</span>
        </div>
      </div>
    </div>
  </section>
</div>

<style>
  /* 基础容器与全屏环境 */
  .login-page {
    position: relative;
    min-height: 100vh;
    min-height: 100dvh;
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 36px 20px;
    box-sizing: border-box;
    overflow-x: hidden;
    background: var(--color-bg-page);
  }

  /* 柔和背景光晕：深邃多层渐变 */
  .auth-ambient-glow {
    position: absolute;
    border-radius: 50%;
    pointer-events: none;
    z-index: 0;
  }

  .auth-ambient-glow--1 {
    top: 18%;
    left: 50%;
    transform: translateX(-50%);
    width: 640px;
    height: 440px;
    background: radial-gradient(circle, color-mix(in srgb, var(--color-brand) 18%, transparent) 0%, transparent 70%);
    filter: blur(70px);
  }

  .auth-ambient-glow--2 {
    bottom: 12%;
    right: 18%;
    width: 380px;
    height: 380px;
    background: radial-gradient(circle, color-mix(in srgb, #6366f1 10%, transparent) 0%, transparent 70%);
    filter: blur(80px);
  }

  /* 极细微网格纹理：增加社区高质感质感层 */
  .auth-grid-overlay {
    position: absolute;
    inset: 0;
    pointer-events: none;
    background-image: radial-gradient(rgba(255, 255, 255, 0.05) 1px, transparent 1px);
    background-size: 28px 28px;
    mask-image: radial-gradient(ellipse at 50% 50%, black 40%, transparent 80%);
    -webkit-mask-image: radial-gradient(ellipse at 50% 50%, black 40%, transparent 80%);
    z-index: 0;
  }

  /* 壳体与卡片 */
  .login-shell {
    position: relative;
    z-index: 1;
    width: 100%;
    max-width: 440px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
  }

  /* 顶部返回论坛导航 */
  .auth-top-action {
    display: flex;
    align-items: center;
    margin-bottom: 14px;
  }

  .auth-back-link {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 14px 6px 10px;
    border-radius: 9999px;
    background: color-mix(in srgb, var(--color-bg-card) 60%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-border) 60%, transparent);
    color: var(--color-text-secondary);
    font-size: 13px;
    font-weight: 500;
    text-decoration: none;
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
    transition: all 0.2s ease;
  }

  .auth-back-link:hover {
    color: var(--color-text-primary);
    background: var(--color-bg-card);
    border-color: var(--color-brand);
    transform: translateX(-2px);
  }

  .login-card {
    width: 100%;
    padding: 34px 32px 28px;
    background: color-mix(in srgb, var(--color-bg-card) 94%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-border) 75%, rgba(255, 255, 255, 0.08));
    border-radius: 16px;
    box-shadow:
      0 20px 40px -15px rgba(0, 0, 0, 0.45),
      0 0 0 1px rgba(255, 255, 255, 0.04) inset,
      0 0 50px -15px color-mix(in srgb, var(--color-brand) 12%, transparent);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    box-sizing: border-box;
    transition: box-shadow 0.3s ease;
  }

  /* 头部与品牌徽标 */
  .login-header-group {
    margin-bottom: var(--space-4, 16px);
  }

  .brand-badge-row {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 14px;
  }

  .brand-avatar {
    width: 38px;
    height: 38px;
    border-radius: 10px;
    background: linear-gradient(135deg, var(--color-brand), color-mix(in srgb, var(--color-brand) 65%, #6366f1));
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 4px 12px -2px color-mix(in srgb, var(--color-brand) 40%, transparent);
    flex-shrink: 0;
  }

  .brand-initial {
    font-size: 19px;
    font-weight: 700;
    color: #ffffff;
    font-family: var(--font-family-mono, monospace);
    line-height: 1;
  }

  .brand-meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .brand-site-tag {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-primary);
    line-height: 1.2;
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
    font-size: 24px;
    font-weight: 700;
    color: var(--color-text-primary);
    margin: 0 0 6px;
    line-height: 1.25;
    letter-spacing: -0.01em;
  }

  .login-subtitle {
    font-size: 13px;
    color: var(--color-text-secondary);
    line-height: 1.45;
    margin: 0;
  }

  .auth-body {
    margin-top: var(--space-3, 12px);
  }

  .auth-alert {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2, 8px);
    padding: 10px 14px;
    margin-bottom: var(--space-3, 12px);
    background: var(--color-danger-soft, rgba(239, 68, 68, 0.08));
    border: 1px solid var(--color-danger-border, rgba(239, 68, 68, 0.25));
    border-radius: 8px;
    color: var(--color-danger, #ef4444);
    font-size: 13px;
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

  /* 表单输入控件 */
  .input-wrapper {
    margin-bottom: 14px;
  }

  .input-label {
    display: block;
    font-size: 13px;
    font-weight: 500;
    color: var(--color-text-primary);
    margin-bottom: 6px;
  }

  .input-control {
    position: relative;
    width: 100%;
  }

  .input-leading-icon {
    position: absolute;
    left: 14px;
    top: 50%;
    transform: translateY(-50%);
    color: var(--color-text-tertiary);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
    transition: color 0.2s ease;
    z-index: 2;
  }

  .input-control.has-icon:focus-within .input-leading-icon {
    color: var(--color-brand);
  }

  .input-field {
    width: 100%;
    height: 44px;
    padding: 0 14px;
    font-size: 14px;
    background: color-mix(in srgb, var(--color-bg-card) 70%, var(--color-bg-page));
    border: 1px solid var(--color-border);
    border-radius: 8px;
    color: var(--color-text-primary);
    box-sizing: border-box;
    transition: border-color 0.2s ease, box-shadow 0.2s ease, background-color 0.2s ease;
  }

  .input-control.has-icon .input-field {
    padding-left: 42px !important;
  }

  .input-control.has-icon.password-control .input-field {
    padding-left: 42px !important;
    padding-right: 44px !important;
  }

  .input-field:hover {
    border-color: color-mix(in srgb, var(--color-border) 70%, var(--color-text-primary));
  }

  .input-field:focus {
    outline: none;
    background: var(--color-bg-card);
    border-color: var(--color-brand);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--color-brand) 20%, transparent);
  }

  .input-field.is-invalid {
    border-color: var(--color-danger, #ef4444);
  }

  .input-field.is-invalid:focus {
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--color-danger, #ef4444) 20%, transparent);
  }

  .input-hint.is-error {
    font-size: 12px;
    color: var(--color-danger, #ef4444);
    margin: 5px 0 0;
    line-height: 1.35;
  }

  .toggle-pwd-btn {
    position: absolute;
    right: 5px;
    top: 50%;
    transform: translateY(-50%);
    width: 34px;
    height: 34px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    color: var(--color-text-tertiary);
    cursor: pointer;
    border-radius: 6px;
    transition: color 0.15s ease, background-color 0.15s ease;
    z-index: 2;
  }

  .toggle-pwd-btn:hover {
    color: var(--color-text-primary);
    background: color-mix(in srgb, var(--color-text-tertiary) 12%, transparent);
  }

  /* 社区规则勾选 */
  .agreement-wrapper {
    margin: 10px 0 16px;
  }

  .agreement-label {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--color-text-secondary);
    cursor: pointer;
    user-select: none;
    line-height: 1.4;
  }

  .agreement-checkbox {
    width: 16px;
    height: 16px;
    accent-color: var(--color-brand);
    cursor: pointer;
    flex-shrink: 0;
  }

  .rules-link {
    color: var(--color-brand);
    text-decoration: underline;
    text-underline-offset: 3px;
    font-weight: 500;
    transition: color 0.15s ease;
  }

  .rules-link:hover {
    filter: brightness(1.15);
  }

  /* 提交按钮 */
  .submit-wrap {
    margin-top: 6px;
  }

  .submit-wrap :global(.btn-primary) {
    height: 44px;
    background: linear-gradient(135deg, var(--color-brand), color-mix(in srgb, var(--color-brand) 80%, #6366f1));
    border: none;
    border-radius: 8px;
    font-size: 15px;
    font-weight: 600;
    letter-spacing: 0.02em;
    box-shadow: 0 4px 14px -2px color-mix(in srgb, var(--color-brand) 38%, transparent);
    transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .submit-wrap :global(.btn-primary:hover:not(:disabled)) {
    filter: brightness(1.08);
    transform: translateY(-1px);
    box-shadow: 0 6px 20px -2px color-mix(in srgb, var(--color-brand) 48%, transparent);
  }

  .submit-wrap :global(.btn-primary:active:not(:disabled)) {
    transform: translateY(1px);
    filter: brightness(0.96);
  }

  .submit-wrap :global(.btn-primary:disabled) {
    opacity: 0.55;
    cursor: not-allowed;
    box-shadow: none;
    transform: none;
  }

  /* OAuth 区域 */
  .oauth-divider {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 20px 0 16px;
  }

  .oauth-divider-line {
    flex: 1;
    height: 1px;
    background: linear-gradient(to right, transparent, var(--color-border), transparent);
  }

  .oauth-divider-text {
    font-size: 12px;
    color: var(--color-text-tertiary);
    font-weight: 500;
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
    height: 42px;
    padding: 0 16px;
    border-radius: 8px;
    border: 1px solid var(--color-border);
    background: color-mix(in srgb, var(--color-bg-card) 60%, var(--color-bg-page));
    color: var(--color-text-primary);
    font-size: 13px;
    font-weight: 500;
    text-decoration: none;
    transition: all 0.2s ease;
    cursor: pointer;
    box-sizing: border-box;
  }

  .oauth-btn:hover {
    background: var(--color-surface-hover, var(--color-bg-card));
    border-color: color-mix(in srgb, var(--color-border) 60%, var(--color-brand));
    transform: translateY(-1px);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  }

  .oauth-icon {
    flex-shrink: 0;
  }

  /* 成功反馈面板 */
  .register-success-pane {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    padding: 16px 8px 10px;
  }

  .success-avatar {
    width: 56px;
    height: 56px;
    border-radius: 50%;
    background: color-mix(in srgb, #10b981 14%, transparent);
    border: 1px solid color-mix(in srgb, #10b981 32%, transparent);
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: 16px;
    color: #10b981;
    box-shadow: 0 4px 16px -2px color-mix(in srgb, #10b981 25%, transparent);
  }

  :global(.success-icon) {
    color: #10b981;
  }

  .success-title {
    font-size: 20px;
    font-weight: 700;
    color: var(--color-text-primary);
    margin: 0 0 8px;
  }

  .success-desc {
    font-size: 13px;
    color: var(--color-text-secondary);
    line-height: 1.5;
    margin: 0 0 20px;
    max-width: 320px;
  }

  .success-action-wrap {
    width: 100%;
    margin-bottom: 14px;
  }

  .success-resend-tip {
    font-size: 12px;
    color: var(--color-text-tertiary);
    margin: 0;
  }

  .resend-link {
    color: var(--color-brand);
    text-decoration: underline;
    margin-left: 4px;
  }

  /* 卡片底部区域 */
  .login-footer-zone {
    margin-top: 18px;
    padding-top: 14px;
    border-top: 1px solid color-mix(in srgb, var(--color-border) 60%, transparent);
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

  .auth-security-trust {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    margin-top: 12px;
    color: var(--color-text-tertiary);
    font-size: 12px;
  }

  /* 移动端优化 */
  @media (max-width: 767px) {
    .login-page {
      padding: 24px 16px 28px;
      align-items: flex-start;
    }

    .auth-top-action {
      margin-bottom: 12px;
    }

    .login-card {
      padding: 24px 20px 20px;
      border-radius: 14px;
    }

    .login-card h1 {
      font-size: 21px !important;
    }

    .auth-ambient-glow--1 {
      width: 320px;
      height: 320px;
    }
  }
</style>
