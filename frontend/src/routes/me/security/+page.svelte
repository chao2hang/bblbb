<script lang="ts">
  // M02-UX-SEC：账号与安全页（自 /me 拆分）。
  // - 安全清单：登录密码 / 两步验证（TOTP/Passkey → /mfa）/ OAuth 授权 /
  //   登录设备，每行展示真实状态（徽标/计数）与唯一管理入口；
  // - 登录设备管理：逐设备撤销（?/revoke，隐藏 session_id）与退出全部设备
  //   （?/logoutall）为原生 form[method=POST]（无 JS 可用，use:enhance
  //   渐进增强）；当前设备按 last_seen_at 最大标记（后端每次请求滑动更新）。
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import type { SecurityActionData, SecurityPageData } from './+page.server';
  import PageTitle from '$lib/components/PageTitle.svelte';
  import SettingsNav from '$lib/components/SettingsNav.svelte';

  let { data, form }: { data: SecurityPageData; form?: SecurityActionData } = $props();

  const user = $derived(data.user);
  const sessions = $derived(data.sessions);
  const currentId = $derived(data.currentSessionId);
  const error = $derived(data.error);
  const topMessage = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );

  /** 从 User-Agent 派生设备简称（仅展示，不解析敏感信息）。 */
  function deviceLabel(ua: string | null): string {
    if (!ua) return '未知设备';
    const s = ua.toLowerCase();
    if (s.includes('iphone') || s.includes('android')) return '手机';
    if (s.includes('ipad')) return '平板';
    if (s.includes('mac')) return 'Mac';
    if (s.includes('windows')) return 'Windows';
    if (s.includes('linux')) return 'Linux';
    return '浏览器';
  }

  function formatTs(ms: number): string {
    const d = new Date(ms);
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  }
</script>

<PageTitle title="账号与安全" />

