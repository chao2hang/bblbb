<script lang="ts">
  // M02-UX-05：/me 页——服务端安全投影（仅渲染自身账号可见字段，不输出
  // 任何会话 token）、账号状态/验证状态与 Session 设备管理。
  // - load 已取 user 与设备列表（+page.server.ts）；
  // - 设备列表：逐设备撤销（?/revoke，隐藏 session_id）与退出全部设备
  //   （?/logoutall）为原生 form[method=POST]（无 JS 可用，use:enhance
  //   渐进增强）；
  // - 当前设备按 last_seen_at 最大标记（后端每次请求滑动更新）。
  import { enhance } from '$app/forms';
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import type { MeActionData, MePageData } from './+page.server';

  let { data, form }: { data: MePageData; form?: MeActionData } = $props();

  const user = $derived(data.user);
  const sessions = $derived(data.sessions);
  const currentId = $derived(data.currentSessionId);
  const error = $derived(data.error);
  const topMessage = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );

  const statusLabel: Record<string, string> = {
    active: '正常',
    pending: '待验证',
    restricted: '受限',
    banned: '已封禁',
    deleted: '已删除'
  };

  function statusBadge(status: string): string {
    switch (status) {
      case 'active':
        return 'badge-success';
      case 'pending':
        return 'badge-warning';
      case 'restricted':
        return 'badge-warning';
      case 'banned':
      case 'deleted':
        return 'badge-danger';
      default:
        return 'badge-neutral';
    }
  }

  function roleLabel(role: string): string {
    const map: Record<string, string> = { admin: '管理员', mod: '版主', member: '成员' };
    return map[role] ?? role;
  }

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

<svelte:head>
  <title>我的主页 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">我的主页</span>
  </nav>

  {#if error}
    <p class="input-hint is-error" role="alert">{error}</p>
  {/if}

  {#if user}
    <div class="card profile-page-card">
      <div class="profile-cover" role="img" aria-label="个人资料背景"></div>
      <div class="profile-header">
        <div class="profile-avatar">
          <Avatar name={user.display_name || user.username} size="xl" />
        </div>
        <div class="profile-info">
          <div class="profile-name">
            {user.display_name || user.username}
            <span class="badge badge-level">LV.{user.level}</span>
          </div>
          <p class="profile-bio">@ {user.username}</p>
        </div>
        <div class="profile-actions">
          <Button text="编辑资料" variant="secondary" size="sm" icon="edit-3" href="/settings" />
        </div>
      </div>
    </div>

    <div class="content-grid" style="margin-top:var(--space-5);">
      <div class="main-col">
        <div class="card">
          <div class="card-header"><span class="card-title">账号信息</span></div>
          <div class="card-body">
            <dl class="profile-about-list">
              <div class="profile-about-item"><dt>用户名</dt><dd>{user.username}</dd></div>
              <div class="profile-about-item"><dt>邮箱</dt><dd>{user.email}</dd></div>
              <div class="profile-about-item">
                <dt>邮箱验证</dt>
                <dd>
                  {#if user.email_verified}
                    <span class="badge badge-success">已验证</span>
                  {:else}
                    <span class="badge badge-warning">未验证</span>
                    <a href="/verify-email" style="margin-left:var(--space-2);">去验证</a>
                  {/if}
                </dd>
              </div>
              <div class="profile-about-item">
                <dt>账号状态</dt>
                <dd><span class="badge {statusBadge(user.status)}">{statusLabel[user.status] ?? user.status}</span></dd>
              </div>
              <div class="profile-about-item">
                <dt>角色</dt>
                <dd>
                  {#if user.roles.length > 0}
                    <span class="badge badge-role-admin">{roleLabel(user.roles[0])}</span>
                  {:else}
                    <span class="badge badge-neutral">成员</span>
                  {/if}
                </dd>
              </div>
            </dl>
          </div>
        </div>
      </div>
      <div class="side-col">
        <div class="card">
          <div class="card-header"><span class="card-title">快捷操作</span></div>
          <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-2);">
            <Button text="发布新帖" variant="primary" size="sm" icon="pen-line" href="/editor" />
            <Button text="账号设置" variant="secondary" size="sm" icon="settings" href="/settings" />
          </div>
        </div>
      </div>
    </div>

    <div class="card" style="margin-top:var(--space-5);">
      <div class="card-header">
        <span class="card-title">登录设备管理</span>
        <span class="text-secondary" style="font-size:var(--text-sm);">共 {sessions.length} 台设备</span>
      </div>
      <div class="card-body">
        {#if topMessage}
          <p class="input-hint is-error" role="alert">{topMessage}</p>
        {/if}
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
