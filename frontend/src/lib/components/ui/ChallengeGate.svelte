<script lang="ts">
  // M08-UI-06：挑战/限流门禁——键盘、屏幕阅读器、移动端与失败回退。
  //
  // 后端 M08-CRAWL-06 挑战流程（一次性 token、过期、失败次数、无障碍替代
  // 路径）尚未启用时不会触发；本组件对未来 `challenge_required` 与 429 限流
  // 提供统一、可访问的展示：
  //  - 挑战入口必须是普通链接（无 JS 可点击）；
  //  - 文案在 role=alert 区域播报（屏幕阅读器）；
  //  - 按钮支持键盘（原生 <button>）与触屏；
  //  - 失败/取消后永远保留「重新搜索」「返回首页」等安全回退入口。
  import Button from './Button.svelte';

  let {
    challengeUrl = null,
    retryAfterSecs = null,
    onRetry = null,
    title = '访问频率过高',
    message = '系统检测到异常频繁的访问。请完成验证后继续，或稍后再试。'
  }: {
    challengeUrl?: string | null;
    retryAfterSecs?: number | null;
    onRetry?: (() => void) | null;
    title?: string;
    message?: string;
  } = $props();

  /** 屏显倒计时（每秒刷新；失败回退仍可用）。 */
  let countdown = $state(0);
  $effect(() => {
    if (!(retryAfterSecs && retryAfterSecs > 0)) {
      countdown = 0;
      return;
    }
    countdown = retryAfterSecs;
    const timer = setInterval(() => {
      countdown = Math.max(0, countdown - 1);
      if (countdown === 0) clearInterval(timer);
    }, 1000);
    return () => clearInterval(timer);
  });

  function retryNow() {
    if (onRetry) onRetry();
  }
</script>

<div class="card" role="alert" aria-live="assertive" style="border-color:var(--color-warning);margin:var(--space-4) 0;">
  <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
    <div style="display:flex;gap:var(--space-2);align-items:center;">
      <span class="badge badge-warning">验证</span>
      <strong>{title}</strong>
    </div>
    <p style="margin:0;">{message}</p>

    <div style="display:flex;flex-wrap:wrap;gap:var(--space-2);">
      {#if challengeUrl}
        <!-- 挑战入口：普通链接，无 JS 可用；键盘/读屏/触屏同原生语义。 -->
        <a class="btn btn-primary btn-sm" href={challengeUrl} rel="nofollow">完成验证</a>
      {/if}
      {#if onRetry}
        <Button text="重新搜索" variant="secondary" size="sm" onclick={retryNow} />
      {/if}
      {#if retryAfterSecs && retryAfterSecs > 0}
        <span class="text-secondary" style="font-size:var(--text-sm);align-self:center;" role="status">
          {countdown > 0 ? `可在约 ${countdown} 秒后重试` : '可重试'}
        </span>
      {/if}
    </div>

    <p class="input-hint" style="margin:0;">
      验证不会解除服务端的内容授权边界；若仍无法访问，可稍后再试或
      <a href="/search" class="text-link">返回搜索首页</a>。
    </p>
  </div>
</div>
