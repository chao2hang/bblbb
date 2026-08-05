<script lang="ts">
  // M02-UX-04：重置密码页。
  // - 无 JS：原生 form[method=POST] 提交（隐藏 token）；
  // - 有 JS：use:enhance 渐进增强；
  // - 成功面板明确提示“其他设备上的会话已全部撤销”（后端改密时撤销全部
  //   Session，M02-IDENTITY-10）；无效/已消费/过期统一 400 提示重新发起；
  // - 密码字段错误经 fieldErrors 与输入框 aria-describedby 关联。
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import type { PasswordResetConfirmData } from './+page.server';

  let {
    data,
    form
  }: { data: { token: string | null }; form?: PasswordResetConfirmData } = $props();

  const passwordError = $derived(form?.fieldErrors?.password ?? null);
  const confirmError = $derived(form?.fieldErrors?.confirm ?? null);
  const topMessage = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );
</script>

<svelte:head>
  <title>重置密码 — BBLBB</title>
</svelte:head>

<div class="auth-wrapper">
  <div class="auth-card">
    <div class="auth-header">
      <div class="auth-logo">BBLBB</div>
      <div class="auth-title">设置新密码</div>
      <div class="auth-subtitle">链接一次有效，30 分钟后过期</div>
    </div>
    <div class="auth-body">
      {#if form?.ok}
        <div class="empty-state">
          <div class="empty-state-title">密码已重置</div>
          <div class="empty-state-desc">
            为保护你的账号安全，其他设备上的会话已全部撤销，请使用新密码重新登录。
          </div>
          <div style="margin-top:var(--space-3);display:flex;gap:var(--space-2);justify-content:center;">
            <Button text="前往登录" variant="primary" size="sm" href="/login" />
          </div>
        </div>
      {:else if data.token}
        <form method="POST" use:enhance novalidate>
          {#if topMessage}
            <p class="input-hint is-error" role="alert">{topMessage}</p>
          {/if}
          <input type="hidden" name="token" value={data.token} />
          <div class="input-wrapper">
            <label class="input-label" for="reset-password">新密码</label>
            <input
              type="password"
              class="input-field"
              id="reset-password"
              name="password"
              placeholder="8-128 个字符，含字母和数字"
              autocomplete="new-password"
              aria-invalid={passwordError ? 'true' : undefined}
              aria-describedby={passwordError ? 'reset-password-error' : undefined}
            />
            {#if passwordError}
              <p class="input-hint is-error" id="reset-password-error" role="alert">{passwordError}</p>
            {/if}
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="reset-confirm">确认新密码</label>
            <input
              type="password"
              class="input-field"
              id="reset-confirm"
              name="confirm"
              placeholder="再次输入新密码"
              autocomplete="new-password"
              aria-invalid={confirmError ? 'true' : undefined}
              aria-describedby={confirmError ? 'reset-confirm-error' : undefined}
            />
            {#if confirmError}
              <p class="input-hint is-error" id="reset-confirm-error" role="alert">{confirmError}</p>
            {/if}
          </div>
          <Button text="重置密码" variant="primary" size="lg" type="submit" />
        </form>
      {:else}
        <p class="auth-hint" role="status">请从重置邮件中的完整链接进入本页（链接含一次性 token）。</p>
        <div class="auth-divider">或</div>
        <div style="text-align:center;">
          <Button text="重新发起找回密码" variant="ghost" size="sm" href="/password-reset" />
        </div>
      {/if}
    </div>
    <div class="auth-footer">
      想起来了？<a href="/login">返回登录</a>
    </div>
  </div>
</div>
