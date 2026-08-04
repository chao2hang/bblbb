<script lang="ts">
  import { goto } from '$app/navigation';
  import { register } from '$lib/api/client';
  import { problemText, fieldError, type Problem } from '$lib/errors';
  import Button from '$lib/components/ui/Button.svelte';

  let username = $state('');
  let email = $state('');
  let password = $state('');
  let confirm = $state('');
  let problem = $state<Problem | null>(null);
  let error = $state('');
  let ok = $state(false);
  let submitting = $state(false);

  const usernameError = $derived(fieldError(problem, 'username'));
  const emailError = $derived(fieldError(problem, 'email'));
  const passwordError = $derived(fieldError(problem, 'password'));

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    if (password !== confirm) {
      error = '两次输入的密码不一致';
      return;
    }
    if (!username.trim() || !email.trim() || !password) return;
    submitting = true;
    error = '';
    problem = null;
    try {
      await register(fetch, username.trim(), email.trim(), password);
      ok = true;
    } catch (err: unknown) {
      problem = err as Problem;
      error = problemText(problem);
    }
    submitting = false;
  }
</script>

<svelte:head>
  <title>注册 — BBLBB</title>
</svelte:head>

<div class="auth-wrapper">
  <div class="auth-card">
    <div class="auth-header">
      <div class="auth-logo">BBLBB</div>
      <div class="auth-title">创建账号</div>
      <div class="auth-subtitle">加入我们，开启你的社区之旅</div>
    </div>
    <div class="auth-body">
      {#if ok}
        <div class="empty-state">
          <div class="empty-state-title">注册成功</div>
          <div class="empty-state-desc">验证邮件已发送（本地环境无 SMTP 时，账号需由管理员验证）。</div>
          <div style="margin-top:var(--space-3);display:flex;gap:var(--space-2);justify-content:center;">
            <Button text="去登录" variant="primary" size="sm" href="/login" />
          </div>
        </div>
      {:else}
        <form class="auth-form" onsubmit={handleSubmit}>
          <div class="input-wrapper">
            <label class="input-label" for="reg-username">用户名</label>
            <input type="text" class="input-field" id="reg-username" placeholder="3-20 个字符" bind:value={username} autocomplete="username" aria-describedby={usernameError ? 'reg-username-error' : undefined} />
            {#if usernameError}<p class="input-hint is-error" id="reg-username-error">{usernameError}</p>{/if}
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="reg-email">邮箱</label>
            <input type="email" class="input-field" id="reg-email" placeholder="用于验证和找回密码" bind:value={email} autocomplete="email" aria-describedby={emailError ? 'reg-email-error' : undefined} />
            {#if emailError}<p class="input-hint is-error" id="reg-email-error">{emailError}</p>{/if}
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="reg-password">密码</label>
            <input type="password" class="input-field" id="reg-password" placeholder="至少 6 位" bind:value={password} autocomplete="new-password" aria-describedby={passwordError ? 'reg-password-error' : undefined} />
            {#if passwordError}<p class="input-hint is-error" id="reg-password-error">{passwordError}</p>{/if}
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="reg-confirm">确认密码</label>
            <input type="password" class="input-field" id="reg-confirm" placeholder="再次输入密码" bind:value={confirm} autocomplete="new-password" />
          </div>
          {#if error}<p class="input-hint is-error" role="alert">{error}</p>{/if}
          <Button text={submitting ? '注册中…' : '注册'} variant="primary" size="lg" type="submit" disabled={submitting} />
        </form>
      {/if}
    </div>
    <div class="auth-footer">
      已有账号？<a href="/login">立即登录</a>
    </div>
  </div>
</div>
