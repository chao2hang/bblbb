<script lang="ts">
  // M02-UX-05：/me 页——服务端安全投影（仅渲染自身账号可见字段，不输出
  // 任何会话 token）、账号状态/验证状态与 Session 设备管理。
  // - load 已取 user 与设备列表（+page.server.ts）；
  // - 设备列表：逐设备撤销（?/revoke，隐藏 session_id）与退出全部设备
  //   （?/logoutall）为原生 form[method=POST]（无 JS 可用，use:enhance
  //   渐进增强）；
  // - 当前设备按 last_seen_at 最大标记（后端每次请求滑动更新）；
  // - GAP-FIX 既有页面增强：账户卡（B币/签到，GET /activity/summary
  //   失败时整卡隐藏）、我的处罚区块（GET /me/sanctions，后端端点落地前恒空；
  //   有记录时显示类型/原因/时间 + 去申诉入口）。
  // - 排版重构（2026-09）：账号卡下方增加 app-toolbar 快捷导航（对齐原型
  //   me.html 的工具条：两步验证/登录设备/通知设置/OAuth 授权）；主栏收纳
  //   账号信息 + 登录设备管理（#sessions）+ 两步验证（#mfa，M18-MFA-01
  //   注册二维码）；侧栏为账户卡 + 快捷操作（去重）+ 图标化快捷入口。
  import { enhance } from '$app/forms';
  import CosmeticAvatar from '$lib/components/wardrobe/CosmeticAvatar.svelte';
  import CosmeticName from '$lib/components/wardrobe/CosmeticName.svelte';
  import ProfileCover from '$lib/components/ui/ProfileCover.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { formatRelative } from '$lib/utils';
  import type { MeActionData, MePageData } from './+page.server';
  import PageTitle from '$lib/components/PageTitle.svelte';

  let { data, form }: { data: MePageData; form?: MeActionData } = $props();

  const user = $derived(data.user);
  const presentation = $derived(data.presentation ?? null);
  const sessions = $derived(data.sessions);
  const currentId = $derived(data.currentSessionId);
  const error = $derived(data.error);
  // GAP-FIX 账户卡 / 我的处罚（load 增强数据；缺失时安全降级不渲染）。
  const activity = $derived(data.activity ?? null);
  const sanctions = $derived(data.sanctions ?? []);
  // M20-TRUST 信任等级进度（缺失时安全降级不渲染）。
  const trust = $derived(data.trust ?? null);
  const coinBalance = $derived((activity?.balances ?? []).find((b) => b.currency === 'coin'));
  const topMessage = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );
  const mfaStep = $derived(form?.mfa);

  /** 侧栏图标化快捷入口。 */
  const quickLinks = [
    { href: '/favorites', icon: 'star', label: '我的收藏' },
    { href: '/me/level', icon: 'award', label: '我的等级' },
    { href: '/me/attachments', icon: 'paperclip', label: '我的附件' },
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
    if (!role) return '成员';
    const r = role.toLowerCase();
    const map: Record<string, string> = {
      admin: '管理员',
      administrator: '管理员',
      mod: '版主',
      moderator: '版主',
      member: '成员'
    };
    return map[r] ?? role;
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

  <PageTitle title="我的" />

<div class="container page-content app-page" id="page-me">
  <h1 class="u-visually-hidden">我的</h1>

  {#if error}
    <div class="app-notice is-danger" role="alert">
      <span>{error}</span>
      <a href="/me">重新加载</a>
    </div>
  {/if}

  {#if user}
    <!-- 个人资料卡：参考信息小卡片设计，集中展示个人信息与账号状态 -->
    <section class="me-profile-card" aria-label="个人信息">
      <div class="me-coverwrap">
        <ProfileCover attachmentId={data.cover?.attachment_id} label="个人资料背景" class="me-cover" />
        <span class="badge badge-level me-cover-level">TL{trust?.level ?? user.level ?? 0}{trust?.name ? ` · ${trust.name}` : ''}</span>
        <div class="me-head">
          <div class="me-avatar">
            <CosmeticAvatar
              name={user.display_name || user.username}
              size="xl"
              presentation={presentation}
              avatarAttachmentId={user.avatar_attachment_id}
              seed={user.username ?? user.id}
            />
          </div>
          <div class="me-identity">
            <h2 class="me-name">
              <CosmeticName name={user.display_name || user.username} presentation={presentation} />
            </h2>
            <span class="me-handle">@{user.username}</span>
          </div>
        </div>
      </div>

      <div class="me-body">
        <div class="me-body-header">
          <div class="me-badges">
            {#if user.roles.length > 0}
              <span class="badge badge-role-admin">{roleLabel(user.roles[0])}</span>
            {:else}
              <span class="badge badge-neutral">成员</span>
            {/if}
            <span class="badge {statusBadge(user.status)}">{statusLabel[user.status] ?? user.status}</span>
            {#if user.mfa_enabled}
              <span class="badge badge-success">2FA 已开启</span>
            {/if}
          </div>
          <div class="me-actions">
            <Button text="编辑资料" variant="secondary" size="sm" icon="edit-3" href="/settings" />
          </div>
        </div>

        {#if user.signature}
          <p class="me-bio">{user.signature}</p>
        {:else}
          <p class="me-bio is-empty">暂无个性签名</p>
        {/if}

        <div class="me-info-strip">
          <div class="me-info-item">
            <span class="me-info-label">用户名</span>
            <span class="me-info-value">@{user.username}</span>
          </div>
          <div class="me-info-item">
            <span class="me-info-label">账号状态</span>
            <span class="me-info-value">
              <span class="badge {statusBadge(user.status)}">{statusLabel[user.status] ?? user.status}</span>
            </span>
          </div>
          <div class="me-info-item">
            <span class="me-info-label">角色</span>
            <span class="me-info-value">
              {#if user.roles.length > 0}
                <span class="badge badge-role-admin">{roleLabel(user.roles[0])}</span>
              {:else}
                <span class="badge badge-neutral">成员</span>
              {/if}
            </span>
          </div>
          {#if activity}
            <div class="me-info-item">
              <span class="me-info-label">B币</span>
              <span class="me-info-value"><strong>{coinBalance ? coinBalance.amount : 0}</strong></span>
            </div>
            <div class="me-info-item">
              <span class="me-info-label">签到</span>
              <span class="me-info-value"><strong>{activity.streak_days}</strong> 天</span>
            </div>
          {/if}
          <div class="me-info-item">
            <span class="me-info-label">登录设备</span>
            <span class="me-info-value"><strong>{sessions.length}</strong> 台</span>
          </div>
        </div>
      </div>
    </section>

    <!-- 快捷导航（紧贴资料卡下方，简洁行内链接） -->
    <nav class="me-nav" aria-label="快捷导航">
      <a href="/mfa"><Icon name="shield" size={14} />两步验证</a>
      <a href="#sessions"><Icon name="smartphone" size={14} />登录设备</a>
      <a href="/settings#settings-notifications"><Icon name="bell" size={14} />通知设置</a>
      <a href="/settings#settings-oauth"><Icon name="key" size={14} />OAuth 授权</a>
      <a href="/settings"><Icon name="settings" size={14} />账号设置</a>
    </nav>

    {#if topMessage}
      <p class="input-hint is-error" role="alert" style="margin-top:var(--space-4);">{topMessage}</p>
    {/if}

    <div class="content-grid" style="margin-top:var(--space-4);">
      <div class="main-col">
        <!-- 登录设备管理 -->
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

        <!-- 两步验证 -->
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
                  <li style="font-family:var(--font-family-mono);font-variant-numeric:tabular-nums;padding:var(--space-2);border:1px solid var(--color-border);border-radius:var(--radius-sm);text-align:center;">{code}</li>
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
              <form method="POST" action="?/re-auth" use:enhance novalidate style="margin-top:var(--space-4);">
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
                <div style="margin-top:var(--space-3);">
                  <Button text="验证身份" variant="primary" size="sm" type="submit" />
                </div>
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
        <!-- 账户卡：资产 / 签到 + 快捷操作（等级体系已统一为信任等级 TL0–TL4） --\>
        {#if activity}
          <div class="card">
            <div class="card-header">
              <span class="card-title">账户与资产</span>
              <span class="badge badge-level">TL{trust?.level ?? user.level ?? 0}</span>
            </div>
            <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
              <div style="display:flex;justify-content:space-between;align-items:baseline;">
                <span class="text-secondary" style="font-size:var(--text-sm);">B币余额</span>
                <strong style="font-variant-numeric:tabular-nums;">{coinBalance ? coinBalance.amount : '—'}</strong>
              </div>
              <div style="display:flex;justify-content:space-between;align-items:baseline;">
                <span class="text-secondary" style="font-size:var(--text-sm);">连续签到</span>
                <span style="font-variant-numeric:tabular-nums;">{activity.streak_days} 天{activity.checked_in_today ? '（今日已签）' : ''}</span>
              </div>
              <div style="display:flex;flex-direction:column;gap:var(--space-2);margin-top:var(--space-1);padding-top:var(--space-3);border-top:var(--border-default);">
                <Button text="发布新帖" variant="primary" size="sm" icon="pen-line" href="/editor" />
                <a class="btn btn-secondary btn-sm" href="/me/balance" style="text-align:center;">签到 / 积分明细</a>
                <a class="btn btn-secondary btn-sm" href="/me/level" style="text-align:center;">社区信任等级中心</a>
              </div>
            </div>
          </div>
        {/if}

        <!-- M20-TRUST 信任等级卡：当前等级 + 下一级逐项进度（LinuxDo 式 TL0–TL4） -->
        {#if trust}
          <div class="card">
            <div class="card-header">
              <span class="card-title">信任等级</span>
              <span class="badge badge-level">TL{trust.level} · {trust.name}</span>
            </div>
            <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
              {#if trust.grace_until}
                <p class="input-hint" style="margin:0;">
                  TL3 考核宽限期至 {new Date(trust.grace_until).toLocaleDateString()}，期间不降级。
                </p>
              {/if}
              {#if trust.next_level}
                {@const next = trust.next_level}
                {#if next.manual_only}
                  <p class="input-hint" style="margin:0;">
                    TL{next.level}（{next.name}）仅可由工作人员手动授予。
                  </p>
                {:else}
                  <p class="input-hint" style="margin:0;">
                    距 TL{next.level}（{next.name}）
                    {next.eligible ? '条件已全部满足，待系统晋升。' : '：'}
                  </p>
                  <ul style="list-style:none;margin:0;padding:0;display:flex;flex-direction:column;gap:var(--space-1);">
                    {#each next.requirements as req (req.key)}
                      <li style="display:flex;justify-content:space-between;gap:var(--space-2);font-size:var(--text-sm);">
                        <span style={req.met ? 'color:var(--color-success);' : 'color:var(--color-danger);'}>
                          {req.met ? '✓' : '·'} {req.label}
                        </span>
                        <span style="font-variant-numeric:tabular-nums;white-space:nowrap;" class="text-secondary">
                          {req.current} / {req.required}
                        </span>
                      </li>
                    {/each}
                  </ul>
                {/if}
              {/if}
              {#if trust.summary}
                <p class="input-hint" style="margin:0;">{trust.summary}</p>
              {/if}
            </div>
          </div>
        {/if}

        <!-- 快捷入口 -->
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
      <!-- GAP-FIX 我的处罚：load 取 GET /me/sanctions；空列表时不渲染此卡。 -->
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
  /* 快捷导航条：紧贴资料卡下方，行内图标链接 */
  .me-nav {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    margin-top: var(--space-3);
    padding: var(--space-2) 0;
    flex-wrap: wrap;
  }
  .me-nav a {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 6px 12px;
    border-radius: var(--radius-sm);
    color: var(--color-text-secondary);
    font-size: var(--text-sm);
    text-decoration: none;
    transition: color 0.15s, background 0.15s;
  }
  .me-nav a:hover {
    color: var(--color-brand);
    background: var(--color-bg-subtle);
  }
  /* 个人信息卡片（参考信息小卡片） */
  .me-profile-card {
    position: relative;
    overflow: hidden;
    background: var(--color-bg-card);
    border: var(--border-default);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-sm);
  }
  .me-coverwrap {
    position: relative;
  }
  :global(.me-cover) {
    height: 140px;
    min-height: 140px;
    background:
      radial-gradient(110% 150% at 90% -20%, color-mix(in srgb, var(--color-brand) 22%, transparent), transparent 55%),
      linear-gradient(135deg, var(--color-bg-inset), var(--color-brand-soft));
    background-position: center;
    background-size: cover;
    border-bottom: 1px solid var(--color-border);
  }
  .me-cover-level {
    position: absolute;
    top: var(--space-3);
    left: var(--space-3);
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    box-shadow: var(--shadow-sm);
    z-index: 2;
  }
  .me-head {
    position: absolute;
    left: var(--space-4);
    right: var(--space-4);
    bottom: -22px;
    display: flex;
    align-items: flex-end;
    gap: var(--space-3);
    margin-top: 0;
    z-index: 3;
    pointer-events: none;
  }
  .me-head > * {
    pointer-events: auto;
  }
  .me-avatar :global(.avatar) {
    border: 3px solid var(--color-bg-card);
    box-shadow: var(--shadow-sm);
  }
  .me-identity {
    flex: 1;
    min-width: 0;
    margin-bottom: 24px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .me-name {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: var(--weight-bold);
    line-height: 1.2;
    color: var(--color-text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  :global(html.dark) .me-name {
    text-shadow: 0 1px 4px rgba(0, 0, 0, 0.8);
  }
  .me-handle {
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
    line-height: 1.2;
  }
  :global(html.dark) .me-handle {
    text-shadow: 0 1px 3px rgba(0, 0, 0, 0.8);
  }
  .me-body {
    position: relative;
    padding: var(--space-4);
    padding-top: calc(var(--space-3) + 20px);
  }
  .me-body-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    flex-wrap: wrap;
    margin-bottom: var(--space-3);
  }
  .me-badges {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .me-actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-shrink: 0;
  }
  .me-bio {
    margin: 0 0 var(--space-3);
    font-size: var(--text-sm);
    line-height: var(--line-height-relaxed);
    color: var(--color-text-secondary);
    word-break: break-word;
  }
  .me-bio.is-empty {
    color: var(--color-text-tertiary);
    font-style: italic;
  }
  .me-info-strip {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-4);
    padding: var(--space-2) var(--space-3);
    background: var(--color-bg-subtle);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    font-size: var(--text-xs);
  }
  .me-info-item {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
  }
  .me-info-label {
    color: var(--color-text-tertiary);
  }
  .me-info-value {
    color: var(--color-text-primary);
  }
  .me-info-value strong {
    font-variant-numeric: tabular-nums;
  }
  @media (max-width: 767px) {
    :global(.me-cover) {
      height: 110px;
      min-height: 110px;
    }
    .me-head {
      left: var(--space-3);
      right: var(--space-3);
      bottom: -18px;
      gap: var(--space-2);
    }
    .me-identity {
      margin-bottom: 20px;
    }
    .me-name {
      font-size: var(--text-lg);
    }
    .me-body {
      padding: var(--space-3);
      padding-top: calc(var(--space-3) + 16px);
    }
    .me-info-strip {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: var(--space-2);
    }
  }
  /* 工具条锚点滚定位时不被顶栏遮挡 */
  #sessions,
  #mfa {
    scroll-margin-top: var(--space-6, 24px);
  }
</style>
