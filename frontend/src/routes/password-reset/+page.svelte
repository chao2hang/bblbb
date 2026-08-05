<script lang="ts">
  // M02-UX-04：忘记密码页。
  // - 无 JS：原生 form[method=POST] 提交（默认 action）；
  // - 有 JS：use:enhance 渐进增强；
  // - 提交后统一显示“如果该邮箱已注册…已发送”（后端 202 防枚举，不泄漏
  //   账号是否存在）；429 按 Retry-After 提示稍后再试。
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import type { PasswordResetRequestData } from './+page.server';

  let { form }: { form?: PasswordResetRequestData } = $props();

  const topMessage = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );
  const emailValue = $derived(form?.email ?? '');
</script>

<svelte:head>
  <title>忘记密码 — BBLBB</title>
</svelte:head>

<div class="auth-wrapper">
  <div class="auth-card">
    <div class="auth-header">
      <div class="auth-logo">BBLBB</div>
      <div class="auth-title">找回密码</div>
      <div class="auth-subtitle">输入注册邮箱，我们将发送重置链接</div>
    </div>
    <div class="auth-body">
      {#if form?.sent}
        <div class="empty-state">
          <div class="empty-state-title">重置链接已发送</div>
          <div class="empty-state-desc">
            如果该邮箱已注册，我们已向它发送密码重置链接（30 分钟内有效）。请查收邮件；若未收到，请检查垃圾邮件箱或稍后重试。
          </div>
          <div style="margin-top:var(--space-3);display:flex;gap:var(--space-2);justify-content:center;">
            <Button text="返回登录" variant="primary" size="sm" href="/login" />
          </div>
        </div>
      {:else}
        <form method="POST" use:enhance novalidate>
          {#if topMessage}
            <p class="input-hint is-error" role="alert">{topMessage}</p>
          {/if}
          <div class="input-wrapper">
            <label class="input-label" for="reset-email">邮箱</label>
            <input
              type="email"
              class="input-field"
              id="reset-email"
              name="email"
              placeholder="注册时使用的邮箱"
              value={emailValue}
              autocomplete="email"
            />
          </div>
          <Button text="发送重置链接" variant="primary" size="lg" type="submit" />
          <p class="auth-hint">若收不到邮件，请确认邮箱与注册时一致；每个账号每日最多请求 3 次。</p>
        </form>
      {/if}
    </div>
    <div class="auth-footer">
      想起来了？<a href="/login">返回登录</a> · <a href="/register">注册新账号</a>
    </div>
  </div>
</div>
