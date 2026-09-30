<script lang="ts">
  // GA 邮箱换绑确认页：读 ?token= → ?/confirm 代理后端一次性消费。
  // 成功 → 新邮箱成为绑定邮箱且视为已验证；失败给明确下一步（回设置页重发）。
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import { resolveSiteCopy, pageTitle, type SiteCopyView } from '$lib/site/copy';
  import type { EmailChangeConfirmActionData } from './+page.server';

  let {
    data,
    form
  }: { data: { token: string | null; site?: SiteCopyView | null }; form?: EmailChangeConfirmActionData } = $props();

  const site = $derived<SiteCopyView>(data.site ?? resolveSiteCopy(null));
</script>

<svelte:head>
  <title>{pageTitle('邮箱换绑', site.siteName)}</title>
</svelte:head>

<div class="auth-wrapper">
  <div class="auth-card">
    <div class="auth-header">
      <div class="auth-logo">{site.siteName}</div>
      <div class="auth-title">邮箱换绑确认</div>
      <div class="auth-subtitle">确认后新邮箱将成为你的绑定邮箱并自动完成验证</div>
    </div>
    <div class="auth-body">
      {#if form?.ok}
        <div class="empty-state">
          <div class="empty-state-title">换绑成功</div>
          <div class="empty-state-desc">{form.message ?? '新邮箱已完成验证，现在是你的登录邮箱。'}</div>
          <div style="margin-top:var(--space-3);display:flex;gap:var(--space-2);justify-content:center;">
            <Button text="前往设置页" variant="primary" size="sm" href="/settings" />
          </div>
        </div>
      {:else}
        {#if data.token}
          <form method="POST" action="?/confirm" use:enhance novalidate>
            {#if form?.message}
              <p class="input-hint is-error" role="alert">{form.message}</p>
            {/if}
            <input type="hidden" name="token" value={data.token} />
            <div class="input-wrapper">
              <p class="auth-hint">点击下方按钮确认换绑。链接一次有效，30 分钟后过期。</p>
            </div>
            <Button text="确认换绑邮箱" variant="primary" size="lg" type="submit" />
          </form>
        {:else}
          <p class="auth-hint" role="status">请从换绑确认邮件中的完整链接进入本页；链接已失效时可在设置页重新申请换绑。</p>
        {/if}
        <div class="auth-divider">或</div>
        <div class="auth-unverified">
          <p>链接无效或已过期？前往 <a href="/settings">账号设置 → 绑定邮箱</a> 重新申请换绑。</p>
        </div>
      {/if}
    </div>
    <div class="auth-footer">
      <a href="/settings">返回设置</a> · <a href="/">回首页</a>
    </div>
  </div>
</div>
