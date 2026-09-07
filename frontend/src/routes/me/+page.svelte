<script lang="ts">
  // M02-UX-05：/me 页——服务端安全投影（仅渲染自身账号可见字段，不输出
  // 任何会话 token）、账号状态/验证状态与 Session 设备管理。
  // - load 已取 user 与设备列表（+page.server.ts）；
  // - 设备列表：逐设备撤销（?/revoke，隐藏 session_id）与退出全部设备
  //   （?/logoutall）为原生 form[method=POST]（无 JS 可用，use:enhance
  //   渐进增强）；
  // - 当前设备按 last_seen_at 最大标记（后端每次请求滑动更新）；
  // - GAP-FIX 既有页面增强：账户卡（经验/B币/签到，GET /activity/summary
  //   失败时整卡隐藏）、我的处罚区块（GET /me/sanctions，后端端点落地前恒空；
  //   有记录时显示类型/原因/时间 + 去申诉入口）。
  // - 排版重构（2026-09）：账号卡下方增加 app-toolbar 快捷导航（对齐原型
  //   me.html 的工具条：两步验证/登录设备/通知设置/OAuth 授权）；主栏收纳
  //   账号信息 + 登录设备管理（#sessions）+ 两步验证（#mfa，M18-MFA-01
  //   注册二维码）；侧栏为账户卡 + 快捷操作（去重）+ 图标化快捷入口。
  import { enhance } from '$app/forms';
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
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

  /** 侧栏图标化快捷入口（页面级导航改由 app-toolbar 承担，此处只留业务入口）。 */
  const quickLinks = [
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
    <section class="app-profile">
      <Avatar name={user.display_name || user.username} size="xl" />
      <div class="app-profile__body">
        <h2>
          {user.display_name || user.username}
          <span class="badge badge-level">LV.{user.level}</span>
        </h2>
        {#if user.bio}
          <p>{user.bio}</p>
        {:else}
          <p class="text-secondary">@{user.username}</p>
        {/if}
        <div class="app-profile__meta">
          <span><b>@{user.username}</b></span>
          <span>{user.email}</span>
        </div>
      </div>
      <div class="app-profile__actions">
        <Button text="编辑资料" variant="secondary" size="sm" icon="edit-3" href="/settings" />
      </div>
    </section>

    <div class="app-account-cards" style="margin-top:14px;">
      <div class="app-account-card">
        <small>经验</small>
        <strong>{activityXp(activity)}</strong>
        <span>LV.{user.level} · 成长进度</span>
      </div>
      <div class="app-account-card">
        <small>B币</small>
        <strong>{coinBalance?.amount ?? 0}</strong>
        <span>可用于商城与内容解锁</span>
      </div>
      <div class="app-account-card">
        <small>身份</small>
        <strong>{user.roles.length > 0 ? roleLabel(user.roles[0]) : '成员'}</strong>
        <span>{statusLabel[user.status] ?? user.status}</span>
      </div>
    </div>

    {#if topMessage}
      <p class="input-hint is-error" role="alert" style="margin-top:var(--space-4);">{topMessage}</p>
    {/if}

    <!-- 快捷导航工具条（原型 me.html app-toolbar 对齐）：页面内锚点 + 安全/
         通知/授权页直达，替代原先深埋侧栏的低可见性文字链接。 -->
    <div class="app-toolbar" style="margin-top:14px;margin-bottom:0;" role="navigation" aria-label="快捷导航">
      <Button text="两步验证" variant="secondary" size="sm" icon="shield" href="/mfa" />
      <Button text="登录设备" variant="secondary" size="sm" icon="smartphone" href="#sessions" />
      <Button text="通知设置" variant="secondary" size="sm" icon="bell" href="/notifications" />
      <Button text="OAuth 授权" variant="secondary" size="sm" icon="key" href="/settings#settings-oauth" />
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

        <!-- 登录设备管理（#sessions：工具条锚点目标） -->
        <div class="card" id="sessions">
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

        <!-- 两步验证（#mfa：工具条直达 /mfa 独立页；本卡提供同页管理） -->
        <div class="card" id="mfa">
          <div class="card-header"><span class="card-title">两步验证（MFA）</span></div>
          <div class="card-body">
            {#if mfaStep?.kind === 'enroll-challenge'}
              <!-- M18-MFA-01：注册二维码（服务端生成的 SVG data URL，<img>
                   渲染，SSR/无 JS 可直接扫码）+ 手工录入降级。 -->
              <div class="mfa-enroll">
                <div class="mfa-enroll__qr">
                  <p class="mfa-enroll__step"><span class="mfa-enroll__num">1</span>用认证器扫描二维码</p>
                  {#if mfaStep.qr_data_url}
                    <img
                      class="otp-qr"
                      src={mfaStep.qr_data_url}
                      alt="两步验证注册二维码（用认证器 App 扫描添加）"
                      width="180"
                      height="180"
                    />
                  {:else}
                    <p class="auth-hint" role="alert">二维码生成失败，请使用下方密钥手工添加。</p>
                  {/if}
                  <details class="mfa-manual">
                    <summary>无法扫码？手工录入密钥</summary>
                    <label class="input-label" for="mfa-secret">密钥（Base32）</label>
                    <input
                      type="text"
                      class="input-field"
                      id="mfa-secret"
                      value={mfaStep.secret_base32}
                      readonly
                    />
                    <p class="auth-hint mfa-manual__uri">{mfaStep.otpauth_uri}</p>
                  </details>
                </div>
                <div class="mfa-enroll__confirm">
                  <p class="mfa-enroll__step"><span class="mfa-enroll__num">2</span>输入 6 位动态验证码完成启用</p>
                  <p class="auth-hint">扫码后，在认证器中找到本账号，输入当前 6 位动态验证码。</p>
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
                    <div style="margin-top:var(--space-3);">
                      <Button text="完成启用" variant="primary" size="sm" type="submit" />
                    </div>
                  </form>
                  <form method="POST" action="?/mfa-cancel" use:enhance style="margin-top:var(--space-2);">
                    <Button text="取消" variant="ghost" size="sm" type="submit" />
                  </form>
                </div>
              </div>
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
              <div class="mfa-status-row">
                <div>
                  {#if user.mfa_enabled}
                    <span class="badge badge-success">已启用</span>
                    <span class="text-secondary" style="font-size:var(--text-sm);margin-left:var(--space-2);">登录时需要输入身份验证器验证码</span>
                  {:else}
                    <span class="badge badge-neutral">未启用</span>
                    <span class="text-secondary" style="font-size:var(--text-sm);margin-left:var(--space-2);">开启后登录时需要输入身份验证器验证码，安全性更高</span>
                  {/if}
                </div>
                <div style="display:flex;gap:var(--space-2);align-items:center;flex-wrap:wrap;">
                  {#if user.mfa_enabled}
                    <form method="POST" action="?/mfa-recovery" use:enhance>
                      <Button text="生成新恢复码" variant="secondary" size="sm" type="submit" />
                    </form>
                    <form method="POST" action="?/mfa-disable" use:enhance>
                      <Button text="停用两步验证" variant="danger" size="sm" type="submit" />
                    </form>
                  {:else}
                    <form method="POST" action="?/mfa-enroll" use:enhance>
                      <Button text="启用两步验证" variant="primary" size="sm" type="submit" />
                    </form>
                  {/if}
                </div>
              </div>
            {/if}
          </div>
        </div>
      </div>
      <div class="side-col">
        <!-- GAP-FIX 账户卡：经验 / B币 / 签到（GET /activity/summary；失败/缺失时
             整卡隐藏，不阻塞页面）。 -->
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
          </div>
        </div>

        <!-- 图标化快捷入口（业务页直达；页面内导航由上方工具条承担）。 -->
        <div class="card">
          <div class="card-header"><span class="card-title">快捷入口</span></div>
          <div class="card-body">
            <div class="quick-grid">
              {#each quickLinks as link (link.href)}
                <a href={link.href} class="quick-link">
                  <Icon name={link.icon} size={15} />
                  <span>{link.label}</span>
                </a>
              {/each}
              <a href="/users/{encodeURIComponent(user.username)}?tab=posts" class="quick-link">
                <Icon name="list" size={15} />
                <span>我的帖子</span>
              </a>
            </div>
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
  {:else if !error}
    <div class="empty-state"><div class="empty-state-title">加载中…</div></div>
  {/if}
</div>

<style>
  /* MFA 注册：扫码（QR）+ 确认码 双栏；窄屏折行为上下堆叠。 */
  .mfa-enroll {
    display: flex;
    gap: var(--space-5);
    flex-wrap: wrap;
  }
  .mfa-enroll__qr {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
    flex: 0 0 auto;
  }
  .mfa-enroll__confirm {
    flex: 1 1 240px;
    min-width: 240px;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .mfa-enroll__step {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    font-size: var(--text-base);
    font-weight: 600;
  }
  .mfa-enroll__num {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--color-brand);
    color: #fff;
    font-size: var(--text-xs);
    font-weight: 700;
    flex-shrink: 0;
  }
  .otp-qr {
    width: 180px;
    height: 180px;
    padding: 8px;
    background: #fff;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
  }
  .mfa-manual {
    width: 100%;
    max-width: 220px;
    font-size: var(--text-sm);
  }
  .mfa-manual summary {
    cursor: pointer;
    color: var(--color-brand);
    user-select: none;
  }
  .mfa-manual .input-field {
    margin-top: var(--space-2);
  }
  .mfa-manual__uri {
    margin: var(--space-2) 0 0;
    font-size: var(--text-xs);
    word-break: break-all;
  }
  .mfa-status-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    flex-wrap: wrap;
  }
  /* 侧栏图标化快捷入口 */
  .quick-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-2);
  }
  .quick-link {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 10px 11px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg-card);
    color: var(--color-text-secondary);
    font-size: var(--text-sm);
    text-decoration: none;
    transition: border-color 0.15s ease, color 0.15s ease, background 0.15s ease;
  }
  .quick-link:hover {
    border-color: var(--color-brand);
    color: var(--color-brand);
    background: var(--color-bg-subtle);
  }
  /* 工具条锚点滚定位时不被顶栏遮挡 */
  #sessions,
  #mfa {
    scroll-margin-top: var(--space-6, 24px);
  }
</style>
