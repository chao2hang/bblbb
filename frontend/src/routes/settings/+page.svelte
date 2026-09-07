<script lang="ts">
  // M03-UI-02：/settings 页——资料编辑 SSR 表单（无 JS 可提交）+ If-Match
  // 乐观并发 + 字段错误 + 保存后投影刷新。
  // - load 已取 user（含 version）与 OAuth 授权列表（GAP-FIX），form action
  //   `?/profile` 走服务端代理；
  // - 原生 form[method=POST] + use:enhance 渐进增强；
  // - 版本冲突（409）→ 提示横幅 + “加载最新资料”入口（enhance 下
  //   invalidateAll，无 JS 下整页刷新）；
  // - 保存成功 → form.user（更新后投影）直接渲染，use:enhance 默认
  //   invalidateAll 使 data 同步新版本；
  // - GAP-FIX 既有页面增强：资料可见性选择器（?/visibility → PATCH /me
  //   profile_visible_to，后端已支持）、修改密码表单（?/password → POST
  //   /me/password；TODO(BE-2) 后端端点待接入，当前提交会提示不可用）、
  //   OAuth 授权应用管理（?/revoke-oauth + DangerConfirm；同 TODO）；
  // - 只输出本人公开/账号字段，不输出任何会话 token（SSR 守卫见
  //   settings-nojs.test）。
  import { onMount } from 'svelte';
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import Button from '$lib/components/ui/Button.svelte';
  import Card from '$lib/components/ui/Card.svelte';
  import DangerConfirm from '$lib/components/ui/DangerConfirm.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { PROFILE_TEXT_LIMITS } from '$lib/profile';
  import { show } from '$lib/ui/toast';
  import { formatRelative } from '$lib/utils';
  import { readPreference, applyTheme, type ThemePreference } from '$lib/theme';
  import { applyThemeTokens, clearThemeTokens, type ActiveThemeView } from '$lib/theme/projection';
  import type { OAuthGrantItem } from '$lib/api/types';
  import type { SettingsFormResult, SettingsPageData } from './+page.server';

  let { data, form }: { data: SettingsPageData; form?: SettingsFormResult } = $props();

  const error = $derived(data.error);
  // 保存成功后优先展示 action 返回的新投影；否则用 load 数据。
  const user = $derived(form?.user ?? data.user);
  const grants = $derived(data.grants ?? []);
  const conflict = $derived(form?.conflict === true);
  const topMessage = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );
  const passwordResult = $derived(form?.password);
  const visibilityResult = $derived(form?.visibility);
  const revokeResult = $derived(form?.revokeOAuth);

  const limit = PROFILE_TEXT_LIMITS;
  let activeTab = $state('profile');
  let currentMode = $state<ThemePreference>('system');
  let activeThemeId = $state('default');

  const THEMES_LIST = [
    {
      id: 'default',
      name: 'BBLBB 经典赤墨 (原版默认)',
      desc: '经典暖珊瑚红点缀与米白宣纸底色',
      bg: 'linear-gradient(135deg, #f5f3ed 0%, #b23e2a 50%, #fffefb 100%)',
      tokens: {
        'color.background': '#f5f3ed',
        'color.surface': '#fffefb',
        'color.text': '#17211f',
        'color.muted': '#53605b',
        'color.accent': '#b23e2a',
        'color.border': '#d9d6cc'
      }
    },
    {
      id: 'chinese-elegance',
      name: '水墨青石 (中国风)',
      desc: '典雅含蓄的书卷水墨素雅与青石灰蓝',
      bg: 'linear-gradient(135deg, #f5f3ee 0%, #5a6c7d 50%, #fbfaf7 100%)',
      tokens: {
        'color.background': '#f5f3ee',
        'color.surface': '#fbfaf7',
        'color.text': '#1f1d1a',
        'color.muted': '#6b6b6b',
        'color.accent': '#5a6c7d',
        'color.border': '#e4e1d7'
      }
    },
    {
      id: 'midnight',
      name: '暗夜极光',
      desc: '深蓝灰与天蓝点缀的沉浸暗色',
      bg: 'linear-gradient(135deg, #0f172a 0%, #38bdf8 50%, #1e293b 100%)',
      tokens: {
        'color.background': '#0f172a',
        'color.surface': '#1e293b',
        'color.text': '#e2e8f0',
        'color.muted': '#94a3b8',
        'color.accent': '#38bdf8',
        'color.border': '#334155'
      }
    },
    {
      id: 'paper',
      name: '复古羊皮纸',
      desc: '温暖柔和的书卷复古质感',
      bg: 'linear-gradient(135deg, #faf6ef 0%, #b23e2a 50%, #ffffff 100%)',
      tokens: {
        'color.background': '#faf6ef',
        'color.surface': '#ffffff',
        'color.text': '#2c2c2c',
        'color.muted': '#736b5e',
        'color.accent': '#b23e2a',
        'color.border': '#e4dcce'
      }
    },
    {
      id: 'forest',
      name: '翡翠森林',
      desc: '清新自然的墨绿与薄荷翡翠色',
      bg: 'linear-gradient(135deg, #f0f5f2 0%, #0f756c 50%, #ffffff 100%)',
      tokens: {
        'color.background': '#f0f5f2',
        'color.surface': '#ffffff',
        'color.text': '#132a21',
        'color.muted': '#516f63',
        'color.accent': '#0f756c',
        'color.border': '#cfe0d8'
      }
    },
    {
      id: 'cyberpunk',
      name: '赛博霓虹',
      desc: '深紫暗夜与高亮粉紫霓虹碰撞',
      bg: 'linear-gradient(135deg, #181126 0%, #ec4899 50%, #241b35 100%)',
      tokens: {
        'color.background': '#181126',
        'color.surface': '#241b35',
        'color.text': '#f3f0f7',
        'color.muted': '#9d93b3',
        'color.accent': '#ec4899',
        'color.border': '#3b2d56'
      }
    }
  ];

  onMount(() => {
    currentMode = readPreference();
    if (typeof document !== 'undefined') {
      activeThemeId = document.documentElement.dataset.theme || 'default';
    }
    const hash = window.location.hash.replace(/^#settings-/, '');
    if (['profile', 'appearance', 'security', 'oauth', 'privacy'].includes(hash)) activeTab = hash;
  });

  function selectTab(tab: string): void {
    activeTab = tab;
    if (typeof window !== 'undefined') window.history.replaceState(null, '', `#settings-${tab}`);
  }

  function setDisplayMode(mode: ThemePreference) {
    currentMode = mode;
    applyTheme(mode);
    show(`已切换为${mode === 'light' ? '浅色模式' : mode === 'dark' ? '深色模式' : '跟随系统'}`, 'success');
  }

  function selectTheme(themeItem: { id: string; name: string; tokens?: Record<string, unknown> }) {
    activeThemeId = themeItem.id;
    if (themeItem.id === 'default' || themeItem.id === 'bblbb-classic') {
      clearThemeTokens();
    } else {
      applyThemeTokens({
        name: themeItem.id,
        revision: 1,
        tokens: themeItem.tokens ?? {},
        source: 'user_preference'
      });
    }
    show(`已应用「${themeItem.name}」主题`, 'success');
  }

  /** 资料可见性当前值（契约 Me.profile_visible_to；缺省 everyone）。 */
  const visibilityValue = $derived(user?.profile_visible_to ?? 'everyone');

  /** 后端时间戳为毫秒（M01-DB-08），formatRelative 口径为秒。 */
  function toSeconds(ts: number | null | undefined): number | null {
    if (typeof ts !== 'number' || !Number.isFinite(ts) || ts <= 0) return null;
    return ts > 1e11 ? Math.floor(ts / 1000) : ts;
  }

  // ── OAuth 撤销确认（DangerConfirm + 隐藏表单，模式同 admin/attachments） ──
  let revokeForm: HTMLFormElement | undefined = $state();
  let revokeTarget: OAuthGrantItem | null = $state(null);
  let revoking = $state(false);

  function openRevoke(grant: OAuthGrantItem): void {
    revokeTarget = grant;
  }
</script>

<svelte:head>
  <title>账号设置 — BBLBB</title>
</svelte:head>

<div class="container page-content app-settings-page">
  <div class="app-route-head">
    <div class="app-route-head__copy">
      <span class="app-kicker">ACCOUNT / SETTINGS</span>
      <h1 tabindex="-1">账号设置</h1>
      <p>个人资料、安全、设备、通知、OAuth 授权</p>
    </div>
  </div>

  <div class="app-settings-layout">
    <nav class="app-settings-nav" aria-label="设置导航">
      <button type="button" class:is-active={activeTab === 'profile'} onclick={() => selectTab('profile')}><span aria-hidden="true">◈</span>个人资料</button>
      <button type="button" class:is-active={activeTab === 'appearance'} onclick={() => selectTab('appearance')}><span aria-hidden="true">◐</span>外观与主题</button>
      <button type="button" class:is-active={activeTab === 'security'} onclick={() => selectTab('security')}><span aria-hidden="true">◇</span>账号安全</button>
      <a href="/me#sessions"><span aria-hidden="true">▣</span>登录设备</a>
      <a href="/notifications"><span aria-hidden="true">◌</span>通知设置</a>
      <button type="button" class:is-active={activeTab === 'oauth'} onclick={() => selectTab('oauth')}><span aria-hidden="true">⌁</span>OAuth 授权</button>
      <a href="/settings/privacy"><span aria-hidden="true">□</span>隐私设置</a>
    </nav>

    <div class="settings-content">
      {#if error && !user}
        <p class="input-hint is-error" role="alert">{error}</p>
      {/if}

      {#if user}
        <form
          class="card settings-panel settings-panel-profile"
          method="POST"
          action="?/profile"
          class:is-active={activeTab === 'profile'}
          use:enhance={() => {
            return async ({ result, update }) => {
              await update();
              // 保存成功（非 fail）→ 刷新 load 数据，让投影与版本保持最新。
              if (result.type === 'success') await invalidateAll();
            };
          }}
        >
          <input type="hidden" name="version" value={user.version} />

          <div class="card-header"><span class="card-title">基本资料</span></div>
          <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-4);">
            {#if conflict}
              <div class="alert alert-warning" role="alert" style="padding:var(--space-3);border:1px solid var(--color-warning);border-radius:var(--radius-md);">
                <p style="margin:0 0 var(--space-2);">资料已在其他窗口被修改（版本冲突）。请加载最新资料后再编辑，避免覆盖他人修改。</p>
                <div style="display:flex;gap:var(--space-2);">
                  <a class="btn btn-primary btn-sm" href="/settings">加载最新资料</a>
                </div>
              </div>
            {/if}
            {#if topMessage && !conflict}
              <p class="input-hint is-error" role="alert">{topMessage}</p>
            {/if}

            <div class="input-wrapper">
              <label class="input-label" for="set-display-name">昵称</label>
              <input
                type="text"
                class="input-field"
                id="set-display-name"
                name="display_name"
                value={user.display_name ?? ''}
                placeholder="显示昵称"
                maxlength={limit.display_name}
              />
              <p class="input-hint">用于帖子、回复和主页展示；留空则使用用户名（最多 {limit.display_name} 字）。</p>
            </div>

            <div class="input-wrapper">
              <label class="input-label" for="set-bio">简介</label>
              <textarea
                class="input-field"
                id="set-bio"
                name="bio"
                rows="4"
                placeholder="简单介绍一下自己"
                maxlength={limit.bio}
              >{user.bio ?? ''}</textarea>
              <p class="input-hint">显示在你的主页；封禁/注销中不对外展示（最多 {limit.bio} 字）。</p>
            </div>

            <div class="input-wrapper">
              <label class="input-label" for="set-signature">签名</label>
              <input
                type="text"
                class="input-field"
                id="set-signature"
                name="signature"
                value={user.signature ?? ''}
                placeholder="帖子下方展示的签名"
                maxlength={limit.signature}
              />
              <p class="input-hint">显示在你帖子与回复的下方（最多 {limit.signature} 字）。</p>
            </div>

            <div>
              <Button text="保存修改" variant="primary" size="sm" type="submit" />
            </div>
          </div>
        </form>

        <!-- GAP-FIX 资料可见性：PATCH /me profile_visible_to（后端已支持，
             users.rs update_me）。独立 action，与基本资料表单互不影响。 -->
        <form
          class="card settings-panel settings-panel-profile"
          method="POST"
          action="?/visibility"
          class:is-active={activeTab === 'profile'}
          use:enhance={() => {
            return async ({ result, update }) => {
              if (result.type === 'success') {
                const data = result.data as SettingsFormResult | undefined;
                show(data?.visibility?.message ?? '资料可见性已保存', 'success');
                await update();
                await invalidateAll();
              } else {
                if (result.type === 'failure') {
                  const data = result.data as SettingsFormResult | undefined;
                  show(data?.visibility?.message ?? data?.message ?? '保存失败，请重试', 'danger');
                }
                await update();
              }
            };
          }}
        >
          <input type="hidden" name="version" value={user.version} />
          <div class="card-header"><span class="card-title">资料可见性</span></div>
          <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-4);">
            {#if visibilityResult && !visibilityResult.ok}
              <p class="input-hint is-error" role="alert">{visibilityResult.message}</p>
            {:else if visibilityResult?.ok}
              <p class="input-hint" role="status">{visibilityResult.message}</p>
            {/if}
            <div class="input-wrapper">
              <label class="input-label" for="set-visibility">谁可以查看我的主页</label>
              <select class="input-field" id="set-visibility" name="profile_visible_to">
                <option value="everyone" selected={visibilityValue === 'everyone'}>公开（所有人，含未登录）</option>
                <option value="registered" selected={visibilityValue === 'registered'}>仅注册用户</option>
                <option value="nobody" selected={visibilityValue === 'nobody'}>私密（仅自己）</option>
              </select>
              <p class="input-hint">公开资料始终只包含昵称/简介/签名等安全投影；更严格的可见性会限制主页访问范围。</p>
            </div>
            <div>
              <Button text="保存可见性" variant="primary" size="sm" type="submit" />
            </div>
          </div>
        </form>

        <!-- 外观与主题设置面板 -->
        <section
          class="card settings-panel settings-panel-appearance"
          class:is-active={activeTab === 'appearance'}
          aria-label="外观与主题"
        >
          <div class="card-header">
            <span class="card-title">外观与色彩偏好</span>
          </div>
          <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-5, 20px);">
            <!-- 色彩模式切换 -->
            <div>
              <strong style="font-size:14px;display:block;margin-bottom:8px;">显示模式</strong>
              <div style="display:grid;grid-template-columns:repeat(auto-fit, minmax(200px, 1fr));gap:12px;">
                <button
                  type="button"
                  class="app-card"
                  style="border:2px solid {currentMode === 'light' ? 'var(--color-brand)' : 'var(--color-border)'};border-radius:var(--radius-md);padding:14px;text-align:left;cursor:pointer;background:var(--color-bg-card);"
                  onclick={() => setDisplayMode('light')}
                >
                  <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:6px;">
                    <strong style="font-size:14px;">☀️ 浅色模式</strong>
                    {#if currentMode === 'light'}<span class="sbadge sb-primary">生效中</span>{/if}
                  </div>
                  <p class="text-secondary" style="font-size:12px;margin:0;line-height:1.4;">温润宣纸米白质感底色，字迹舒适分明，不眩光。</p>
                </button>

                <button
                  type="button"
                  class="app-card"
                  style="border:2px solid {currentMode === 'dark' ? 'var(--color-brand)' : 'var(--color-border)'};border-radius:var(--radius-md);padding:14px;text-align:left;cursor:pointer;background:var(--color-bg-card);"
                  onclick={() => setDisplayMode('dark')}
                >
                  <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:6px;">
                    <strong style="font-size:14px;">🌙 深色模式</strong>
                    {#if currentMode === 'dark'}<span class="sbadge sb-primary">生效中</span>{/if}
                  </div>
                  <p class="text-secondary" style="font-size:12px;margin:0;line-height:1.4;">高对比纯黑夜色底色，弱光环境阅读柔和护眼。</p>
                </button>

                <button
                  type="button"
                  class="app-card"
                  style="border:2px solid {currentMode === 'system' ? 'var(--color-brand)' : 'var(--color-border)'};border-radius:var(--radius-md);padding:14px;text-align:left;cursor:pointer;background:var(--color-bg-card);"
                  onclick={() => setDisplayMode('system')}
                >
                  <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:6px;">
                    <strong style="font-size:14px;">💻 跟随系统</strong>
                    {#if currentMode === 'system'}<span class="sbadge sb-primary">生效中</span>{/if}
                  </div>
                  <p class="text-secondary" style="font-size:12px;margin:0;line-height:1.4;">自动同步操作系统与浏览器的深浅色外观偏好。</p>
                </button>
              </div>
            </div>

            <!-- 社区主题风格选择 -->
            <div>
              <strong style="font-size:14px;display:block;margin-bottom:4px;">社区主题风格</strong>
              <p class="text-secondary" style="font-size:12px;margin:0 0 12px 0;">选择你喜爱的全站设计配色，切换后全站按钮、卡片、底色与文字将同步调整。</p>
              <div style="display:grid;grid-template-columns:repeat(auto-fill, minmax(210px, 1fr));gap:12px;">
                {#each THEMES_LIST as t}
                  <div
                    class="app-card"
                    style="border:1px solid var(--color-border);border-radius:var(--radius-md);padding:12px;display:flex;flex-direction:column;gap:8px;background:var(--color-bg-card);"
                  >
                    <div style="height:48px;border-radius:var(--radius-sm);background:{t.bg};"></div>
                    <div>
                      <strong style="font-size:13px;">{t.name}</strong>
                      <p class="text-secondary" style="font-size:11px;margin:2px 0 0 0;line-height:1.3;">{t.desc}</p>
                    </div>
                    <div style="display:flex;align-items:center;justify-content:space-between;margin-top:auto;padding-top:4px;">
                      <button
                        type="button"
                        class="btn sm {activeThemeId === t.id ? 'ghost' : 'secondary'}"
                        disabled={activeThemeId === t.id}
                        onclick={() => selectTheme(t)}
                      >
                        {activeThemeId === t.id ? '当前生效' : '应用'}
                      </button>
                    </div>
                  </div>
                {/each}
              </div>
            </div>
          </div>
        </section>

        <!-- GAP-FIX 修改密码：POST /me/password（security 区）。TODO(BE-2)：
             后端端点尚未注册，当前提交返回失败提示；落地后成功即撤销其他会话。 -->
        <form
          class="card settings-panel settings-panel-security"
          method="POST"
          action="?/password"
          class:is-active={activeTab === 'security'}
          use:enhance={() => {
            return async ({ result, update }) => {
              await update();
              if (result.type === 'success') {
                const data = result.data as SettingsFormResult | undefined;
                show(data?.password?.message ?? '密码已更新', 'success');
                await invalidateAll();
              } else if (result.type === 'failure') {
                const data = result.data as SettingsFormResult | undefined;
                const message =
                  data?.password?.currentError ?? data?.password?.newError ?? data?.password?.message;
                if (message) show(message, 'danger');
              }
            };
          }}
        >
          <div class="card-header"><span class="card-title">修改密码</span></div>
          <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-4);">
            {#if passwordResult?.ok}
              <p class="input-hint" role="status">{passwordResult.message ?? '密码已更新，其他设备已下线'}</p>
            {/if}
            {#if passwordResult && !passwordResult.ok && passwordResult.message}
              <p class="input-hint is-error" role="alert">{passwordResult.message}</p>
            {/if}
            <div class="input-wrapper">
              <label class="input-label" for="set-current-password">当前密码</label>
              <input
                type="password"
                class="input-field"
                id="set-current-password"
                name="current_password"
                autocomplete="current-password"
                required
              />
              {#if passwordResult?.currentError}
                <p class="input-hint is-error" role="alert">{passwordResult.currentError}</p>
              {/if}
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="set-new-password">新密码</label>
              <input
                type="password"
                class="input-field"
                id="set-new-password"
                name="new_password"
                autocomplete="new-password"
                minlength="8"
                maxlength="128"
                required
              />
              {#if passwordResult?.newError}
                <p class="input-hint is-error" role="alert">{passwordResult.newError}</p>
              {/if}
              <p class="input-hint">8-128 位，需同时包含字母和数字；修改成功后其他设备会被强制下线。</p>
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="set-confirm-password">确认新密码</label>
              <input
                type="password"
                class="input-field"
                id="set-confirm-password"
                name="confirm_password"
                autocomplete="new-password"
                minlength="8"
                maxlength="128"
                required
              />
            </div>
            <div>
              <Button text="更新密码" variant="primary" size="sm" type="submit" />
            </div>
          </div>
        </form>

        <!-- GAP-FIX OAuth 授权应用：GET /me/oauth-grants + 每行撤销
             （DangerConfirm 确认后提交隐藏表单）。TODO(BE-2)：后端端点尚未
             注册，当前列表恒空（空态说明）；落地后自动显示真实授权。 -->
        <div class="card settings-panel settings-panel-oauth" class:is-active={activeTab === 'oauth'}>
          <div class="card-body">
            {#if revokeResult?.message}
              <p class="input-hint {revokeResult.ok ? '' : 'is-error'}" role="{revokeResult.ok ? 'status' : 'alert'}" style="margin-top:0;">{revokeResult.message}</p>
            {/if}
            {#if grants.length === 0}
              <EmptyState icon="key" title="暂无授权应用" desc="你使用 BBLBB 账号登录的第三方应用会显示在这里" />
            {:else}
              <ul style="list-style:none;margin:0;padding:0;display:flex;flex-direction:column;gap:var(--space-2);">
                {#each grants as grant (grant.client_id)}
                  <li style="border:var(--border-default);border-radius:var(--radius-md);padding:var(--space-3);display:flex;flex-wrap:wrap;gap:var(--space-2);align-items:center;">
                    <div style="min-width:0;flex:1;">
                      <div style="font-weight:var(--weight-medium);">{grant.client_name}</div>
                      <div class="text-secondary" style="font-size:var(--text-xs);margin-top:2px;display:flex;gap:var(--space-2);flex-wrap:wrap;">
                        <span>授权于 {formatRelative(toSeconds(grant.granted_at))}</span>
                        {#if grant.last_used_at}
                          <span>· 最近使用 {formatRelative(toSeconds(grant.last_used_at))}</span>
                        {/if}
                      </div>
                      {#if grant.scopes.length > 0}
                        <div style="display:flex;gap:var(--space-1);flex-wrap:wrap;margin-top:var(--space-1);">
                          {#each grant.scopes as scope}
                            <span class="badge badge-neutral">{scope}</span>
                          {/each}
                        </div>
                      {/if}
                    </div>
                    <Button
                      text="撤销授权"
                      variant="danger"
                      size="sm"
                      onclick={() => openRevoke(grant)}
                    />
                  </li>
                {/each}
              </ul>
              <p class="input-hint" style="margin-bottom:0;">撤销后该应用将无法再访问你的账号数据；下次登录需重新授权。</p>
            {/if}
          </div>
        </div>

        <Card>
          <div class="card-header"><span class="card-title">当前公开投影</span></div>
          <div class="card-body">
            <dl class="profile-about-list">
              <div class="profile-about-item"><dt>昵称</dt><dd>{user.display_name || user.username}</dd></div>
              <div class="profile-about-item"><dt>用户名</dt><dd>{user.username}</dd></div>
              {#if user.bio}<div class="profile-about-item"><dt>简介</dt><dd>{user.bio}</dd></div>{/if}
              {#if user.signature}<div class="profile-about-item"><dt>签名</dt><dd>{user.signature}</dd></div>{/if}
            </dl>
            <p class="input-hint">保存后主页与资料卡将按此公开投影展示（版本 v{user.version}）。</p>
          </div>
        </Card>

        <div class="card" style="margin-top:var(--space-4);">
          <div class="card-header"><span class="card-title">账号信息</span></div>
          <div class="card-body">
            <dl class="profile-about-list">
              <div class="profile-about-item"><dt>邮箱</dt><dd>{user.email}</dd></div>
              <div class="profile-about-item"><dt>状态</dt><dd>{user.status}</dd></div>
              <div class="profile-about-item"><dt>等级</dt><dd>LV.{user.level}</dd></div>
            </dl>
          </div>
        </div>

        <!-- 撤销 OAuth 授权：隐藏表单（client_id 取确认目标），DangerConfirm
             确认后 requestSubmit（模式同 admin/attachments 删除）。 -->
        <form
          method="POST"
          action="?/revoke-oauth"
          bind:this={revokeForm}
          use:enhance={() => {
            return async ({ result, update }) => {
              await update();
              if (result.type === 'success') {
                const data = result.data as SettingsFormResult | undefined;
                show(data?.revokeOAuth?.message ?? '已撤销授权', 'success');
                revokeTarget = null;
                await invalidateAll();
              } else if (result.type === 'failure') {
                const data = result.data as SettingsFormResult | undefined;
                show(data?.revokeOAuth?.message ?? '撤销授权失败，请重试', 'danger');
              }
              revoking = false;
            };
          }}
        >
          <input type="hidden" name="client_id" value={revokeTarget?.client_id ?? ''} />
        </form>

        <DangerConfirm
          open={revokeTarget !== null}
          title="撤销授权确认"
          description={revokeTarget ? `确定撤销「${revokeTarget.client_name}」对你账号的访问授权吗？撤销后该应用将立即失去访问权限。` : ''}
          confirmText="撤销授权"
          busyText="撤销中…"
          busy={revoking}
          onconfirm={() => {
            revoking = true;
            revokeForm?.requestSubmit();
          }}
          oncancel={() => {
            revokeTarget = null;
          }}
        />
      {:else if !error}
        <div class="empty-state"><div class="empty-state-title">加载中…</div></div>
      {/if}
    </div>
  </div>
</div>