<div class="container page-content app-page app-settings-page" id="page-me-security">
  <h1 class="u-visually-hidden">账号与安全</h1>

  <div class="app-settings-layout">
    <SettingsNav active="devices" />

    <div class="settings-content">
      <nav class="sec-back" aria-label="页面路径">
        <a href="/me"><Icon name="chevron-left" size={14} />返回个人主页</a>
      </nav>

      {#if error}
        <div class="app-notice is-danger" role="alert">
          <span>{error}</span>
          <a href="/me/security">重新加载</a>
        </div>
      {/if}

      {#if topMessage}
        <p class="input-hint is-error" role="alert">{topMessage}</p>
      {/if}

      {#if user}
        <!-- 安全清单：每一行的状态都来自后端真实数据，管理动作只指向唯一入口 -->
        <section class="card" aria-label="安全状态">
      <div class="card-header">
        <span class="card-title">安全状态</span>
        <span class="text-secondary" style="font-size:var(--text-sm);">访问与身份验证</span>
      </div>
      <div class="card-body" style="padding:0;">
        <ul class="sec-list" role="list">
          <li class="sec-row">
            <span class="sec-row__icon"><Icon name="lock" size={16} /></span>
            <div class="sec-row__meta">
              <span class="sec-row__title">登录密码</span>
              <span class="sec-row__desc">定期更换密码，避免在其他网站重复使用。</span>
            </div>
            <a class="sec-row__action" href="/settings#settings-security">修改密码<Icon name="chevron-right" size={14} /></a>
          </li>
          <li class="sec-row">
            <span class="sec-row__icon"><Icon name="shield" size={16} /></span>
            <div class="sec-row__meta">
              <span class="sec-row__title">
                两步验证
                <span class="badge {user.mfa_enabled ? 'badge-success' : 'badge-neutral'}">
                  {user.mfa_enabled ? '已启用' : '未启用'}
                </span>
              </span>
              <span class="sec-row__desc">
                {#if user.mfa_enabled}
                  已开启，登录需要输入认证器动态验证码或 Passkey。
                {:else}
                  开启后登录需要额外输入动态验证码，安全性更高。
                {/if}
              </span>
            </div>
            <a class="sec-row__action" href="/mfa">管理两步验证<Icon name="chevron-right" size={14} /></a>
          </li>
          <li class="sec-row">
            <span class="sec-row__icon"><Icon name="key" size={16} /></span>
            <div class="sec-row__meta">
              <span class="sec-row__title">OAuth 授权</span>
              <span class="sec-row__desc">查看并撤回已授权给第三方应用的账号访问。</span>
            </div>
            <a class="sec-row__action" href="/settings#settings-oauth">管理授权<Icon name="chevron-right" size={14} /></a>
          </li>
          <li class="sec-row">
            <span class="sec-row__icon"><Icon name="smartphone" size={16} /></span>
            <div class="sec-row__meta">
              <span class="sec-row__title">登录设备</span>
              <span class="sec-row__desc">当前共 {sessions.length} 台设备在线，可在下方列表逐台撤销。</span>
            </div>
            <a class="sec-row__action" href="#sessions">管理设备<Icon name="chevron-right" size={14} /></a>
          </li>
        </ul>
      </div>
    </section>

    <!-- 登录设备管理（自 /me 平移，M02-UX-05） -->
    <div class="card" id="sessions" style="margin-top:var(--space-4);">
      <div class="card-header">
        <span class="card-title">登录设备管理</span>
        <span class="text-secondary" style="font-size:var(--text-sm);">共 {sessions.length} 台设备</span>
      </div>
      <div class="card-body">
        {#if sessions.length === 0}
          <p class="auth-hint">暂无登录设备。</p>
        {:else}
          <ul class="session-list" style="list-style:none;margin:0;padding:0;display:flex;flex-direction:column;gap:var(--space-2);">
            {#each sessions as session (session.id)}
              <li class="session-item" style="display:flex;align-items:center;justify-content:space-between;gap:var(--space-3);padding:var(--space-3);border:1px solid var(--color-border);border-radius:var(--radius-md);">
                <div style="min-width:0;">
                  <div style="display:flex;align-items:center;gap:var(--space-2);">
                    <span class="badge badge-neutral">{deviceLabel(session.user_agent)}</span>
                    {#if session.id === currentId}
                      <span class="badge badge-success">当前设备</span>
                    {/if}
                  </div>
                  {#if session.user_agent}
                    <p class="text-secondary" style="font-size:var(--text-sm);margin:var(--space-1) 0 0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:420px;">{session.user_agent}</p>
                  {/if}
                  <p class="text-secondary" style="font-size:var(--text-xs);margin:var(--space-1) 0 0;">
                    最近活跃 {formatTs(session.last_seen_at)} · 登录于 {formatTs(session.created_at)} · 过期于 {formatTs(session.absolute_expires_at)}
                  </p>
                </div>
                <div style="flex-shrink:0;">
                  {#if session.id === currentId}
                    <span class="text-secondary" style="font-size:var(--text-sm);">当前设备不可撤销</span>
                  {:else}
                    <form method="POST" action="?/revoke" use:enhance>
                      <input type="hidden" name="session_id" value={session.id} />
                      <Button text="撤销" variant="ghost" size="sm" type="submit" />
                    </form>
                  {/if}
                </div>
              </li>
            {/each}
          </ul>
        {/if}
        <div style="margin-top:var(--space-3);display:flex;justify-content:flex-end;">
          <form method="POST" action="?/logoutall" use:enhance>
            <Button text="退出全部设备" variant="danger" size="sm" type="submit" />
          </form>
        </div>
        <p class="auth-hint" style="margin-top:var(--space-2);">
          撤销设备后，该设备上的登录将立即失效；退出全部设备会把当前设备也一并退出。
        </p>
      </div>
    </div>
  {:else if !error}
    <div class="empty-state"><div class="empty-state-title">加载中…</div></div>
  {/if}
    </div>
  </div>
</div>

<style>
  /* 返回个人主页的路径入口 */
  .sec-back {
    margin-top: 0;
    margin-bottom: var(--space-2);
  }
  .sec-back a {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    text-decoration: none;
  }
  .sec-back a:hover {
    color: var(--color-brand);
  }
  /* 安全清单：状态行（图标 / 名称与说明 / 状态徽标 / 管理入口） */
  .sec-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
  }
  .sec-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-bottom: var(--border-default);
  }
  .sec-row:last-child {
    border-bottom: none;
  }
  .sec-row__icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    flex-shrink: 0;
    border-radius: var(--radius-md);
    background: var(--color-bg-subtle);
    color: var(--color-text-secondary);
  }
  .sec-row__meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .sec-row__title {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
    font-weight: var(--weight-semibold, 600);
    font-size: var(--text-sm);
    color: var(--color-text-primary);
  }
  .sec-row__desc {
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
  }
  .sec-row__action {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
    font-size: var(--text-sm);
    color: var(--color-brand);
    text-decoration: none;
  }
  .sec-row__action:hover {
    text-decoration: underline;
  }
  @media (max-width: 767px) {
    .sec-row {
      flex-wrap: wrap;
    }
    .sec-row__action {
      margin-left: calc(32px + var(--space-3));
    }
  }
  /* 锚点滚动定位时不被顶栏遮挡 */
  #sessions {
    scroll-margin-top: var(--space-6, 24px);
  }
</style>
