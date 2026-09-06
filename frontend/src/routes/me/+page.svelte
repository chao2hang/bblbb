<script lang="ts">
  // M02-UX-05：/me 页——服务端安全投影（仅渲染自身账号可见字段，不输出
  // 任何会话 token）、账号状态/验证状态与 Session 设备管理。
  // - load 已取 user 与设备列表（+page.server.ts）；
  // - 设备列表：逐设备撤销（?/revoke，隐藏 session_id）与退出全部设备
  //   （?/logoutall）为原生 form[method=POST]（无 JS 可用，use:enhance
  //   渐进增强）；
  // - 当前设备按 last_seen_at 最大标记（后端每次请求滑动更新）；
  // - GAP-FIX 既有页面增强：账户卡（经验/B币/签到，GET /activity/summary
  //   失败时整卡隐藏）、快捷入口行（收藏/积分明细/我的帖子/私信/API 密钥/
  //   下载账单）、我的处罚区块（GET /me/sanctions，后端端点落地前恒空；
  //   有记录时显示类型/原因/时间 + 去申诉入口）。
  import { enhance } from '$app/forms';
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import { formatRelative } from '$lib/utils';
  import { activityLevelNumber, activityXp } from '$lib/api/types';
  import type { MeActionData, MePageData } from './+page.server';

  let { data, form }: { data: MePageData; form?: MeActionData } = $props();

  const user = $derived(data.user);
  const sessions = $derived(data.sessions);
  const currentId = $derived(data.currentSessionId);
  const error = $derived(data.error);
  // GAP-FIX 账户卡 / 我的处罚（load 增强数据；缺失时安全降级不渲染）。
  const activity = $derived(data.activity ?? null);
  const sanctions = $derived(data.sanctions ?? []);
  const coinBalance = $derived((activity?.balances ?? []).find((b) => b.currency === 'coin'));
  const topMessage = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );
  const mfaStep = $derived(form?.mfa);

  /** 快捷入口（M18-IA-01 对齐原型：账号安全/登录设备/通知设置/OAuth授权 + 业务入口）。 */
  const quickLinks = [
    { href: '/settings#settings-security', icon: 'shield', label: '账号安全' },
    { href: '#sessions', icon: 'smartphone', label: '登录设备' },
    { href: '/notifications', icon: 'bell', label: '通知设置' },
    { href: '/settings#settings-oauth', icon: 'key', label: 'OAuth 授权' },
    { href: '/favorites', icon: 'star', label: '我的收藏' },
    { href: '/me/balance', icon: 'coins', label: '积分明细' },
    { href: '/messages', icon: 'mail', label: '私信' },
    { href: '/apikeys', icon: 'key', label: 'API 密钥' },
    { href: '/me/billing', icon: 'download', label: '下载账单' }
  ] as const;

  /** 处罚类型中文标签（moderation SanctionKind；未知值原样展示）。 */
  const sanctionKindLabels: Record<string, string> = {
    warning: '警告',
    rate_limit: '限流',
    mute: '禁言',
    board_mute: '板块禁言',
    ban: '封禁',
    suspend: '暂停'
  };

  /** 后端时间戳为毫秒（M01-DB-08），formatRelative 口径为秒。 */
  function toSeconds(ts: number | null | undefined): number | null {
    if (typeof ts !== 'number' || !Number.isFinite(ts) || ts <= 0) return null;
    return ts > 1e11 ? Math.floor(ts / 1000) : ts;
  }

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
  <title>我的 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <!-- 原型对齐（prototype/pages/me.html）：app-route-head 仅 h1「我的」，无面包屑。 -->
  <div class="app-route-head">
    <div class="app-route-head__copy">
      <h1 tabindex="-1">我的</h1>
    </div>
  </div>

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
        <!-- GAP-FIX 账户卡：经验 / B币（GET /activity/summary；失败/缺失时
             整卡隐藏，不阻塞页面）。贡献统计暂无后端端点，不展示。 -->
        {#if activity}
          {@const lvlNum = activityLevelNumber(activity.level)}
          {@const lvlName =
            typeof activity.level === 'object' && activity.level && 'name' in activity.level
              ? (activity.level.name ?? null)
              : (activity.level_name ?? null)}
          {@const lvlSuffix = lvlName && lvlName !== `L${lvlNum}` ? ` · ${lvlName}` : ''}
          <div class="card">
            <div class="card-header">
              <span class="card-title">账户</span>
              {#if lvlNum !== null}<span class="badge badge-level">LV.{lvlNum}{lvlSuffix}</span>{/if}
            </div>
            <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
              <div style="display:flex;justify-content:space-between;align-items:baseline;">
                <span class="text-secondary" style="font-size:var(--text-sm);">经验</span>
                <strong style="font-variant-numeric:tabular-nums;">{activityXp(activity)}</strong>
              </div>
              {#if activity.xp_to_next !== null && activity.xp_to_next !== undefined}
                <p class="input-hint" style="margin:0;">距下一级还需 {activity.xp_to_next} 经验</p>
              {/if}
              <div style="display:flex;justify-content:space-between;align-items:baseline;">
                <span class="text-secondary" style="font-size:var(--text-sm);">B币余额</span>
                <strong style="font-variant-numeric:tabular-nums;">{coinBalance ? coinBalance.amount : '—'}</strong>
              </div>
              <div style="display:flex;justify-content:space-between;align-items:baseline;">
                <span class="text-secondary" style="font-size:var(--text-sm);">连续签到</span>
                <span style="font-variant-numeric:tabular-nums;">{activity.streak_days} 天{activity.checked_in_today ? '（今日已签）' : ''}</span>
              </div>
              <a class="btn btn-secondary btn-sm" href="/me/balance" style="text-align:center;">签到 / 积分明细</a>
            </div>
          </div>
        {/if}

        <div class="card">
          <div class="card-header"><span class="card-title">快捷操作</span></div>
          <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-2);">
            <Button text="发布新帖" variant="primary" size="sm" icon="pen-line" href="/editor" />
            <Button text="编辑资料" variant="secondary" size="sm" icon="edit-3" href="/settings" />
            <Button text="账号设置" variant="secondary" size="sm" icon="settings" href="/settings" />
          </div>
        </div>

        <!-- GAP-FIX 快捷入口行：收藏/积分明细/我的帖子/私信/API 密钥/下载账单。 -->
        <div class="card">
          <div class="card-header"><span class="card-title">快捷入口</span></div>
          <div class="card-body" style="display:grid;grid-template-columns:repeat(2, minmax(0, 1fr));gap:var(--space-2);">
            {#each quickLinks as link (link.href)}
              <a
                href={link.href}
                class="btn btn-ghost btn-sm"
                style="justify-content:flex-start;gap:var(--space-2);"
              >
                {link.label}
              </a>
            {/each}
            <a
              href="/users/{encodeURIComponent(user.username)}?tab=posts"
              class="btn btn-ghost btn-sm"
              style="justify-content:flex-start;gap:var(--space-2);"
            >
              我的帖子
            </a>
          </div>
        </div>
      </div>
    </div>

    {#if sanctions.length > 0}
      <!-- GAP-FIX 我的处罚：listMySanctions（load 取 GET /me/sanctions；后端
           端点落地前恒空，此卡不渲染）。 -->
      <div class="card" style="margin-top:var(--space-5);border-color:var(--color-warning);">
        <div class="card-header">
          <span class="card-title">我的处罚记录</span>
          <span class="badge badge-warning">{sanctions.length} 条</span>
        </div>
        <div class="card-body" style="padding:0;">
          <ul style="list-style:none;margin:0;padding:0;display:flex;flex-direction:column;">
            {#each sanctions as sanction (sanction.id)}
              <li style="padding:var(--space-3) var(--space-4);border-bottom:var(--border-default);display:flex;flex-wrap:wrap;gap:var(--space-2);align-items:center;">
                <span class="badge badge-warning">{sanctionKindLabels[sanction.kind] ?? sanction.kind}</span>
                <span style="flex:1;min-width:0;">{sanction.reason}</span>
                <span class="text-secondary" style="font-size:var(--text-xs);">
                  {formatRelative(toSeconds(sanction.created_at))}
                  {#if sanction.expires_at}
                    · 至 {formatRelative(toSeconds(sanction.expires_at))}
                  {:else}
                    · 未注明期限
                  {/if}
                </span>
                <a class="btn btn-secondary btn-sm" href="/moderation/appeals?create">去申诉</a>
              </li>
            {/each}
          </ul>
          <p class="input-hint" style="padding:var(--space-2) var(--space-4);margin:0;">
            对处罚有异议可提交申诉，由管理团队复核；申诉入口会要求处罚 ID（处罚通知中的 ID）。
          </p>
        </div>
      </div>
    {/if}

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

    <div class="card" style="margin-top:var(--space-5);">
      <div class="card-header"><span class="card-title">两步验证（MFA）</span></div>
      <div class="card-body">
        {#if topMessage}
          <p class="input-hint is-error" role="alert">{topMessage}</p>
        {/if}

        {#if mfaStep?.kind === 'enroll-challenge'}
          <div class="input-wrapper">
            <label class="input-label" for="mfa-secret">密钥</label>
            <input
              type="text"
              class="input-field"
              id="mfa-secret"
              value={mfaStep.secret_base32}
              readonly
            />
          </div>
          <p class="auth-hint">在身份验证器（如 Google Authenticator / 1Password）中添加账号，然后输入当前 6 位验证码完成启用。</p>
          <p class="auth-hint" style="word-break:break-all;">{mfaStep.otpauth_uri}</p>
          <form method="POST" action="?/mfa-confirm" use:enhance novalidate>
            <div class="input-wrapper">
              <label class="input-label" for="mfa-code">6 位验证码</label>
              <input
                type="text"
                class="input-field"
                id="mfa-code"
                name="code"
                placeholder="6 位验证码"
                inputmode="numeric"
                pattern="[0-9]{6}"
                maxlength="6"
                autocomplete="one-time-code"
              />
            </div>
            <Button text="完成启用" variant="primary" size="sm" type="submit" />
          </form>
          <form method="POST" action="?/mfa-cancel" use:enhance style="margin-top:var(--space-2);">
            <Button text="取消" variant="ghost" size="sm" type="submit" />
          </form>
        {:else if mfaStep?.kind === 'enroll-confirmed'}
          <p class="input-hint" role="status">两步验证已启用。建议立即生成恢复码并妥善保存（只显示一次）。</p>
          <form method="POST" action="?/mfa-recovery" use:enhance>
            <Button text="生成恢复码" variant="primary" size="sm" type="submit" />
          </form>
        {:else if mfaStep?.kind === 'recovery-codes'}
          <p class="input-hint" role="status">以下恢复码<b>只显示这一次</b>，请立即抄写或保存到安全的地方；遗失后只能通过重新生成恢复。</p>
          <ul style="list-style:none;margin:var(--space-2) 0;padding:0;display:grid;grid-template-columns:repeat(2, minmax(0, 1fr));gap:var(--space-2);">
            {#each mfaStep.codes as code}
              <li style="font-family:monospace;padding:var(--space-2);border:1px solid var(--color-border);border-radius:var(--radius-sm);text-align:center;">{code}</li>
            {/each}
          </ul>
          <div style="display:flex;gap:var(--space-2);align-items:center;">
            <a class="btn btn-primary btn-sm" href="/me">我已保存</a>
          </div>
        {:else if mfaStep?.kind === 'disabled'}
          <p class="input-hint" role="status">两步验证已停用，账号恢复仅凭密码登录。</p>
          <form method="POST" action="?/mfa-enroll" use:enhance>
            <Button text="重新启用两步验证" variant="primary" size="sm" type="submit" />
          </form>
        {:else if mfaStep?.kind === 'step-up'}
          <p class="auth-hint">出于安全考虑，此操作需要重新输入密码确认身份（近期已认证则可直接执行）。</p>
          <form method="POST" action="?/re-auth" use:enhance novalidate>
            <input type="hidden" name="intent" value={mfaStep.intent} />
            <div class="input-wrapper">
              <label class="input-label" for="reauth-password">密码</label>
              <input
                type="password"
                class="input-field"
                id="reauth-password"
                name="password"
                placeholder="输入当前密码"
                autocomplete="current-password"
              />
            </div>
            <Button text="验证身份" variant="primary" size="sm" type="submit" />
          </form>
        {:else if mfaStep?.kind === 'reauth-done'}
          <p class="input-hint" role="status">身份已验证，请再次点击原操作完成。</p>
          {#if mfaStep.intent === 'disable'}
            <form method="POST" action="?/mfa-disable" use:enhance>
              <Button text="停用两步验证" variant="danger" size="sm" type="submit" />
            </form>
          {:else}
            <form method="POST" action="?/mfa-recovery" use:enhance>
              <Button text="生成恢复码" variant="primary" size="sm" type="submit" />
            </form>
          {/if}
        {:else}
          {#if user.mfa_enabled}
            <p><span class="badge badge-success">已启用</span><span class="text-secondary" style="font-size:var(--text-sm);margin-left:var(--space-2);">登录时需要输入身份验证器验证码</span></p>
            <div style="display:flex;gap:var(--space-2);align-items:center;margin-top:var(--space-2);">
              <form method="POST" action="?/mfa-recovery" use:enhance>
                <Button text="生成新恢复码" variant="secondary" size="sm" type="submit" />
              </form>
              <form method="POST" action="?/mfa-disable" use:enhance>
                <Button text="停用两步验证" variant="danger" size="sm" type="submit" />
              </form>
            </div>
          {:else}
            <p><span class="badge badge-neutral">未启用</span><span class="text-secondary" style="font-size:var(--text-sm);margin-left:var(--space-2);">开启后登录时需要输入身份验证器验证码，安全性更高</span></p>
            <div style="margin-top:var(--space-2);">
              <form method="POST" action="?/mfa-enroll" use:enhance>
                <Button text="启用两步验证" variant="primary" size="sm" type="submit" />
              </form>
            </div>
          {/if}
        {/if}
      </div>
    </div>
  {:else if !error}
    <div class="empty-state"><div class="empty-state-title">加载中…</div></div>
  {/if}
</div>
