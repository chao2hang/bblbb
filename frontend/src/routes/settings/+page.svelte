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
  // - 修改密码表单（?/password → POST /me/password，
  //   成功后撤销其他会话）、OAuth 授权应用管理（?/revoke-oauth + DangerConfirm）；
  // - 只输出本人公开/账号字段，不输出任何会话 token（SSR 守卫见
  //   settings-nojs.test）。
  import { tick } from 'svelte';
  import { onMount } from 'svelte';
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import ProfileCover from '$lib/components/ui/ProfileCover.svelte';
  import AttachmentUploader from '$lib/components/upload/AttachmentUploader.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import DangerConfirm from '$lib/components/ui/DangerConfirm.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { PROFILE_TEXT_LIMITS } from '$lib/profile';
  import { show } from '$lib/ui/toast';
  import { formatRelative } from '$lib/utils';
  import { readPreference, applyTheme, type ThemePreference } from '$lib/theme';
  import { applyThemeTokens, clearThemeTokens, type ActiveThemeView } from '$lib/theme/projection';
  import {
    getNotificationPreferences,
    setNotificationPreference,
    type NotificationPreference
  } from '$lib/api/client';
  import type { OAuthGrantItem } from '$lib/api/types';
  import type { SettingsFormResult, SettingsPageData } from './+page.server';
  import PageTitle from '$lib/components/PageTitle.svelte';
  import SettingsNav from '$lib/components/SettingsNav.svelte';
  import { registerPasskey, passkeyErrorMessage, passkeySupported } from '$lib/mfa/passkey';
  import type { PasskeyInfo } from '$lib/mfa/passkey-types';

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
  const revokeResult = $derived(form?.revokeOAuth);

  const limit = PROFILE_TEXT_LIMITS;
  let activeTab = $state('profile');
  let currentMode = $state<ThemePreference>('system');
  let activeThemeId = $state('default');
  let currentAvatarId = $state<string | null>(null);
  let currentCoverId = $state<string | null>(null);
  let displayNameVal = $state('');
  let signatureVal = $state('');

  let initialSyncDone = false;
  $effect(() => {
    if (!initialSyncDone) {
      if (user) {
        currentAvatarId = user.avatar_attachment_id ?? null;
        displayNameVal = user.display_name ?? '';
        signatureVal = user.signature ?? '';
      }
      if (data?.cover !== undefined) {
        currentCoverId = data.cover?.attachment_id ?? null;
      }
      initialSyncDone = true;
    }
  });

  // ── 通知偏好（从 /notifications 迁移至设置页） ──
  let prefs = $state<NotificationPreference[]>([]);
  let prefsLoading = $state(false);
  let prefsError = $state<string | null>(null);

  const categoryLabels: Record<string, string> = {
    reply: '回复',
    reaction: '点赞',
    activity: '互动',
    moderation: '审核',
    system: '系统',
    security: '安全',
    digest: '摘要'
  };

  const categoryDescriptions: Record<string, string> = {
    activity: '他人的点赞、回复和提及',
    moderation: '内容被审核处理与申诉结果',
    system: '账号与站点运营相关提醒',
    security: '登录与账号安全提醒',
    digest: '周期性的动态摘要'
  };

  async function loadPrefs() {
    prefsLoading = true;
    prefsError = null;
    try {
      const result = await getNotificationPreferences(fetch);
      prefs = result.items;
    } catch {
      prefsError = '偏好加载失败';
    } finally {
      prefsLoading = false;
    }
  }

  async function togglePref(p: NotificationPreference, key: 'email_enabled' | 'in_app_enabled' | 'push_enabled') {
    const next = { ...p, [key]: !p[key] };
    try {
      await setNotificationPreference(fetch, next);
      Object.assign(p, next);
      prefsError = null;
    } catch {
      prefsError = '偏好保存失败（安全通知不可完全关闭）';
    }
  }

  // 社区主题风格（与官方预置包 v1.1 日/夜双模式一致：日间 6 色 +
  // 夜间 color.*.dark 变体；应用后随上方浅色/深色模式自动切换色板）。
  // bg 为昼夜分区预览：左日间 → 中间品牌色分界 → 右夜间。
  const THEMES_LIST = [
    {
      id: 'default',
      name: 'BBLBB 经典赤墨 (原版默认)',
      desc: '日间暖珊瑚红×米白宣纸，夜间墨绿×珊瑚橙（日/夜双模式）',
      bg: 'linear-gradient(105deg, #f5f3ed 0%, #f5f3ed 40%, #b23e2a 49%, #b23e2a 51%, #101b19 60%, #101b19 100%)',
      tokens: {
        'color.background': '#f5f3ed',
        'color.surface': '#fffefb',
        'color.text': '#17211f',
        'color.muted': '#53605b',
        'color.accent': '#b23e2a',
        'color.border': '#d9d6cc',
        'color.background.dark': '#101b19',
        'color.surface.dark': '#172522',
        'color.text.dark': '#f5f3ea',
        'color.muted.dark': '#b5c0ba',
        'color.accent.dark': '#f27759',
        'color.border.dark': '#30433e'
      }
    },
    {
      id: 'chinese-elegance',
      name: '水墨青石 (中国风)',
      desc: '日间宣纸水墨×青石蓝，夜间宿墨玄青×月白（日/夜双模式）',
      bg: 'linear-gradient(105deg, #f5f3ee 0%, #f5f3ee 40%, #5a6c7d 49%, #5a6c7d 51%, #171a1d 60%, #171a1d 100%)',
      tokens: {
        'color.background': '#f5f3ee',
        'color.surface': '#fbfaf7',
        'color.text': '#1f1d1a',
        'color.muted': '#6b6b6b',
        'color.accent': '#5a6c7d',
        'color.border': '#e4e1d7',
        'color.background.dark': '#171a1d',
        'color.surface.dark': '#202429',
        'color.text.dark': '#e7e5df',
        'color.muted.dark': '#9aa0a3',
        'color.accent.dark': '#7f95a8',
        'color.border.dark': '#2e343b'
      }
    },
    {
      id: 'midnight',
      name: '暗夜极光',
      desc: '日间极昼浅蓝×晴空青，夜间深蓝灰×天蓝极光（日/夜双模式）',
      bg: 'linear-gradient(105deg, #eef3f8 0%, #eef3f8 40%, #0284c7 49%, #0284c7 51%, #0f172a 60%, #0f172a 100%)',
      tokens: {
        'color.background': '#eef3f8',
        'color.surface': '#ffffff',
        'color.text': '#16202e',
        'color.muted': '#5b6b7f',
        'color.accent': '#0284c7',
        'color.border': '#d4deea',
        'color.background.dark': '#0f172a',
        'color.surface.dark': '#1e293b',
        'color.text.dark': '#e2e8f0',
        'color.muted.dark': '#94a3b8',
        'color.accent.dark': '#38bdf8',
        'color.border.dark': '#334155'
      }
    },
    {
      id: 'paper',
      name: '复古羊皮纸',
      desc: '日间暖羊皮×朱砂红，夜间灯火书斋×琥珀（日/夜双模式）',
      bg: 'linear-gradient(105deg, #faf6ef 0%, #faf6ef 40%, #b23e2a 49%, #b23e2a 51%, #1b1813 60%, #1b1813 100%)',
      tokens: {
        'color.background': '#faf6ef',
        'color.surface': '#ffffff',
        'color.text': '#2c2c2c',
        'color.muted': '#736b5e',
        'color.accent': '#b23e2a',
        'color.border': '#e4dcce',
        'color.background.dark': '#1b1813',
        'color.surface.dark': '#25211a',
        'color.text.dark': '#e9e2d2',
        'color.muted.dark': '#a89e8d',
        'color.accent.dark': '#e07856',
        'color.border.dark': '#3a342a'
      }
    },
    {
      id: 'forest',
      name: '翡翠森林',
      desc: '日间薄荷浅林×墨绿，夜间深林夜色×翡翠荧光（日/夜双模式）',
      bg: 'linear-gradient(105deg, #f0f5f2 0%, #f0f5f2 40%, #0f756c 49%, #0f756c 51%, #0f1713 60%, #0f1713 100%)',
      tokens: {
        'color.background': '#f0f5f2',
        'color.surface': '#ffffff',
        'color.text': '#132a21',
        'color.muted': '#516f63',
        'color.accent': '#0f756c',
        'color.border': '#cfe0d8',
        'color.background.dark': '#0f1713',
        'color.surface.dark': '#17231c',
        'color.text.dark': '#ddebe2',
        'color.muted.dark': '#8fa89b',
        'color.accent.dark': '#40c9a2',
        'color.border.dark': '#27392f'
      }
    },
    {
      id: 'cyberpunk',
      name: '赛博霓虹',
      desc: '日间雾紫纸面×热粉，夜间深紫暗夜×粉紫霓虹（日/夜双模式）',
      bg: 'linear-gradient(105deg, #f5f1fa 0%, #f5f1fa 40%, #db2777 49%, #db2777 51%, #181126 60%, #181126 100%)',
      tokens: {
        'color.background': '#f5f1fa',
        'color.surface': '#ffffff',
        'color.text': '#251c38',
        'color.muted': '#7d7296',
        'color.accent': '#db2777',
        'color.border': '#ded4ee',
        'color.background.dark': '#181126',
        'color.surface.dark': '#241b35',
        'color.text.dark': '#f3f0f7',
        'color.muted.dark': '#9d93b3',
        'color.accent.dark': '#ec4899',
        'color.border.dark': '#3b2d56'
      }
    }
  ];

  onMount(() => {
    currentMode = readPreference();
    if (typeof document !== 'undefined') {
      // 数据型主题名在 data-theme-name（dataset.theme 归日夜模式 light/dark 所有）
      const domThemeName = document.documentElement.dataset.themeName;
      activeThemeId = domThemeName && domThemeName !== 'default' ? domThemeName : 'default';
    }
    const parseHash = () => {
      const hash = window.location.hash.replace(/^#(settings-)?/, '');
      if (['profile', 'appearance', 'security', 'oauth', 'privacy', 'notifications'].includes(hash)) {
        activeTab = hash;
        if (hash === 'notifications') loadPrefs();
      }
    };
    parseHash();
    window.addEventListener('hashchange', parseHash);
    return () => {
      window.removeEventListener('hashchange', parseHash);
    };
  });

  function selectTab(tab: string): void {
    activeTab = tab;
    if (typeof window !== 'undefined') window.history.replaceState(null, '', `#settings-${tab}`);
    if (tab === 'notifications' && prefs.length === 0) loadPrefs();
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

  // ── 即时持久化头像（上传即生效，无须等待点击底部“保存修改”） ──
  let updatingAvatar = $state(false);
  let avatarFormEl = $state<HTMLFormElement>();

  function instantUpdateAvatar(newAttachmentId: string | null) {
    if (!user) return;
    updatingAvatar = true;
    currentAvatarId = newAttachmentId;
    const form = document.getElementById('instant-avatar-form') as HTMLFormElement | null;
    const input = document.getElementById('instant-avatar-input') as HTMLInputElement | null;
    if (input) {
      input.value = newAttachmentId ?? '';
    }
    form?.requestSubmit();
  }

  // ── Passkey 状态与注册 ──
  const passkeyEnabled = $derived(data.passkeyEnabled ?? false);
  const passkeys = $derived(data.passkeys ?? []);
  const passkeysError = $derived(data.passkeysError);
  let passkeyName = $state('');
  let passkeyBusy = $state(false);

  function formatPasskeyMs(ms: number | null): string {
    if (!ms) return '—';
    return new Date(ms).toLocaleString();
  }

  async function addPasskey() {
    if (passkeyBusy) return;
    if (!passkeySupported()) {
      show('当前浏览器不支持 Passkey（需 HTTPS 或 localhost 且需现代浏览器）', 'danger');
      return;
    }
    passkeyBusy = true;
    try {
      const beginRes = await fetch('/mfa/passkey/begin', {
        method: 'POST',
        headers: { Accept: 'application/json' }
      });
      if (!beginRes.ok) {
        const problem = (await beginRes.json().catch(() => null)) as { message?: string } | null;
        throw new Error(problem?.message || '开始注册失败，请重试');
      }
      const credential = await registerPasskey(
        (await beginRes.json()) as Parameters<typeof registerPasskey>[0]
      );
      const confirmRes = await fetch('/mfa/passkey/confirm', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
        body: JSON.stringify({ name: passkeyName.trim() || undefined, credential })
      });
      if (!confirmRes.ok) {
        const problem = (await confirmRes.json().catch(() => null)) as { message?: string } | null;
        throw new Error(problem?.message || '确认注册失败，请重试');
      }
      show('Passkey 已添加', 'success');
      passkeyName = '';
      await invalidateAll();
    } catch (e) {
      show(passkeyErrorMessage(e), 'danger');
    } finally {
      passkeyBusy = false;
    }
  }
</script>

  <PageTitle title="账号设置" />

<div class="container page-content app-page app-settings-page" id="page-settings">
  <h1 class="u-visually-hidden">账号设置</h1>

  <div class="app-settings-layout">
    <SettingsNav active={activeTab} onSelectTab={selectTab} />

    <div class="settings-content">
      {#if error && !user}
        <div class="app-notice is-danger" role="alert">
          <span>{error}</span>
          <a href="/settings">重新加载</a>
        </div>
      {/if}

      {#if user}
        <form
          id="settings-panel-profile"
          class="card settings-panel settings-panel-profile"
          method="POST"
          action="?/profile"
          class:is-active={activeTab === 'profile'}
          use:enhance={() => {
            return async ({ result, update }) => {
              if (result.type === 'success') {
                show('个人资料与头像封面已保存', 'success');
                await update();
                await invalidateAll();
              } else {
                if (result.type === 'failure') {
                  const data = result.data as SettingsFormResult | undefined;
                  show(data?.message ?? '保存失败，请检查后重试', 'danger');
                }
                await update();
              }
            };
          }}
        >
          <input type="hidden" name="version" value={user.version} />
          <input type="hidden" name="avatar_attachment_id" value={currentAvatarId ?? ''} />
          <input type="hidden" name="cover_attachment_id" value={currentCoverId ?? ''} />

          <div class="card-header" style="display:flex;align-items:center;justify-content:space-between;gap:var(--space-2);flex-wrap:wrap;">
            <div>
              <span class="card-title">编辑资料</span>
              <p class="input-hint" style="margin:2px 0 0;">修改你的头像、昵称与个性签名；背景装扮与主页效果请前往个人主页。</p>
            </div>
            <a href="/me" class="btn btn-secondary btn-sm" style="display:inline-flex;align-items:center;gap:4px;text-decoration:none;">
              <Icon name="user" size={14} />
              <span>查看主页展示</span>
            </a>
          </div>
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

            <!-- 头像设置与上传 -->
            <div class="avatar-editor-section" style="display:flex;align-items:center;gap:var(--space-4);padding-bottom:var(--space-4);border-bottom:var(--border-default);flex-wrap:wrap;">
              <div style="flex-shrink:0;">
                <Avatar
                  name={user.display_name || user.username}
                  size="xl"
                  attachmentId={currentAvatarId}
                  seed={user.username ?? user.id}
                />
              </div>
              <div style="flex:1;min-width:220px;display:flex;flex-direction:column;gap:var(--space-2);">
                <div>
                  <strong style="font-size:var(--text-sm);color:var(--color-text-primary);">个人头像</strong>
                  <p class="input-hint" style="margin:2px 0 0;">支持 JPG、PNG、WebP、GIF 等格式图片（上传或更换后立即自动保存并生效）。</p>
                </div>
                <div style="display:flex;align-items:center;gap:var(--space-2);flex-wrap:wrap;">
                  <AttachmentUploader
                    accept="image/*"
                    label={updatingAvatar ? "正在保存头像…" : (currentAvatarId ? "更换头像" : "上传新头像")}
                    waitReady={true}
                    showQuota={false}
                    autoUpload={true}
                    onReady={(a) => {
                      void instantUpdateAvatar(a.id);
                    }}
                  />
                  {#if currentAvatarId}
                    <Button
                      text="恢复默认头像"
                      variant="secondary"
                      size="sm"
                      type="button"
                      disabled={updatingAvatar}
                      onclick={() => {
                        void instantUpdateAvatar(null);
                      }}
                    />
                  {/if}
                </div>
              </div>
            </div>

            <div class="input-wrapper">
              <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:var(--space-1);">
                <label class="input-label" for="set-display-name" style="margin:0;">显示昵称</label>
                <span class="input-hint" style="margin:0;font-size:var(--text-xs);font-variant-numeric:tabular-nums;">{displayNameVal.length} / {limit.display_name}</span>
              </div>
              <input
                type="text"
                class="input-field"
                id="set-display-name"
                name="display_name"
                bind:value={displayNameVal}
                placeholder="设置你的展示昵称"
                maxlength={limit.display_name}
              />
              <p class="input-hint">用于帖子、回复和主页展示；留空则默认展示用户名（最多 {limit.display_name} 字）。</p>
            </div>

            <div class="input-wrapper">
              <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:var(--space-1);">
                <label class="input-label" for="set-signature" style="margin:0;">个性签名</label>
                <span class="input-hint" style="margin:0;font-size:var(--text-xs);font-variant-numeric:tabular-nums;">{signatureVal.length} / {limit.signature}</span>
              </div>
              <textarea
                class="input-field app-textarea"
                id="set-signature"
                name="signature"
                bind:value={signatureVal}
                placeholder="介绍一下自己，或者写一句喜欢的格言（显示在帖子与回复下方）"
                maxlength={limit.signature}
                rows="3"
                style="resize:vertical;min-height:72px;"
              >{user.signature ?? ''}</textarea>
              <p class="input-hint">显示在你发表的帖子与回复底部（最多 {limit.signature} 字）。</p>
            </div>

            <div>
              <Button text="保存修改" variant="primary" size="sm" type="submit" />
            </div>
          </div>
        </form>

        <!-- 外观与主题设置面板 -->
        <section
          id="settings-panel-appearance"
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
              <div role="radiogroup" aria-label="显示模式" style="display:grid;grid-template-columns:repeat(auto-fit, minmax(200px, 1fr));gap:12px;">
                <button
                  type="button"
                  role="radio"
                  aria-checked={currentMode === 'light'}
                  aria-label="浅色模式"
                  class="app-card"
                  style="border:2px solid {currentMode === 'light' ? 'var(--color-brand)' : 'var(--color-border)'};border-radius:var(--radius-md);padding:14px;text-align:left;cursor:pointer;background:var(--color-bg-card);"
                  onclick={() => setDisplayMode('light')}
                >
                  <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:6px;">
                    <strong style="font-size:14px;display:inline-flex;align-items:center;gap:6px;">
                      <Icon name="sun" size={14} />浅色模式
                    </strong>
                    {#if currentMode === 'light'}<span class="sbadge sb-primary">生效中</span>{/if}
                  </div>
                  <p class="text-secondary" style="font-size:12px;margin:0;line-height:1.4;">温润宣纸米白质感底色，字迹舒适分明，不眩光。</p>
                </button>

                <button
                  type="button"
                  role="radio"
                  aria-checked={currentMode === 'dark'}
                  aria-label="深色模式"
                  class="app-card"
                  style="border:2px solid {currentMode === 'dark' ? 'var(--color-brand)' : 'var(--color-border)'};border-radius:var(--radius-md);padding:14px;text-align:left;cursor:pointer;background:var(--color-bg-card);"
                  onclick={() => setDisplayMode('dark')}
                >
                  <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:6px;">
                    <strong style="font-size:14px;display:inline-flex;align-items:center;gap:6px;">
                      <Icon name="moon" size={14} />深色模式
                    </strong>
                    {#if currentMode === 'dark'}<span class="sbadge sb-primary">生效中</span>{/if}
                  </div>
                  <p class="text-secondary" style="font-size:12px;margin:0;line-height:1.4;">高对比纯黑夜色底色，弱光环境阅读柔和护眼。</p>
                </button>

                <button
                  type="button"
                  role="radio"
                  aria-checked={currentMode === 'system'}
                  aria-label="跟随系统模式"
                  class="app-card"
                  style="border:2px solid {currentMode === 'system' ? 'var(--color-brand)' : 'var(--color-border)'};border-radius:var(--radius-md);padding:14px;text-align:left;cursor:pointer;background:var(--color-bg-card);"
                  onclick={() => setDisplayMode('system')}
                >
                  <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:6px;">
                    <strong style="font-size:14px;display:inline-flex;align-items:center;gap:6px;">
                      <Icon name="monitor" size={14} />跟随系统
                    </strong>
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

        <!-- GAP-FIX 修改密码：POST /me/password（security 区）；成功后撤销其他会话。 -->
        <form
          id="settings-panel-security"
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

        <!-- 两步验证 (2FA / TOTP) 设置卡片 -->
        <section
          class="card settings-panel settings-panel-security"
          class:is-active={activeTab === 'security'}
          style="margin-top:var(--space-4);"
          aria-label="两步验证设置"
        >
          <div class="card-header" style="display:flex;align-items:center;justify-content:space-between;">
            <div style="display:flex;align-items:center;gap:var(--space-2);">
              <Icon name="shield" size={16} />
              <span class="card-title">两步验证（2FA / TOTP）</span>
            </div>
            <span class="badge {user.mfa_enabled ? 'badge-success' : 'badge-neutral'}">
              {user.mfa_enabled ? '已启用' : '未启用'}
            </span>
          </div>
          <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-4);">
            {#if user.mfa_enabled}
              <p class="text-secondary" style="margin:0;line-height:1.6;">
                你的账号已启用两步验证。登录时除密码外，还需要输入身份验证器（如 Google Authenticator、1Password 等）生成的 6 位动态验证码，保障账号安全。
              </p>
              <div style="display:flex;gap:var(--space-3);align-items:center;flex-wrap:wrap;">
                <a href="/mfa">
                  <Button text="管理两步验证与恢复码" variant="secondary" size="sm" />
                </a>
              </div>
            {:else}
              <p class="text-secondary" style="margin:0;line-height:1.6;">
                开启两步验证后，在输入密码后还需要输入手机认证器（Google Authenticator、1Password 等）生成的 6 位动态验证码，极大增强账户安全。
              </p>
              <div style="display:flex;gap:var(--space-3);align-items:center;flex-wrap:wrap;">
                <a href="/mfa">
                  <Button text="立即开启两步验证" variant="primary" size="sm" />
                </a>
              </div>
            {/if}
          </div>
        </section>

        <!-- Passkey（通行密钥）设置卡片 -->
        <section
          class="card settings-panel settings-panel-security"
          class:is-active={activeTab === 'security'}
          style="margin-top:var(--space-4);"
          aria-label="通行密钥设置"
        >
          <div class="card-header" style="display:flex;align-items:center;justify-content:space-between;">
            <div style="display:flex;align-items:center;gap:var(--space-2);">
              <Icon name="fingerprint" size={16} />
              <span class="card-title">Passkey（通行密钥）</span>
            </div>
            {#if passkeyEnabled}
              <span class="badge {passkeys.length > 0 ? 'badge-success' : 'badge-neutral'}">
                {passkeys.length > 0 ? `已绑定 ${passkeys.length} 把密钥` : '未绑定'}
              </span>
            {/if}
          </div>
          <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-4);">
            <p class="text-secondary" style="margin:0;line-height:1.6;">
              注册 Passkey 后，登录第二步可直接使用当前设备的指纹、Face ID 或屏幕锁通过验证，无需再输入 6 位动态验证码（与两步验证共存，任一方式均可完成第二步验证）。
            </p>

            {#if !passkeyEnabled}
              <div class="app-notice" role="status">
                <span>服务端尚未配置 Passkey（WebAuthn）支持，如需开启请联系管理员配置 passkey_rp_id。</span>
              </div>
            {:else}
              {#if passkeysError}
                <p class="input-hint is-error" role="alert" style="margin:0;">{passkeysError}</p>
              {/if}

              {#if passkeys.length > 0}
                <ul class="passkey-list" role="list">
                  {#each passkeys as item (item.id)}
                    <li class="passkey-item">
                      <div class="passkey-item__meta">
                        <strong>{item.name}</strong>
                        <span class="text-secondary">注册于 {formatPasskeyMs(item.created_at)}</span>
                        <span class="text-secondary">最近使用 {formatPasskeyMs(item.last_used_at)}</span>
                        {#if item.backed_up}
                          <span class="badge badge-success">已云同步</span>
                        {/if}
                      </div>
                      <form
                        method="POST"
                        action="?/passkeyRevoke"
                        use:enhance={() => {
                          return async ({ result, update }) => {
                            if (result.type === 'success') {
                              show('Passkey 已撤销', 'success');
                              await update();
                              await invalidateAll();
                            } else {
                              show('撤销 Passkey 失败，请重试', 'danger');
                              await update();
                            }
                          };
                        }}
                      >
                        <input type="hidden" name="id" value={item.id} />
                        <Button text="撤销" variant="ghost" size="sm" type="submit" />
                      </form>
                    </li>
                  {/each}
                </ul>
              {:else}
                <p class="text-secondary" style="margin:0;font-size:var(--text-sm);">尚未添加任何 Passkey。</p>
              {/if}

              <div class="passkey-add-row">
                <input
                  type="text"
                  class="input-field passkey-name-input"
                  placeholder="名称（可选，如「MacBook 指纹」）"
                  maxlength="64"
                  bind:value={passkeyName}
                  aria-label="Passkey 名称"
                />
                <Button
                  text={passkeyBusy ? '等待认证器…' : '添加 Passkey'}
                  variant="secondary"
                  size="sm"
                  onclick={addPasskey}
                  disabled={passkeyBusy}
                />
              </div>
            {/if}
          </div>
        </section>

        <!-- 通知偏好：类别 × 渠道矩阵（从 /notifications 迁入设置页）。
             桌面三列对齐（列头承载渠道名），移动端隐藏列头、渠道标签随行内显示。 -->
        <section
          id="settings-panel-notifications"
          class="card settings-panel settings-panel-notifications"
          class:is-active={activeTab === 'notifications'}
          aria-label="通知设置"
        >
          <div class="card-header">
            <div class="np-head-copy">
              <span class="card-title">通知偏好</span>
              <span class="np-subtitle">选择每类通知的接收渠道</span>
            </div>
          </div>
          <div class="np-body">
            {#if prefsError}
              <div class="app-notice is-danger" role="alert">
                <span>{prefsError}</span>
                <Button text="重新加载" variant="secondary" size="sm" onclick={() => void loadPrefs()} />
              </div>
            {/if}
            {#if prefsLoading}
              <div class="app-empty" role="status">正在加载通知偏好…</div>
            {:else}
            <div class="np-matrix">
              <div class="np-row np-row-head" aria-hidden="true">
                <span class="np-cat-head">类别</span>
                <div class="np-channels">
                  <span class="np-cell np-cell-head">邮件</span>
                  <span class="np-cell np-cell-head">站内</span>
                  <span class="np-cell np-cell-head">推送</span>
                </div>
              </div>
              {#each prefs as p}
                {@const label = categoryLabels[p.category] ?? p.category}
                <div class="np-row" role="group" aria-label={`${label} 通知偏好`}>
                  <div class="np-cat">
                    <span class="np-cat-name">{label}</span>
                    <span class="np-cat-desc">{categoryDescriptions[p.category] ?? ''}</span>
                  </div>
                  <div class="np-channels">
                    <label class="np-cell">
                      <input class="np-check" type="checkbox" checked={p.email_enabled} onchange={() => togglePref(p, 'email_enabled')} />
                      <span class="np-cell-label">邮件</span>
                    </label>
                    <label class="np-cell">
                      <input class="np-check" type="checkbox" checked={p.in_app_enabled} onchange={() => togglePref(p, 'in_app_enabled')} />
                      <span class="np-cell-label">站内</span>
                    </label>
                    <label class="np-cell">
                      <input class="np-check" type="checkbox" checked={p.push_enabled} onchange={() => togglePref(p, 'push_enabled')} />
                      <span class="np-cell-label">推送</span>
                    </label>
                  </div>
                </div>
              {/each}
            </div>
            {/if}
          </div>
          <div class="np-foot">
            <Icon name="shield" size={13} />
            <span>安全通知至少保留一个接收渠道</span>
          </div>
        </section>

        <!-- GAP-FIX OAuth 授权应用：GET /me/oauth-grants + 每行撤销
             （DangerConfirm 确认后提交隐藏表单）；接口失败时按空态降级。 -->
        <div id="settings-panel-oauth" class="card settings-panel settings-panel-oauth" role="tabpanel" class:is-active={activeTab === 'oauth'}>
          <div class="card-body">
            {#if revokeResult?.message}
              <p class="input-hint {revokeResult.ok ? '' : 'is-error'}" role="{revokeResult.ok ? 'status' : 'alert'}" style="margin-top:0;">{revokeResult.message}</p>
            {/if}
            {#if grants.length === 0}
              <EmptyState icon="key" title="暂无授权应用" desc="你使用本站账号登录的第三方应用会显示在这里" />
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
        <!-- 隐藏表单：专门用于头像和封面的自动即时持久化（通过 use:enhance 自动处理 Cookie/CSRF 与刷新） -->
        <form
          id="instant-avatar-form"
          method="POST"
          action="?/update-avatar"
          style="display:none;"
          use:enhance={() => {
            return async ({ result, update }) => {
              updatingAvatar = false;
              if (result.type === 'success') {
                show(currentAvatarId ? '头像已更新并生效' : '已恢复默认头像', 'success');
                await update();
                await invalidateAll();
              } else {
                show('头像更新失败，请重试', 'danger');
                await update();
              }
            };
          }}
        >
          <input type="hidden" name="version" value={user.version} />
          <input id="instant-avatar-input" type="hidden" name="avatar_attachment_id" value="" />
        </form>
      {:else if !error}
        <div class="empty-state"><div class="empty-state-title">加载中…</div></div>
      {/if}
    </div>
  </div>
</div>

<style>
  /* ---- 通知偏好矩阵（从 /notifications 迁入） ---- */
  .np-head-copy {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .np-subtitle {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
  }

  .np-body {
    padding: 0;
  }

  .np-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) repeat(3, 64px);
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-5);
    transition: background-color var(--duration-fast) var(--ease-out);
  }

  .np-row-head {
    padding-block: var(--space-2);
  }

  .np-row:not(.np-row-head) {
    border-top: var(--border-thin);
  }

  .np-row:not(.np-row-head):hover,
  .np-row:not(.np-row-head):focus-within {
    background: var(--color-bg-subtle);
  }

  .np-channels {
    display: contents;
  }

  .np-cell {
    position: relative;
    display: grid;
    place-items: center;
  }

  .np-cat-head,
  .np-cell-head {
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--color-text-tertiary);
    letter-spacing: 0.04em;
  }

  .np-cell-head {
    text-align: center;
  }

  .np-cat {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .np-cat-name {
    font-size: var(--text-base);
    font-weight: var(--weight-medium);
    color: var(--color-text-primary);
  }

  .np-cat-desc {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
  }

  .np-check {
    /* 隐藏原生 checkbox，用伪元素自绘以保证深/浅色模式下 on/off 区分度 */
    appearance: none;
    -webkit-appearance: none;
    width: 18px;
    height: 18px;
    margin: 0;
    border: 1.5px solid var(--color-border-strong, #555);
    border-radius: 3px;
    background: transparent;
    cursor: pointer;
    position: relative;
    transition: background-color 0.15s, border-color 0.15s;
  }

  .np-check:checked {
    background: var(--color-brand);
    border-color: var(--color-brand);
  }

  .np-check:checked::after {
    content: '';
    position: absolute;
    top: 2px;
    left: 5px;
    width: 5px;
    height: 9px;
    border: solid var(--on-brand, #fff);
    border-width: 0 2px 2px 0;
    transform: rotate(45deg);
  }

  .np-check:focus-visible {
    outline: 2px solid var(--color-focus-ring);
    outline-offset: 2px;
  }

  .np-cell-label {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: -1px;
    padding: 0;
    border: 0;
    overflow: hidden;
    clip: rect(0 0 0 0);
    clip-path: inset(50%);
    white-space: nowrap;
  }

  .np-foot {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-5);
    border-top: var(--border-default);
    font-size: var(--text-xs);
    color: var(--color-text-tertiary);
  }

  @media (max-width: 640px) {
    .np-row-head {
      display: none;
    }

    .np-row:not(.np-row-head) {
      grid-template-columns: 1fr;
      align-items: start;
      padding-block: var(--space-4);
    }

    .np-channels {
      display: flex;
      justify-content: flex-end;
      gap: var(--space-5);
    }

    .np-cell {
      display: inline-flex;
      align-items: center;
      gap: 6px;
    }

    .np-cell-label {
      position: static;
      width: auto;
      height: auto;
      margin: 0;
      overflow: visible;
      clip: auto;
      clip-path: none;
      white-space: normal;
      font-size: var(--text-sm);
      color: var(--color-text-secondary);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .np-row {
      transition: none;
    }
  }

  /* 设置侧栏 Lucide 图标——与文字基线对齐，颜色随 hover/active 切换 */
  :global(.app-settings-nav .app-settings-nav__icon) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    flex-shrink: 0;
    color: currentColor;
    opacity: 0.85;
    transition: opacity var(--duration-fast) var(--ease-out);
  }

  :global(.app-settings-nav button:hover .app-settings-nav__icon),
  :global(.app-settings-nav button.is-active .app-settings-nav__icon),
  :global(.app-settings-nav a:hover .app-settings-nav__icon),
  :global(.app-settings-nav a.is-active .app-settings-nav__icon) {
    opacity: 1;
  }

  /* M02-MFA-PK：Passkey 管理卡片 */
  .passkey-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .passkey-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    background: var(--color-bg-subtle);
    border-radius: var(--radius-sm);
  }

  .passkey-item__meta {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-1) var(--space-3);
    font-size: var(--text-sm);
  }

  .passkey-add-row {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    flex-wrap: wrap;
  }

  .passkey-name-input {
    flex: 1;
    min-width: 200px;
  }
</style>
