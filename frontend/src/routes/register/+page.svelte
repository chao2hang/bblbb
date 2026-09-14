<script lang="ts">
  // M02-UX-01：注册页（服务端表单 action + 字段错误关联 + 统一冲突提示）。
  // - 无 JS：原生 `<form method="POST">` 提交到 +page.server.ts action；
  // - 有 JS：use:enhance 渐进增强（同一 action，避免双实现）；
  // - 字段错误经 action 返回的 fieldErrors 与输入框 aria-describedby 关联
  //   （错误元素 role=alert，M00-FRONTEND-07）；
  // - 用户名/邮箱已存在与成功统一显示成功（后端防枚举返回一致 201）。
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
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

  /** M18-MISC-01：同意社区规则（对齐原型表单控件）。 */
  let agreed = $state(true);
</script>

<svelte:head>
  <title>{pageTitle('注册', site.siteName)}</title>
</svelte:head>

<div class="login-page auth-wrapper" id="page-register">
  <section class="login-shell">
    <div class="login-card auth-card">
      <p class="login-eyebrow">{site.registerEyebrow}</p>
      <h1 tabindex="-1">{site.registerTitle}</h1>
      <p class="login-subtitle">{site.registerSubtitle}</p>
      <div class="auth-body">
      {#if form?.ok}
        <div class="empty-state">
          <div class="empty-state-title">注册成功</div>
          <div class="empty-state-desc">验证邮件已发送（本地环境无 SMTP 时，账号需由管理员验证）。</div>
          <div style="margin-top:var(--space-3);display:flex;gap:var(--space-2);justify-content:center;">
            <Button text="去登录" variant="primary" size="sm" href="/login" />
          </div>
          <div class="empty-state-desc" style="margin-top:var(--space-3);">
            没有收到验证邮件？<a href="/verify-email">重新发送</a>
          </div>
        </div>
      {:else}
        <form method="POST" use:enhance novalidate>
          {#if topMessage}
            <p class="input-hint is-error" role="alert">{topMessage}</p>
          {/if}
          <div class="input-wrapper">
            <label class="input-label" for="reg-username">用户名</label>
            <!-- M14-A11Y-08 修复：value 仅在表单响应返回后受控（回填），
                 初始无 form 时为非受控输入 —— 避免 hydration 重渲染把
                 用户早期输入重置为空（快速输入/无 JS 退化场景）。 -->
            <input
              type="text"
              class="input-field"
              id="reg-username"
              name="username"
              placeholder="3-20 个字符，字母/数字/_/-"
              value={form?.fieldErrors ? form.values?.username ?? '' : undefined}
              autocomplete="username"
              aria-invalid={usernameError ? 'true' : undefined}
              aria-describedby={usernameError ? 'reg-username-error' : undefined}
            />
            {#if usernameError}
              <p class="input-hint is-error" id="reg-username-error" role="alert">{usernameError}</p>
            {/if}
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="reg-email">邮箱</label>
            <input
              type="email"
              class="input-field"
              id="reg-email"
              name="email"
              placeholder="用于验证和找回密码"
              value={form?.fieldErrors ? form.values?.email ?? '' : undefined}
              autocomplete="email"
              aria-invalid={emailError ? 'true' : undefined}
              aria-describedby={emailError ? 'reg-email-error' : undefined}
            />
            {#if emailError}
              <p class="input-hint is-error" id="reg-email-error" role="alert">{emailError}</p>
            {/if}
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="reg-password">密码</label>
            <input
              type="password"
              class="input-field"
              id="reg-password"
              name="password"
              placeholder="8-128 位，须含字母和数字"
              autocomplete="new-password"
              aria-invalid={passwordError ? 'true' : undefined}
              aria-describedby={passwordError ? 'reg-password-error' : undefined}
            />
            {#if passwordError}
              <p class="input-hint is-error" id="reg-password-error" role="alert">{passwordError}</p>
            {/if}
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="reg-confirm">确认密码</label>
            <input
              type="password"
              class="input-field"
              id="reg-confirm"
              name="confirm"
              placeholder="再次输入密码"
              autocomplete="new-password"
              aria-invalid={confirmError ? 'true' : undefined}
              aria-describedby={confirmError ? 'reg-confirm-error' : undefined}
            />
            {#if confirmError}
              <p class="input-hint is-error" id="reg-confirm-error" role="alert">{confirmError}</p>
            {/if}
          </div>
          <!-- M18-MISC-01：对齐原型注册页「我已阅读并同意 社区规则」 -->
          <div style="display:flex;align-items:center;gap:var(--space-2);margin-bottom:var(--space-4);">
            <input type="checkbox" id="reg-agree" name="agree" bind:checked={agreed} />
            <label for="reg-agree" style="font-size:var(--text-sm);color:var(--color-text-secondary);cursor:pointer;user-select:none;">
              我已阅读并同意 <a href="/posts/rules" class="text-link" target="_blank" rel="noopener">社区规则</a>
            </label>
          </div>
          <Button text="注册" variant="primary" size="lg" type="submit" block disabled={!agreed} />
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
      <p class="login-signup">
        已有账号？ <a href="/login">立即登录</a>
      </p>
    </div>
  </section>
</div>

<style>
  /* 与登录页 .login-subtitle 同视觉（登录页样式为 scoped，不跨页共享）。 */
  .login-subtitle {
    font-size: 13px;
    color: var(--color-text-secondary);
    line-height: 1.4;
    margin: 0;
  }

  .login-signup a {
    text-decoration: underline;
    text-underline-offset: 3px;
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
    background: var(--color-border);
  }

  .oauth-divider-text {
    font-size: 12px;
    color: var(--color-text-secondary);
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
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border);
    background: var(--color-bg-card);
    color: var(--color-text-primary);
    font-size: 13px;
    font-weight: 500;
    text-decoration: none;
    transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease;
    cursor: pointer;
    box-sizing: border-box;
  }

  .oauth-btn:hover {
    background: var(--color-surface-hover);
    border-color: var(--color-border-strong);
    color: var(--color-text-primary);
  }

  .oauth-icon {
    flex-shrink: 0;
  }

  @media (max-width: 767px) {
    /* 注册字段较多，连续排版比把内容均匀拉满视口更易扫描。 */
    :global(#page-register .login-card) {
      justify-content: flex-start !important;
    }

    :global(#page-register .login-signup) {
      margin-top: var(--space-6);
    }
  }
</style>
