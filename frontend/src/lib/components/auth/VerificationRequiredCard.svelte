<script lang="ts">
  // GA 修复：邮箱验证门槛提示卡（Flarum 模式）。
  // - 未验证邮箱的登录用户不渲染发帖/回复编辑器（后端本就 403，前端不再让
  //   用户输入完才报错），改为展示本卡：说明 + 就地重发验证邮件 + 前往
  //   验证页链接。
  // - 重发走 POST /api/v1/auth/resend-verification（60s 冷却，429 按
  //   Retry-After 显示剩余秒数，与 CooldownButton 行为一致）。
  import Icon from '$lib/components/ui/Icon.svelte';
  import CooldownButton from '$lib/components/ui/CooldownButton.svelte';
  import { resendVerification } from '$lib/api/client';

  let {
    email = null
  }: { email?: string | null } = $props();

  let cooldown = $state(0);
  let attempt = $state(0);
  let sending = $state(false);
  let sent = $state(false);
  let errorMessage = $state<string | null>(null);

  async function resend() {
    if (!email) return;
    sending = true;
    errorMessage = null;
    try {
      await resendVerification(fetch, email);
      sent = true;
      cooldown = 60;
      attempt += 1;
    } catch (problem: any) {
      const retry = typeof problem?.retry_after === 'number' ? problem.retry_after : null;
      if (retry != null) {
        cooldown = retry;
        attempt += 1;
        errorMessage = `发送过于频繁，请 ${retry} 秒后再试`;
      } else {
        errorMessage = problem?.detail ?? '发送失败，请稍后重试';
      }
    } finally {
      sending = false;
    }
  }
</script>

<div class="card" class:verify-card-alert={true} style="margin-top:var(--space-4);">
  <div class="card-body verify-card-body">
    <div class="verify-card-icon" aria-hidden="true">
      <Icon name="mail" size={20} />
    </div>
    <div class="verify-card-main">
      <b>完成邮箱验证后即可发帖和回复</b>
      <p class="text-secondary verify-card-desc">
        {#if email}
          验证邮件已发送至 <b>{email}</b>，请点击邮件中的链接完成验证。
        {:else}
          请查收验证邮件，点击邮件中的链接完成验证。
        {/if}
        {#if sent}
          <span class="verify-card-sent" role="status">验证邮件已重新发送，请查收（30 分钟内有效）。</span>
        {/if}
        {#if errorMessage}
          <span class="verify-card-error" role="alert">{errorMessage}</span>
        {/if}
      </p>
      <div class="verify-card-actions">
        {#if email}
          <CooldownButton
            text={sending ? '发送中…' : '重新发送验证邮件'}
            {cooldown}
            {attempt}
            class="btn btn-primary btn-sm"
            onclick={resend}
            disabled={sending}
          />
        {/if}
        <a href="/verify-email" class="text-link">前往验证页</a>
      </div>
    </div>
  </div>
</div>

<style>
  .verify-card-body {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
  }

  .verify-card-alert {
    border-color: color-mix(in srgb, var(--color-brand) 35%, var(--color-border));
  }

  .verify-card-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    width: 38px;
    height: 38px;
    border-radius: 50%;
    background: color-mix(in srgb, var(--color-brand) 12%, transparent);
    color: var(--color-brand);
  }

  .verify-card-main {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
  }

  .verify-card-desc {
    margin: 0;
    font-size: var(--text-sm);
    line-height: 1.7;
  }

  .verify-card-sent {
    display: block;
    color: var(--color-success, var(--color-brand));
  }

  .verify-card-error {
    display: block;
    color: var(--color-danger);
  }

  .verify-card-actions {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    margin-top: var(--space-1);
  }
</style>
