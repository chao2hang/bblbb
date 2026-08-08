<script lang="ts">
  // M02-UX-03：登录页（服务端表单 action + 两步登录 MFA）。
  // - 无 JS：原生 form[method=POST] 提交到 +page.server.ts action；
  // - 有 JS：use:enhance 渐进增强（同一 action，无双重实现）；
  // - 启用 TOTP 的账号：第一步返回 challenge，页面切换第二步输入
  //   6 位验证码（或切换到恢复码）；
  // - 失败提示统一（后端 401 不泄漏账号是否存在/密码是否正确）。
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import type { LoginActionData } from './+page.server';

  let { form }: { form?: LoginActionData } = $props();

  const mfaStep = $derived(form?.mfa_required === true);
  let useRecovery = $state(false);
  const topMessage = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );

  function toggleRecovery() {
    useRecovery = !useRecovery;
  }
</script>

<svelte:head>
  <title>登录 — BBLBB</title>
</svelte:head>

<div class="auth-wrapper">
  <div class="auth-card">
    <div class="auth-header">
      <div class="auth-logo">BBLBB</div>
      <div class="auth-title">欢迎回来</div>
      <div class="auth-subtitle">登录你的账号继续探索</div>
    </div>
    <div class="auth-body">
      {#if mfaStep}
        <form method="POST" action="?/mfa" use:enhance novalidate>
          {#if topMessage}
            <p class="input-hint is-error" role="alert">{topMessage}</p>
          {/if}
          <input type="hidden" name="challenge_token" value={form?.challenge_token ?? ''} />
          <p class="auth-hint">该账号启用了两步验证。请输入身份验证器中的 6 位验证码。</p>
          {#if useRecovery}
            <div class="input-wrapper">
              <label class="input-label" for="login-recovery">恢复码</label>
              <input
                type="text"
                class="input-field"
                id="login-recovery"
                name="recovery_code"
                placeholder="16 位恢复码"
                autocomplete="one-time-code"
              />
            </div>
            <Button text="用验证码登录" variant="ghost" size="sm" type="button" onclick={toggleRecovery} />
          {:else}
            <div class="input-wrapper">
              <label class="input-label" for="login-totp">验证码</label>
              <input
                type="text"
                class="input-field"
                id="login-totp"
                name="totp_code"
                placeholder="6 位验证码"
                inputmode="numeric"
                pattern="[0-9]{6}"
                maxlength="6"
                autocomplete="one-time-code"
              />
            </div>
            <Button text="使用恢复码" variant="ghost" size="sm" type="button" onclick={toggleRecovery} />
          {/if}
          <Button text="验证并登录" variant="primary" size="lg" type="submit" />
          <p class="auth-hint">验证码有误？请重试；验证码每 30 秒更新。</p>
        </form>
      {:else}
        <!-- M14-A11Y-08 修复：显式 action="?/login" —— 页面无 default action，
             缺省 POST（无 JS 退化）会 404；指定命名 action 后无 JS 表单可提交。 -->
        <form method="POST" action="?/login" use:enhance novalidate>
          {#if topMessage}
            <p class="input-hint is-error" role="alert">{topMessage}</p>
          {/if}
          <div class="input-wrapper">
            <label class="input-label" for="login-identifier">用户名或邮箱</label>
            <input
              type="text"
              class="input-field"
              id="login-identifier"
              name="identifier"
              placeholder="用户名或邮箱"
              autocomplete="username"
            />
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="login-password">密码</label>
            <input
              type="password"
              class="input-field"
              id="login-password"
              name="password"
              placeholder="输入密码"
              autocomplete="current-password"
            />
          </div>
          <div class="login-options" style="display:flex;align-items:center;justify-content:space-between;gap:var(--space-2);margin:var(--space-1) 0 var(--space-3);">
            <label class="input-label" style="display:flex;align-items:center;gap:6px;cursor:pointer;margin:0;font-size:var(--text-sm);">
              <input type="checkbox" name="remember" value="on" checked style="accent-color:var(--color-primary);" />
              记住我
            </label>
            <a href="/password-reset" class="text-link" style="font-size:var(--text-sm);">忘记密码？</a>
          </div>
          <Button text="登录" variant="primary" size="lg" type="submit" />
        </form>
      {/if}
    </div>
    <div class="auth-footer">
      还没有账号？<a href="/register">立即注册</a>
    </div>
  </div>
</div>
