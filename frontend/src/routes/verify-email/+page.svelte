<script lang="ts">
  // M02-UX-02：邮箱验证结果页。
  // - 带 token 进入：提交“完成邮箱验证”（?/verify），显示结果；
  // - 重发入口（?/resend）：冷却 60s 倒计时（CooldownButton，服务器 429
  //   时按 Retry-After 显示剩余）；
  // - 未验证账号允许/禁止动作说明（REQUIREMENTS.md：只能浏览/改账号/重发，
  //   不能发帖/回复/上传/交易/领取奖励）。
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import CooldownButton from '$lib/components/ui/CooldownButton.svelte';
  import type { VerifyEmailActionData } from './+page.server';

  let {
    data,
    form
  }: { data: { token: string | null }; form?: VerifyEmailActionData } = $props();

  // 每次 resend 结果变化都递增，强制 CooldownButton 重启计时（含同秒数）
  let resendAttempt = $state(0);
  $effect(() => {
    if (form?.cooldown != null) resendAttempt += 1;
  });

  const topMessage = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );
  const emailValue = $derived(form?.email ?? '');
</script>

<svelte:head>
  <title>邮箱验证 — BBLBB</title>
</svelte:head>

<div class="auth-wrapper">
  <div class="auth-card">
    <div class="auth-header">
      <div class="auth-logo">BBLBB</div>
      <div class="auth-title">邮箱验证</div>
      <div class="auth-subtitle">完成验证即可解锁完整社区功能</div>
    </div>
    <div class="auth-body">
      {#if form?.ok}
        <div class="empty-state">
          <div class="empty-state-title">验证成功</div>
          <div class="empty-state-desc">你的邮箱已验证，现在可以发帖、回复和参与社区活动了。</div>
          <div style="margin-top:var(--space-3);display:flex;gap:var(--space-2);justify-content:center;">
            <Button text="前往首页" variant="primary" size="sm" href="/" />
          </div>
        </div>
      {:else}
        {#if data.token}
          <form method="POST" action="?/verify" use:enhance novalidate>
            {#if topMessage}
              <p class="input-hint is-error" role="alert">{topMessage}</p>
            {/if}
            <input type="hidden" name="token" value={data.token} />
            <div class="input-wrapper">
              <p class="auth-hint">点击下方按钮完成邮箱验证。链接一次有效，30 分钟后过期。</p>
            </div>
            <Button text="完成邮箱验证" variant="primary" size="lg" type="submit" />
          </form>
        {:else}
          <p class="auth-hint" role="status">请从验证邮件中的完整链接进入本页，或在下方重新发送验证邮件。</p>
        {/if}

        <div class="auth-divider">或</div>

        <form method="POST" action="?/resend" use:enhance novalidate>
          {#if topMessage && !data.token}
            <p class="input-hint is-error" role="alert">{topMessage}</p>
          {/if}
          {#if form?.sent}
            <p class="input-hint" role="status">验证邮件已发送，请查收（若邮箱不存在或已激活则不发送，提示保持一致）。</p>
          {/if}
          <div class="input-wrapper">
            <label class="input-label" for="resend-email">邮箱</label>
            <input
              type="email"
              class="input-field"
              id="resend-email"
              name="email"
              placeholder="注册时使用的邮箱"
              value={emailValue}
              autocomplete="email"
            />
          </div>
          <CooldownButton
            text="重新发送验证邮件"
            cooldown={form?.cooldown ?? 0}
            attempt={resendAttempt}
            class="btn btn-primary btn-lg"
          />
        </form>

        <div class="auth-divider">未验证账号说明</div>
        <div class="auth-unverified">
          <p>未验证邮箱的账号<b>可以</b>：浏览内容、登录、修改账号资料、重发验证邮件。</p>
          <p>未验证邮箱的账号<b>不能</b>：发帖、回复、上传附件、参与交易或领取活动奖励。</p>
          <p>验证后立即可用；站点开启新用户冷静期时，验证后仍受冷静期限制。</p>
        </div>
      {/if}
    </div>
    <div class="auth-footer">
      还没有账号？<a href="/register">立即注册</a> · <a href="/login">登录</a>
    </div>
  </div>
</div>
