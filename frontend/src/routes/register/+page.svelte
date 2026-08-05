<script lang="ts">
  // M02-UX-01：注册页（服务端表单 action + 字段错误关联 + 统一冲突提示）。
  // - 无 JS：原生 `<form method="POST">` 提交到 +page.server.ts action；
  // - 有 JS：use:enhance 渐进增强（同一 action，避免双实现）；
  // - 字段错误经 action 返回的 fieldErrors 与输入框 aria-describedby 关联
  //   （错误元素 role=alert，M00-FRONTEND-07）；
  // - 用户名/邮箱已存在与成功统一显示成功（后端防枚举返回一致 201）。
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import type { RegisterActionData } from './+page.server';

  let { form }: { form?: RegisterActionData } = $props();

  const usernameError = $derived(form?.fieldErrors?.username ?? null);
  const emailError = $derived(form?.fieldErrors?.email ?? null);
  const passwordError = $derived(form?.fieldErrors?.password ?? null);
  const confirmError = $derived(form?.fieldErrors?.confirm ?? null);
  const topMessage = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );
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
            <input
              type="text"
              class="input-field"
              id="reg-username"
              name="username"
              placeholder="3-20 个字符，字母/数字/_/-"
              value={form?.values?.username ?? ''}
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
              value={form?.values?.email ?? ''}
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
          <Button text="注册" variant="primary" size="lg" type="submit" />
        </form>
      {/if}
    </div>
    <div class="auth-footer">
      已有账号？<a href="/login">立即登录</a>
    </div>
  </div>
</div>
