<script lang="ts">
  // M13-UI-03 & M18-ADMIN-THEMES：管理主题页
  // 支持：主题列表呈现、全站全局实时预览、UI 组件库效果展示、设为站点默认、Token 可视化编辑、预置主题一键安装、自定义上传与删除。
  import { onDestroy } from 'svelte';
  import { enhance } from '$app/forms';
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import { withActionToast, toastActionResult } from '$lib/ui/action-toast';
  import {
    THEME_TOKEN_KEYS,
    applyThemeTokens,
    previewThemeTokens,
    clearThemeTokens,
    fallbackDefaultTheme,
    resolveLayoutMode,
    LAYOUT_MODE_LABELS,
    type ActiveThemeView
  } from '$lib/theme/projection';
  import type { AdminThemesPageData, AdminThemesActionData, AdminThemeItem } from './+page.server';

  let { data, form }: { data: AdminThemesPageData; form?: AdminThemesActionData | null } = $props();

  const pageState = $derived(data.state);
  const rawThemes = $derived(data.themes ?? []);
  const conflict = $derived(form?.conflict === true);

  // JS 启用：动作结果走全局 Toast 浮窗（成功绿/失败红）；顶部内联横幅仅保留为
  // 无 JS 回退（SSR HTML 仍渲染，见 hasJs）。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  // 官方高质量预置主题包（收录原版官方默认配色与各风格主题）。
  // 全部 6 个官方包均为**日/夜双模式**：日间 6 色 + 夜间 6 色（color.*.dark
  // 可选变体，封闭 schema v1.1）。站点亮色模式取日间板，暗色模式（html.dark）
  // 取夜间板；两者由 theme-tokens.css 按模式自动解析，管理员无需手动切换。
  const PRESET_THEMES = [
    {
      name: 'bblbb-classic',
      display_name: 'BBLBB 经典赤墨 (原版默认)',
      desc: 'BBLBB 官方原生品牌视觉：日间暖珊瑚红×米白宣纸，夜间墨绿×珊瑚橙',
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
        'color.border.dark': '#30433e',
        'font.body': 'system-ui',
        'font.mono': 'ui-monospace',
        'radius.control': '0.375rem',
        'radius.card': '0.5rem',
        'space.density': 'comfortable',
        'layout.mode': 'classic',
        'shadow.card': 'sm',
        'motion.duration': '150ms',
        'motion.reduced': false
      }
    },
    {
      name: 'chinese-elegance',
      display_name: '水墨青石 (中国风)',
      desc: '日间宣纸水墨×青石灰蓝，夜间宿墨玄青×月白书卷，双模式俱雅',
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
        'color.border.dark': '#2e343b',
        'font.body': 'Noto Sans SC',
        'font.mono': 'monospace',
        'radius.control': '0.125rem',
        'radius.card': '0.125rem',
        'space.density': 'comfortable',
        'layout.mode': 'sidebar',
        'shadow.card': 'none',
        'motion.duration': '150ms',
        'motion.reduced': false
      }
    },
    {
      name: 'midnight',
      display_name: '暗夜极光',
      desc: '日间极昼浅蓝×晴空青，夜间深蓝灰×天蓝极光，昼夜皆沉浸',
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
        'color.border.dark': '#334155',
        'font.body': 'system-ui',
        'font.mono': 'ui-monospace',
        'radius.control': '0.5rem',
        'radius.card': '0.75rem',
        'space.density': 'comfortable',
        'layout.mode': 'sidebar',
        'shadow.card': 'md',
        'motion.duration': '150ms',
        'motion.reduced': false
      }
    },
    {
      name: 'paper',
      display_name: '复古羊皮纸',
      desc: '日间暖羊皮×朱砂红，夜间灯火书斋×琥珀暖调，昼夜皆书卷',
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
        'color.border.dark': '#3a342a',
        'font.body': 'serif',
        'font.mono': 'monospace',
        'radius.control': '0.25rem',
        'radius.card': '0.5rem',
        'space.density': 'comfortable',
        'layout.mode': 'wide',
        'shadow.card': 'sm',
        'motion.duration': '150ms',
        'motion.reduced': false
      }
    },
    {
      name: 'forest',
      display_name: '翡翠森林',
      desc: '日间薄荷浅林×墨绿，夜间深林夜色×翡翠荧光，昼夜皆清爽',
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
        'color.border.dark': '#27392f',
        'font.body': 'sans-serif',
        'font.mono': 'ui-monospace',
        'radius.control': '0.5rem',
        'radius.card': '0.75rem',
        'space.density': 'comfortable',
        'layout.mode': 'wide',
        'shadow.card': 'sm',
        'motion.duration': '150ms',
        'motion.reduced': false
      }
    },
    {
      name: 'cyberpunk',
      display_name: '赛博霓虹',
      desc: '日间雾紫纸面×热粉霓虹，夜间深紫暗夜×粉紫霓虹，昼夜皆潮酷',
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
        'color.border.dark': '#3b2d56',
        'font.body': 'system-ui',
        'font.mono': 'ui-monospace',
        'radius.control': '0.5rem',
        'radius.card': '0.75rem',
        'space.density': 'compact',
        'layout.mode': 'sidebar',
        'shadow.card': 'lg',
        'motion.duration': '100ms',
        'motion.reduced': false
      }
    }
  ];

  const DARK_COLOR_KEYS = [
    'color.background.dark',
    'color.surface.dark',
    'color.text.dark',
    'color.muted.dark',
    'color.accent.dark',
    'color.border.dark'
  ] as const;

  // 官方夜间基线（新主题补全夜间色板时的预填值，来自内置 default 夜板）
  const NIGHT_PALETTE_DEFAULT: Record<string, string> = {
    'color.background.dark': '#101b19',
    'color.surface.dark': '#172522',
    'color.text.dark': '#f5f3ea',
    'color.muted.dark': '#b5c0ba',
    'color.accent.dark': '#f27759',
    'color.border.dark': '#30433e'
  };

  const FONT_BODY_OPTIONS = [
    'system-ui',
    'sans-serif',
    'serif',
    'PingFang SC',
    'Microsoft YaHei',
    'Noto Sans SC',
    'Noto Serif SC',
    'Georgia',
    'Times New Roman'
  ];

  const FONT_MONO_OPTIONS = [
    'ui-monospace',
    'monospace',
    'Courier New'
  ];

  // 格式化安全 Token 为 JSON（仅包含封闭白名单 key，屏蔽内部 secret）
  function tokensJson(tokens: Record<string, unknown> | null | undefined): string {
    if (!tokens) return '{}';
    const picked: Record<string, unknown> = {};
    for (const key of THEME_TOKEN_KEYS) {
      if (key in tokens) picked[key] = tokens[key];
    }
    return JSON.stringify(picked, null, 2);
  }

  // 计算安全的预览背景渐变
  function safeBg(tokens: Record<string, unknown> | null | undefined, name: string): string {
    const bg = typeof tokens?.['color.background'] === 'string' && /^#[0-9a-fA-F]{3,8}$/.test(tokens['color.background'])
      ? tokens['color.background']
      : (name === 'midnight' ? '#0f172a' : '#f5f3ed');
    const accent = typeof tokens?.['color.accent'] === 'string' && /^#[0-9a-fA-F]{3,8}$/.test(tokens['color.accent'])
      ? tokens['color.accent']
      : '#b23e2a';
    const surface = typeof tokens?.['color.surface'] === 'string' && /^#[0-9a-fA-F]{3,8}$/.test(tokens['color.surface'])
      ? tokens['color.surface']
      : '#ffffff';
    return `linear-gradient(135deg, ${bg} 0%, ${accent} 50%, ${surface} 100%)`;
  }

  // 校验安全 Hex 色值
  function safeColor(val: unknown, fallback: string): string {
    return typeof val === 'string' && /^#[0-9a-fA-F]{3,8}$/.test(val) ? val : fallback;
  }

  // 主题是否携带完整夜间色板（v1.1 日/夜双模式；6 个 .dark key 全部合法）
  function hasNightPalette(tokens: Record<string, unknown> | null | undefined): boolean {
    return DARK_COLOR_KEYS.every(
      (k) => typeof tokens?.[k] === 'string' && /^#[0-9a-fA-F]{3,8}$/.test(tokens[k] as string)
    );
  }

  // 日/夜双模式预览横幅：左侧日间板 → 右侧夜间板，中间以品牌强调色作昼夜分界；
  // 无夜间色板的旧主题回退原单色渐变（保持既有视觉）。
  function dualBg(tokens: Record<string, unknown> | null | undefined, name: string): string {
    const day = safeColor(
      tokens?.['color.background'],
      name === 'midnight' || name === 'cyberpunk' ? '#0f172a' : '#f5f3ed'
    );
    const nightRaw = tokens?.['color.background.dark'];
    const night =
      typeof nightRaw === 'string' && /^#[0-9a-fA-F]{3,8}$/.test(nightRaw) ? nightRaw : null;
    if (!night) return safeBg(tokens, name);
    const accent =
      typeof tokens?.['color.accent'] === 'string' && /^#[0-9a-fA-F]{3,8}$/.test(tokens['color.accent'])
        ? (tokens['color.accent'] as string)
        : '#b23e2a';
    return `linear-gradient(105deg, ${day} 0%, ${day} 40%, ${accent} 49%, ${accent} 51%, ${night} 60%, ${night} 100%)`;
  }

  function toThemeView(theme: AdminThemeItem): ActiveThemeView {
    return {
      name: theme.name,
      revision: theme.revision,
      tokens: theme.tokens,
      source: theme.is_default ? 'site_default' : 'user_preference'
    };
  }

  // 当前全局预览的主题
  let previewTheme = $state<AdminThemeItem | null>(null);

  // 弹窗状态
  let showcaseOpen = $state(false);
  let editTheme = $state<AdminThemeItem | null>(null);
  let editTokens = $state<Record<string, unknown>>({});
  let editJsonMode = $state(false);
  let editTokensRaw = $state('');
  // Token 编辑器色板模式：day（6 色基线）/ night（color.*.dark 夜间变体）
  let editColorMode = $state<'day' | 'night'>('day');

  // 当前色板模式下 6 个颜色字段的实际 token key
  function colorKey(base: string): string {
    return editColorMode === 'night' ? `${base}.dark` : base;
  }

  // 切换色板模式；切到夜间且缺夜间色板时用官方夜间基线预填（可继续调整）
  function setEditColorMode(mode: 'day' | 'night') {
    editColorMode = mode;
    if (mode === 'night') {
      for (const k of DARK_COLOR_KEYS) {
        if (typeof editTokens[k] !== 'string') editTokens[k] = NIGHT_PALETTE_DEFAULT[k];
      }
    }
  }

  let setDefaultTheme = $state<AdminThemeItem | null>(null);
  let deleteTargetTheme = $state<AdminThemeItem | null>(null);
  let previewPending = $state<string | null>(null);

  // 激活全局实时预览
  function startPreview(theme: AdminThemeItem) {
    if (previewPending === theme.name) return;
    previewPending = theme.name;
    previewTheme = theme;
    const view: ActiveThemeView = {
      name: theme.name,
      revision: theme.revision,
      tokens: theme.tokens,
      source: 'user_preference'
    };
    try {
      previewThemeTokens(view);
      showToast(`已开启主题「${theme.display_name}」全局实时预览`, 'info');
    } finally {
      previewPending = null;
    }
  }

  // 退出全局预览
  function stopPreview() {
    previewTheme = null;
    clearThemeTokens();
    if (activeTheme && activeTheme.name !== 'default') {
      applyThemeTokens(toThemeView(activeTheme));
    }
    showToast('已退出主题预览，恢复当前默认', 'info');
  }

  // 打开编辑弹窗
  function openEditModal(theme: AdminThemeItem) {
    editTheme = theme;
    editTokens = { ...(theme.tokens ?? {}) };
    // 结构预设缺省补全：旧主题（v1 schema 无 layout.mode）编辑时默认 classic，
    // 避免保存后静默丢失结构声明
    if (!editTokens['layout.mode']) editTokens['layout.mode'] = 'classic';
    editTokensRaw = tokensJson(editTokens);
    editJsonMode = false;
    editColorMode = 'day';
  }

  function closeEditModal() {
    editTheme = null;
  }

  // 页面销毁时恢复当前站点默认主题（而非仅清空）：
  // 根 layout 复用时 activeTheme 数据不变不会重跑 effect，只 clear 会把
  // 正式主题（含结构布局）冲掉后不恢复——离开预览必须显式回放已生效主题。
  onDestroy(() => {
    if (previewTheme && activeTheme) {
      applyThemeTokens(toThemeView(activeTheme));
    } else if (previewTheme) {
      clearThemeTokens();
    }
    previewTheme = null;
  });

  const activeTheme = $derived(rawThemes.find((t) => t.is_default && t.status === 'active') ?? rawThemes[0]);
</script>

<svelte:head>
  <title>主题管理 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="主题管理" />

{#if pageState === 'forbidden'}
  <div class="app-card">
    <div class="app-card__body" role="alert">
      <p class="input-hint is-error">无权限（需要 theme.manage 权限）。</p>
    </div>
  </div>
{:else}
  {#if conflict}
    <div class="app-error" role="alert" style="margin-bottom:14px;padding:12px 16px;background:var(--color-bg-subtle);border-radius:var(--radius-md);border-left:4px solid var(--color-danger);display:flex;align-items:center;justify-content:space-between;">
      <span>版本已变化，请刷新页面后重试（revision 乐观锁冲突）。</span>
      <button type="button" class="btn sm ghost" onclick={() => window.location.reload()}>刷新页面</button>
    </div>
  {/if}

  {#if form?.message && !conflict && !hasJs}
    <div class="app-success" role="status" style="margin-bottom:14px;padding:10px 14px;background:var(--color-success-soft);border-radius:var(--radius-md);border-left:3px solid var(--color-success);font-size:13px;">
      {form.message}
    </div>
  {/if}

  <!-- 全局预览提示浮条 -->
  {#if previewTheme}
    <aside
      class="app-card"
      style="margin-bottom:16px;border:2px solid var(--color-brand);background:var(--color-bg-card);padding:12px 16px;box-shadow:var(--shadow-pop);display:flex;align-items:center;justify-content:space-between;gap:12px;flex-wrap:wrap;border-radius:var(--radius-md);"
      aria-label="全局预览状态"
    >
      <div style="display:flex;align-items:center;gap:10px;">
        <span style="display:inline-block;width:12px;height:12px;border-radius:50%;background:var(--color-brand);animation:pulse 1.5s infinite;"></span>
        <div>
          <strong style="font-size:14px;">正在全局实时预览：「{previewTheme.display_name}」</strong>
          <span class="text-secondary" style="font-size:12px;margin-left:6px;">(代号: /{previewTheme.name} · v{previewTheme.revision})</span>
          <span class="sbadge sb-primary" style="font-size:10px;margin-left:6px;" title="主题声明的页面结构预设（layout.mode），当前已实时应用">
            结构：{LAYOUT_MODE_LABELS[resolveLayoutMode(previewTheme.tokens)]}
          </span>
        </div>
      </div>
      <div style="display:flex;align-items:center;gap:8px;">
        <button type="button" class="btn sm secondary" onclick={() => (showcaseOpen = true)}>
          查看组件效果库
        </button>
        {#if !previewTheme.is_default}
          <button type="button" class="btn sm primary" onclick={() => (setDefaultTheme = previewTheme)}>
            设为站点默认
          </button>
        {/if}
        <button type="button" class="btn sm ghost" onclick={stopPreview}>
          退出预览
        </button>
      </div>
    </aside>
  {/if}

  <!-- 卡片 1：当前生效主题 -->
  <section class="app-card" style="margin-bottom:16px;">
    <header class="app-card__head" style="display:flex;align-items:center;justify-content:space-between;">
      <h2>当前主题</h2>
      {#if activeTheme}
        <span class="sbadge sb-success">
          {activeTheme.is_default ? '站点默认生效中' : '内置回退中'}
        </span>
      {/if}
    </header>
    <div class="app-card__body">
      {#if activeTheme}
        <div
          style="height:88px;border-radius:var(--radius-md);background:{dualBg(activeTheme.tokens, activeTheme.name)};margin-bottom:12px;box-shadow:inset 0 0 0 1px rgba(255,255,255,0.2);display:flex;align-items:flex-end;padding:12px;"
        >
          <span class="theme-hero-title">
            {activeTheme.display_name}
          </span>
        </div>
        <div style="display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:8px;">
          <div class="text-secondary" style="font-size:13px;">
            代号：<code>/{activeTheme.name}</code> · 修订版本：<code>v{activeTheme.revision}</code> · 亮/暗模式均自动兼容 ·
            页面结构：<span class="sbadge sb-primary" style="font-size:10px;">{LAYOUT_MODE_LABELS[resolveLayoutMode(activeTheme.tokens)]}</span>
          </div>
          <div style="display:flex;gap:6px;">
            <button type="button" class="btn sm secondary" onclick={() => startPreview(activeTheme)}>
              实时预览
            </button>
            <button type="button" class="btn sm secondary" onclick={() => openEditModal(activeTheme)}>
              编辑 Token
            </button>
          </div>
        </div>
      {:else}
        <p class="text-secondary">暂无生效主题</p>
      {/if}
    </div>
  </section>

  <!-- 卡片 2：已安装主题列表 -->
  <section class="app-card" style="margin-bottom:16px;">
    <header class="app-card__head" style="display:flex;align-items:center;justify-content:space-between;">
      <div>
        <h2>已安装主题列表</h2>
        <p class="text-secondary" style="font-size:12px;margin:2px 0 0 0;">共 {rawThemes.length} 个主题，数据型主题均通过封闭 Token Schema 安全沙箱隔离。</p>
      </div>
      <button type="button" class="btn sm secondary" onclick={() => (showcaseOpen = true)}>
        打开组件库预览
      </button>
    </header>
    <div class="app-card__body">
      <div style="display:grid;grid-template-columns:repeat(auto-fill, minmax(240px, 1fr));gap:16px;">
        {#each rawThemes as theme (theme.name)}
          <div
            class="app-card"
            style="border:1px solid var(--color-border);border-radius:var(--radius-md);padding:14px;display:flex;flex-direction:column;gap:10px;background:var(--color-bg-card);transition:box-shadow 0.2s;"
          >
            <!-- 色彩预览横幅 -->
            <div
              style="height:68px;border-radius:var(--radius-sm);background:{dualBg(theme.tokens, theme.name)};position:relative;"
            >
              {#if theme.is_default}
                <span class="sbadge sb-success" style="position:absolute;top:6px;right:6px;box-shadow:0 1px 2px rgba(0,0,0,0.2);">站点默认</span>
              {:else if theme.status === 'disabled'}
                <span class="sbadge sb-gray" style="position:absolute;top:6px;right:6px;">隔离（disabled）</span>
              {/if}
            </div>

            <!-- 主题标题与标识 -->
            <div>
              <div style="display:flex;align-items:center;justify-content:space-between;gap:6px;">
                <strong style="font-size:15px;">{theme.display_name}</strong>
                <span style="display:flex;align-items:center;gap:4px;">
                  {#if hasNightPalette(theme.tokens)}
                    <span class="sbadge sb-success" title="已配置日间与夜间双模式色板，随站点亮/暗模式自动切换" style="font-size:10px;">日/夜</span>
                  {:else}
                    <span class="sbadge sb-gray" title="仅日间色板：暗色模式下回退使用日间色值，可编辑补全夜间色板" style="font-size:10px;">仅日间</span>
                  {/if}
                  <span class="sbadge sb-primary" title={`页面结构：${LAYOUT_MODE_LABELS[resolveLayoutMode(theme.tokens)]}`} style="font-size:10px;">{LAYOUT_MODE_LABELS[resolveLayoutMode(theme.tokens)]}</span>
                  <span class="text-secondary" style="font-size:11px;">revision v{theme.revision}</span>
                </span>
              </div>
              <code class="theme-id-code">/{theme.name}</code>
            </div>

            <!-- 核心色彩色板微缩展示 -->
            <div style="display:flex;align-items:center;gap:6px;padding:6px 0;border-top:1px dashed var(--color-border);border-bottom:1px dashed var(--color-border);">
              <span class="text-secondary" style="font-size:11px;">色板：</span>
              <span title="背景色" class="palette-swatch" style="--swatch:{safeColor(theme.tokens?.['color.background'], '#ffffff')};"></span>
              <span title="卡片色" class="palette-swatch" style="--swatch:{safeColor(theme.tokens?.['color.surface'], '#ffffff')};"></span>
              <span title="文字色" class="palette-swatch" style="--swatch:{safeColor(theme.tokens?.['color.text'], '#000000')};"></span>
              <span title="强调色" class="palette-swatch" style="--swatch:{safeColor(theme.tokens?.['color.accent'], '#2563eb')};"></span>
              <span title="边框色" class="palette-swatch" style="--swatch:{safeColor(theme.tokens?.['color.border'], '#e5e7eb')};"></span>
            </div>

            <!-- 操作按钮组 -->
            <div style="display:flex;align-items:center;justify-content:space-between;margin-top:auto;padding-top:6px;gap:6px;flex-wrap:wrap;">
              <div style="display:flex;gap:6px;">
                <button
                  type="button"
                  class="btn sm {previewTheme?.name === theme.name ? 'primary' : 'ghost'}"
                  disabled={previewPending === theme.name}
                  onclick={() => startPreview(theme)}
                >
                  {previewPending === theme.name ? '加载预览…' : previewTheme?.name === theme.name ? '预览中' : '预览'}
                </button>
                <button
                  type="button"
                  class="btn sm secondary"
                  onclick={() => openEditModal(theme)}
                >
                  编辑
                </button>
              </div>

              <div style="display:flex;gap:6px;">
                {#if theme.is_default}
                  <button type="button" class="btn sm ghost" disabled>
                    已是默认
                  </button>
                {:else}
                  <button
                    type="button"
                    class="btn sm primary"
                    onclick={() => (setDefaultTheme = theme)}
                  >
                    设为默认
                  </button>
                {/if}

                {#if theme.name !== 'default' && !theme.is_default}
                  <button
                    type="button"
                    class="btn sm danger"
                    onclick={() => (deleteTargetTheme = theme)}
                  >
                    删除
                  </button>
                {/if}
              </div>
            </div>
          </div>
        {/each}
      </div>
    </div>
  </section>

  <!-- 卡片 3：官方预置主题包（一键安装） -->
  <section class="app-card" style="margin-bottom:16px;">
    <header class="app-card__head">
      <h2>官方预置主题包</h2>
      <p class="text-secondary" style="font-size:12px;margin:2px 0 0 0;">开箱即用经过安全审计的官方预置主题包，点击即可一键上传安装至系统。</p>
    </header>
    <div class="app-card__body">
      <div style="display:grid;grid-template-columns:repeat(auto-fill, minmax(220px, 1fr));gap:14px;">
        {#each PRESET_THEMES as preset (preset.name)}
          {@const installed = rawThemes.some((t) => t.name === preset.name)}
          <div
            class="app-card"
            style="border:1px solid var(--color-border);border-radius:var(--radius-md);padding:12px;display:flex;flex-direction:column;gap:8px;"
          >
            <div
              style="height:52px;border-radius:var(--radius-sm);background:{dualBg(preset.tokens, preset.name)};display:flex;align-items:flex-end;justify-content:space-between;padding:4px 8px;"
              title="左：日间模式 · 右：夜间模式（随站点亮/暗切换自动解析）"
            >
              <span style="font-size:9px;color:#fff;text-shadow:0 1px 2px rgba(0,0,0,0.55);">日间</span>
              <span style="font-size:9px;color:#fff;text-shadow:0 1px 2px rgba(0,0,0,0.55);">夜间</span>
            </div>
            <div>
              <div style="display:flex;align-items:center;justify-content:space-between;gap:6px;">
                <strong>{preset.display_name}</strong>
                <span style="display:flex;align-items:center;gap:4px;">
                  <span class="sbadge sb-success" style="font-size:10px;" title="内置日间+夜间双模式色板">日/夜</span>
                  <span class="sbadge sb-primary" style="font-size:10px;" title="页面结构预设（layout.mode）">{LAYOUT_MODE_LABELS[resolveLayoutMode(preset.tokens)]}</span>
                </span>
              </div>
              <code class="preset-code">/{preset.name}</code>
              <p class="text-secondary" style="font-size:12px;margin:4px 0 0 0;line-height:1.3;">{preset.desc}</p>
            </div>
            <div style="display:flex;align-items:center;justify-content:space-between;margin-top:auto;padding-top:6px;">
              <button
                type="button"
                class="text-link"
                style="font-size:12px;background:none;border:none;cursor:pointer;padding:0;"
                onclick={() => {
                  startPreview({
                    name: preset.name,
                    display_name: preset.display_name,
                    kind: 'data',
                    schema_version: 1,
                    version: '1.0.0',
                    supports: '>=1.0 <2.0',
                    status: 'active',
                    is_default: false,
                    revision: 1,
                    tokens: preset.tokens,
                    created_by: 'preset',
                    updated_at: 0
                  });
                }}
              >
                预览效果
              </button>

              {#if installed}
                <span class="sbadge sb-success" style="font-size:11px;">已安装</span>
              {:else}
                <form method="POST" action="?/upload" use:enhance={withActionToast()}>
                  <input type="hidden" name="name" value={preset.name} />
                  <input type="hidden" name="display_name" value={preset.display_name} />
                  <input type="hidden" name="tokens_json" value={JSON.stringify(preset.tokens)} />
                  <input type="hidden" name="reason" value={`安装官方预置主题：${preset.display_name}`} />
                  <button type="submit" class="btn sm secondary">
                    一键安装
                  </button>
                </form>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    </div>
  </section>

  <!-- 卡片 4：自定义上传主题与底层 Token 契约表单（包含 SSR 测试断言契约） -->
  <details class="app-card" style="margin-bottom:16px;">
    <summary class="app-card__head" style="cursor:pointer;user-select:none;">
      <h2 style="display:inline-block;font-size:15px;margin:0;">上传新主题与高级 Token 维护</h2>
      <span class="text-secondary" style="font-size:12px;margin-left:8px;">（支持导入/编辑 JSON Token 数据包与审计日志）</span>
    </summary>
    <div class="app-card__body" style="padding-top:14px;">
      <h3 style="font-size:14px;margin-bottom:8px;">上传自定义数据型主题</h3>
      <form method="POST" action="?/upload" use:enhance={withActionToast()} class="stack" style="gap:12px;max-width:600px;">
        <label>
          <span class="field-label">主题代号（name）</span>
          <input type="text" name="name" class="input-field" placeholder="例如：my-dark-theme（仅限小写字母/数字/连字符）" pattern="[a-z0-9-]{'{'}1,64{'}'}" required />
        </label>
        <label>
          <span class="field-label">显示名称（display_name）</span>
          <input type="text" name="display_name" class="input-field" placeholder="例如：极客黑金" />
        </label>
        <label>
          <span class="field-label">Token 配置 JSON</span>
          <textarea name="tokens_json" class="input-field" rows="5" placeholder={`{ "color.background": "#f5f3ed", "color.surface": "#fffefb", "color.accent": "#b23e2a", "color.background.dark": "#101b19", "color.accent.dark": "#f27759", ... }`}>{tokensJson(fallbackDefaultTheme().tokens)}</textarea>
        </label>
        <label>
          <span class="field-label">操作原因（写审计日志，必填）</span>
          <input type="text" name="reason" id="settings-reason" class="input-field" required placeholder="必填：如上传新版社区定制暗色主题" />
        </label>
        <Button text="上传主题" variant="primary" type="submit" />
      </form>

      <hr style="margin:20px 0;border:0;border-top:1px solid var(--color-border);" />

      <h3 style="font-size:14px;margin-bottom:8px;">各主题底层 Token 契约设置（带 revision 乐观锁）</h3>
      {#each rawThemes as t (t.name)}
        <div style="margin-top:14px;padding:12px;border:1px solid var(--color-border);border-radius:var(--radius-sm);background:var(--color-bg-subtle);">
          <strong style="font-size:13px;">{t.display_name} (/{t.name}) — revision v{t.revision}</strong>
          <form method="POST" action="?/save-settings" use:enhance={withActionToast()} class="stack" style="gap:10px;margin-top:8px;">
            <input type="hidden" name="name" value={t.name} />
            <input type="hidden" name="revision" value={t.revision} />
            <textarea name="tokens" class="input-field" rows="4">{tokensJson(t.tokens)}</textarea>
            <input type="text" name="reason" class="input-field" required placeholder="修改原因（写审计）" />
            <Button text="保存 Token 设置" variant="secondary" type="submit" />
          </form>
        </div>
      {/each}
    </div>
  </details>
{/if}

<!-- 弹窗 1：全套 UI 组件库效果展示 Showcase -->
{#if showcaseOpen}
  <div class="modal-backdrop" style="position:fixed;inset:0;background:rgba(0,0,0,0.55);z-index:999;display:flex;align-items:center;justify-content:center;padding:16px;">
    <div
      class="app-card theme-preview-scope"
      role="dialog"
      tabindex="-1"
      aria-labelledby="showcase-title"
      onkeydown={(e) => { if (e.key === 'Escape') showcaseOpen = false; }}
      style="max-width:840px;width:100%;max-height:90vh;overflow-y:auto;border-radius:var(--radius-md);box-shadow:var(--shadow-modal);background:var(--color-bg-card);"
    >
      <header class="app-card__head" style="display:flex;align-items:center;justify-content:space-between;position:sticky;top:0;background:var(--color-bg-card);z-index:2;border-bottom:1px solid var(--color-border);">
        <h2 id="showcase-title" style="margin:0;font-size:16px;">
          主题组件库实时效果展示 {previewTheme ? `—「${previewTheme.display_name}」` : ''}
        </h2>
        <button type="button" class="btn sm ghost" onclick={() => (showcaseOpen = false)}>关闭</button>
      </header>
      <div class="app-card__body" style="display:flex;flex-direction:column;gap:18px;">
        <!-- 模拟导航栏 -->
        <div>
          <span class="field-label">模拟导航栏与品牌色</span>
          <div style="background:var(--color-bg-page);border:1px solid var(--color-border);padding:10px 16px;border-radius:var(--radius-md);display:flex;align-items:center;justify-content:space-between;">
            <div style="display:flex;align-items:center;gap:10px;">
              <span class="mock-brand">BBLBB Community</span>
              <span class="sbadge sb-primary">Beta</span>
            </div>
            <div style="display:flex;gap:8px;">
              <span class="text-link" style="font-size:13px;">广场</span>
              <span class="text-link" style="font-size:13px;">探索</span>
              <span class="text-link" style="font-size:13px;">关于</span>
            </div>
          </div>
        </div>

        <!-- 模拟帖子卡片 -->
        <div>
          <span class="field-label">模拟论坛帖子卡片</span>
          <div class="app-card" style="background:var(--color-bg-card);border:1px solid var(--color-border);border-radius:var(--radius-md);padding:14px;">
            <div style="display:flex;align-items:center;gap:8px;margin-bottom:8px;">
              <span class="mock-avatar">BB</span>
              <div>
                <strong class="mock-author">论坛官方</strong>
                <span class="text-secondary" style="font-size:11px;margin-left:6px;">10 分钟前</span>
              </div>
              <span class="sbadge sb-success" style="margin-left:auto;">推荐置顶</span>
            </div>
            <h3 class="mock-title">深入理解数据型主题架构与安全 Token 隔离</h3>
            <p class="mock-desc">
              BBLBB 采用封闭式 Token Schema，只允许修改安全的颜色、圆角、字体与空间密度，杜绝任何外部脚本注入。
            </p>
            <div style="display:flex;gap:6px;">
              <span class="sbadge sb-gray">#架构</span>
              <span class="sbadge sb-gray">#主题</span>
              <span class="sbadge sb-gray">#安全</span>
            </div>
          </div>
        </div>

        <!-- 模拟按钮与控件 -->
        <div>
          <span class="field-label">按钮与状态组件</span>
          <div style="display:flex;gap:8px;flex-wrap:wrap;align-items:center;">
            <button type="button" class="btn sm primary">Primary 主要按钮</button>
            <button type="button" class="btn sm secondary">Secondary 次要按钮</button>
            <button type="button" class="btn sm ghost">Ghost 幽灵按钮</button>
            <button type="button" class="btn sm danger">Danger 危险操作</button>
            <button type="button" class="btn sm" disabled>Disabled 禁用态</button>
          </div>
        </div>

        <!-- 模拟输入表单 -->
        <div>
          <span class="field-label">表单输入项</span>
          <div style="display:flex;gap:10px;">
            <input type="text" class="input-field" placeholder="输入搜索关键词..." value="主题预览测试" style="flex:1;" />
            <button type="button" class="btn primary">搜索</button>
          </div>
        </div>

        <!-- 模拟代码排版 -->
        <div>
          <span class="field-label">等宽字体与代码块排版</span>
          <pre class="mock-code-block"><code>const activeTheme = resolveTheme();
console.log(`Current theme revision: v${'{'}activeTheme.revision{'}'}`);</code></pre>
        </div>
      </div>
      <footer class="app-card__head" style="display:flex;justify-content:flex-end;gap:8px;border-top:1px solid var(--color-border);">
        <button type="button" class="btn secondary" onclick={() => (showcaseOpen = false)}>完成预览</button>
      </footer>
    </div>
  </div>
{/if}

<!-- 弹窗 2：Token 可视化与高级编辑器 Modal -->
{#if editTheme}
  <div class="modal-backdrop" style="position:fixed;inset:0;background:rgba(0,0,0,0.55);z-index:999;display:flex;align-items:center;justify-content:center;padding:16px;">
    <div
      class="app-card"
      role="dialog"
      tabindex="-1"
      aria-labelledby="edit-title"
      onkeydown={(e) => { if (e.key === 'Escape') closeEditModal(); }}
      style="max-width:760px;width:100%;max-height:90vh;overflow-y:auto;border-radius:var(--radius-md);box-shadow:var(--shadow-modal);background:var(--color-bg-card);"
    >
      <header class="app-card__head" style="display:flex;align-items:center;justify-content:space-between;border-bottom:1px solid var(--color-border);">
        <div>
          <h2 id="edit-title" style="margin:0;font-size:16px;">编辑 Token 设置 — {editTheme.display_name}</h2>
          <span class="text-secondary" style="font-size:12px;">代号：/{editTheme.name} · 当前 revision：v{editTheme.revision}</span>
        </div>
        <div style="display:flex;gap:8px;">
          <button
            type="button"
            class="btn sm {editJsonMode ? 'ghost' : 'secondary'}"
            onclick={() => {
              if (editJsonMode) {
                try {
                  editTokens = JSON.parse(editTokensRaw);
                  editJsonMode = false;
                } catch {
                  showToast('JSON 格式错误，请检查', 'danger');
                }
              } else {
                editTokensRaw = tokensJson(editTokens);
                editJsonMode = true;
              }
            }}
          >
            {editJsonMode ? '切换可视化表单' : '切换 JSON 源码模式'}
          </button>
          <button type="button" class="btn sm ghost" onclick={closeEditModal}>关闭</button>
        </div>
      </header>

      <form method="POST" action="?/save-settings" use:enhance={withActionToast()} class="app-card__body" style="display:flex;flex-direction:column;gap:14px;">
        <input type="hidden" name="name" value={editTheme.name} />
        <input type="hidden" name="revision" value={editTheme.revision} />

        {#if editJsonMode}
          <label>
            <span class="field-label">Token JSON 文本</span>
            <textarea
              name="tokens"
              bind:value={editTokensRaw}
              class="input-field"
              rows="12"
              style="font-family:var(--font-family-mono);font-size:12px;"
            ></textarea>
          </label>
        {:else}
          <!-- 色板模式切换：日间（color.*）/ 夜间（color.*.dark，v1.1 双模式） -->
          <div style="display:flex;align-items:center;gap:8px;flex-wrap:wrap;margin-bottom:10px;">
            <span class="field-label" style="margin:0;">色板模式</span>
            <span style="display:inline-flex;border:1px solid var(--color-border-strong);border-radius:var(--radius-sm);overflow:hidden;">
              <button
                type="button"
                onclick={() => setEditColorMode('day')}
                style="padding:5px 14px;font-size:12px;border:0;cursor:pointer;background:{editColorMode === 'day' ? 'var(--color-brand)' : 'transparent'};color:{editColorMode === 'day' ? 'var(--color-text-on-brand)' : 'var(--color-text-secondary)'};transition:background 0.15s,color 0.15s;"
              >
                日间色板
              </button>
              <button
                type="button"
                onclick={() => setEditColorMode('night')}
                style="padding:5px 14px;font-size:12px;border:0;cursor:pointer;background:{editColorMode === 'night' ? 'var(--color-brand)' : 'transparent'};color:{editColorMode === 'night' ? 'var(--color-text-on-brand)' : 'var(--color-text-secondary)'};transition:background 0.15s,color 0.15s;"
              >
                夜间色板
              </button>
            </span>
            <span class="text-secondary" style="font-size:11px;">
              {editColorMode === 'day'
                ? '编辑日间（亮色模式）色板 color.*'
                : '编辑夜间（暗色模式）色板 color.*.dark；站点未配置夜间色板时回退日间值'}
            </span>
          </div>
          <!-- 实时色彩选择器 -->
          <div style="display:grid;grid-template-columns:repeat(auto-fill, minmax(220px, 1fr));gap:12px;">
            <label>
              <span class="field-label">页面背景色 ({colorKey('color.background')})</span>
              <div style="display:flex;gap:6px;">
                <input type="color" bind:value={editTokens[colorKey('color.background')]} style="width:36px;height:34px;border:none;border-radius:4px;cursor:pointer;" />
                <input type="text" class="input-field" bind:value={editTokens[colorKey('color.background')]} style="flex:1;" />
              </div>
            </label>

            <label>
              <span class="field-label">卡片底色 ({colorKey('color.surface')})</span>
              <div style="display:flex;gap:6px;">
                <input type="color" bind:value={editTokens[colorKey('color.surface')]} style="width:36px;height:34px;border:none;border-radius:4px;cursor:pointer;" />
                <input type="text" class="input-field" bind:value={editTokens[colorKey('color.surface')]} style="flex:1;" />
              </div>
            </label>

            <label>
              <span class="field-label">主文字色 ({colorKey('color.text')})</span>
              <div style="display:flex;gap:6px;">
                <input type="color" bind:value={editTokens[colorKey('color.text')]} style="width:36px;height:34px;border:none;border-radius:4px;cursor:pointer;" />
                <input type="text" class="input-field" bind:value={editTokens[colorKey('color.text')]} style="flex:1;" />
              </div>
            </label>

            <label>
              <span class="field-label">次要文字色 ({colorKey('color.muted')})</span>
              <div style="display:flex;gap:6px;">
                <input type="color" bind:value={editTokens[colorKey('color.muted')]} style="width:36px;height:34px;border:none;border-radius:4px;cursor:pointer;" />
                <input type="text" class="input-field" bind:value={editTokens[colorKey('color.muted')]} style="flex:1;" />
              </div>
            </label>

            <label>
              <span class="field-label">品牌/强调色 ({colorKey('color.accent')})</span>
              <div style="display:flex;gap:6px;">
                <input type="color" bind:value={editTokens[colorKey('color.accent')]} style="width:36px;height:34px;border:none;border-radius:4px;cursor:pointer;" />
                <input type="text" class="input-field" bind:value={editTokens[colorKey('color.accent')]} style="flex:1;" />
              </div>
            </label>

            <label>
              <span class="field-label">边框色 ({colorKey('color.border')})</span>
              <div style="display:flex;gap:6px;">
                <input type="color" bind:value={editTokens[colorKey('color.border')]} style="width:36px;height:34px;border:none;border-radius:4px;cursor:pointer;" />
                <input type="text" class="input-field" bind:value={editTokens[colorKey('color.border')]} style="flex:1;" />
              </div>
            </label>
          </div>

          <!-- 字体与排版 -->
          <div style="display:grid;grid-template-columns:1fr 1fr;gap:12px;margin-top:6px;">
            <label>
              <span class="field-label">正文字体族 (font.body)</span>
              <select class="input-field" bind:value={editTokens['font.body']}>
                {#each FONT_BODY_OPTIONS as font}
                  <option value={font}>{font}</option>
                {/each}
              </select>
            </label>

            <label>
              <span class="field-label">等宽字体族 (font.mono)</span>
              <select class="input-field" bind:value={editTokens['font.mono']}>
                {#each FONT_MONO_OPTIONS as font}
                  <option value={font}>{font}</option>
                {/each}
              </select>
            </label>
          </div>

          <!-- 页面结构布局（layout.mode）：主题可改变页面结构的闭集预设 -->
          <div style="margin-top:6px;padding:12px;border:1px dashed var(--color-border-strong);border-radius:var(--radius-sm);background:var(--color-bg-subtle);">
            <span class="field-label" style="display:flex;align-items:center;gap:6px;">
              页面结构布局 (layout.mode)
              <span class="sbadge sb-primary" style="font-size:10px;">结构变体</span>
            </span>
            <p class="text-secondary" style="font-size:11px;margin:4px 0 8px;line-height:1.5;">
              主题声明的整站页面结构预设（已编译闭集，不可注入任意 HTML/CSS/JS）。
              保存后设为站点默认即对全站生效；移动端自动回退经典结构。
            </p>
            <div style="display:grid;grid-template-columns:repeat(auto-fill, minmax(150px, 1fr));gap:8px;">
              <label style="display:flex;flex-direction:column;gap:4px;">
                <input type="radio" name="layout-mode" value="classic" bind:group={editTokens['layout.mode']} style="accent-color:var(--color-brand);" />
                <span style="font-size:12px;font-weight:600;">经典顶栏</span>
                <span class="text-secondary" style="font-size:11px;">顶部横导航 + 居中容器（默认）</span>
              </label>
              <label style="display:flex;flex-direction:column;gap:4px;">
                <input type="radio" name="layout-mode" value="sidebar" bind:group={editTokens['layout.mode']} style="accent-color:var(--color-brand);" />
                <span style="font-size:12px;font-weight:600;">侧栏导航</span>
                <span class="text-secondary" style="font-size:11px;">导航固定为左侧竖栏</span>
              </label>
              <label style="display:flex;flex-direction:column;gap:4px;">
                <input type="radio" name="layout-mode" value="wide" bind:group={editTokens['layout.mode']} style="accent-color:var(--color-brand);" />
                <span style="font-size:12px;font-weight:600;">宽幅全景</span>
                <span class="text-secondary" style="font-size:11px;">宽容器 + 四列卡片网格</span>
              </label>
            </div>
          </div>

          <!-- 圆角与间距密度 -->
          <div style="display:grid;grid-template-columns:repeat(auto-fill, minmax(160px, 1fr));gap:12px;margin-top:6px;">
            <label>
              <span class="field-label">控件圆角 (radius.control)</span>
              <input type="text" class="input-field" bind:value={editTokens['radius.control']} placeholder="如 0.5rem" />
            </label>
            <label>
              <span class="field-label">卡片圆角 (radius.card)</span>
              <input type="text" class="input-field" bind:value={editTokens['radius.card']} placeholder="如 0.75rem" />
            </label>
            <label>
              <span class="field-label">空间密度 (space.density)</span>
              <select class="input-field" bind:value={editTokens['space.density']}>
                <option value="compact">紧凑 (compact)</option>
                <option value="comfortable">适中 (comfortable)</option>
                <option value="relaxed">宽松 (relaxed)</option>
              </select>
            </label>
            <label>
              <span class="field-label">阴影强度 (shadow.card)</span>
              <select class="input-field" bind:value={editTokens['shadow.card']}>
                <option value="none">无阴影 (none)</option>
                <option value="sm">浅阴影 (sm)</option>
                <option value="md">中等 (md)</option>
                <option value="lg">明显 (lg)</option>
              </select>
            </label>
          </div>

          <!-- 隐藏提交文本域以保证表单提交完整序列化 -->
          <input type="hidden" name="tokens" value={JSON.stringify(editTokens)} />
        {/if}

        <label style="margin-top:8px;">
          <span class="field-label">修改原因（写入审计日志，必填）</span>
          <input type="text" name="reason" class="input-field" required placeholder="如：微调品牌强调色对比度" />
        </label>

        <div style="display:flex;justify-content:flex-end;gap:8px;margin-top:10px;">
          <button type="button" class="btn ghost" onclick={closeEditModal}>取消</button>
          <button type="submit" class="btn primary">保存 Token 设置</button>
        </div>
      </form>
    </div>
  </div>
{/if}

<!-- 弹窗 3：设为站点默认确认 Modal -->
{#if setDefaultTheme}
  <div class="modal-backdrop" style="position:fixed;inset:0;background:rgba(0,0,0,0.55);z-index:999;display:flex;align-items:center;justify-content:center;padding:16px;">
    <div
      class="app-card"
      role="dialog"
      tabindex="-1"
      aria-labelledby="default-title"
      onkeydown={(e) => { if (e.key === 'Escape') setDefaultTheme = null; }}
      style="max-width:480px;width:100%;border-radius:var(--radius-md);box-shadow:var(--shadow-modal);background:var(--color-bg-card);"
    >
      <header class="app-card__head">
        <h2 id="default-title" style="margin:0;font-size:16px;">设为站点默认主题</h2>
      </header>
      <form
        method="POST"
        action="?/set-default"
        use:enhance={() => {
          const target = setDefaultTheme;
          return async ({ result, update }) => {
            // 动作结果 → 全局 Toast（成功服务端文案“主题 x 已设为站点默认并激活”/ 失败红）；
            // 顶部横幅为无 JS 回退，失败提示同样靠 Toast，避免结果不可见。
            toastActionResult(result);
            await update();
            if (result.type === 'success' && target) {
              setDefaultTheme = null;
              if (target.name !== 'default') {
                applyThemeTokens(toThemeView(target));
              } else {
                clearThemeTokens();
              }
            }
          };
        }}
        class="app-card__body stack"
        style="gap:12px;"
      >
        <input type="hidden" name="name" value={setDefaultTheme.name} />
        <p style="font-size:13px;line-height:1.5;margin:0;">
          确定将主题<strong>「{setDefaultTheme.display_name}」</strong>设为站点默认主题吗？该操作将激活此主题并对全站未设置个人偏好的用户生效。
        </p>
        <label>
          <span class="field-label">操作原因（写入审计日志，必填）</span>
          <input type="text" name="reason" class="input-field" required placeholder="如：切换为春季新版默认视觉" />
        </label>
        <div style="display:flex;justify-content:flex-end;gap:8px;margin-top:6px;">
          <button type="button" class="btn ghost" onclick={() => (setDefaultTheme = null)}>取消</button>
          <button type="submit" class="btn primary">确认并激活</button>
        </div>
      </form>
    </div>
  </div>
{/if}

<!-- 弹窗 4：删除主题确认 Modal -->
{#if deleteTargetTheme}
  <div class="modal-backdrop" style="position:fixed;inset:0;background:rgba(0,0,0,0.55);z-index:999;display:flex;align-items:center;justify-content:center;padding:16px;">
    <div
      class="app-card"
      role="dialog"
      tabindex="-1"
      aria-labelledby="delete-title"
      onkeydown={(e) => { if (e.key === 'Escape') deleteTargetTheme = null; }}
      style="max-width:480px;width:100%;border-radius:var(--radius-md);box-shadow:var(--shadow-modal);background:var(--color-bg-card);"
    >
      <header class="app-card__head">
        <h2 id="delete-title" class="danger-heading">删除主题确认</h2>
      </header>
      <form method="POST" action="?/delete" use:enhance={withActionToast()} class="app-card__body stack" style="gap:12px;">
        <input type="hidden" name="name" value={deleteTargetTheme.name} />
        <p style="font-size:13px;line-height:1.5;margin:0;">
          确定要彻底删除主题<strong>「{deleteTargetTheme.display_name}」</strong>（<code>/{deleteTargetTheme.name}</code>）吗？此操作不可逆，所有使用该主题的设置将被清理。
        </p>
        <label>
          <span class="field-label">删除原因（写入审计日志，必填）</span>
          <input type="text" name="reason" class="input-field" required placeholder="如：下线旧版试验主题" />
        </label>
        <div style="display:flex;justify-content:flex-end;gap:8px;margin-top:6px;">
          <button type="button" class="btn ghost" onclick={() => (deleteTargetTheme = null)}>取消</button>
          <button type="submit" class="btn danger">确认删除</button>
        </div>
      </form>
    </div>
  </div>
{/if}

<style>
  @keyframes pulse {
    0%, 100% { opacity: 1; transform: scale(1); }
    50% { opacity: 0.4; transform: scale(0.85); }
  }

  .theme-hero-title {
    color: #ffffff;
    text-shadow: 0 1px 3px rgba(0, 0, 0, 0.6);
    font-weight: 600;
    font-size: 16px;
  }

  .theme-id-code {
    font-size: 12px;
    color: var(--color-text-secondary);
    display: block;
    margin-top: 2px;
  }

  .preset-code {
    font-size: 11px;
    color: var(--color-text-secondary);
    display: block;
  }

  .palette-swatch {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 1px solid var(--color-border);
    background: var(--swatch);
    display: inline-block;
  }

  .mock-brand {
    font-weight: 700;
    color: var(--color-brand);
    font-size: 16px;
  }

  .mock-avatar {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background: var(--color-brand);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: #ffffff;
    font-weight: bold;
    font-size: 12px;
  }

  .mock-author {
    font-size: 13px;
    color: var(--color-text-primary);
  }

  .mock-title {
    font-size: 15px;
    color: var(--color-text-primary);
    margin: 0 0 6px 0;
  }

  .mock-desc {
    font-size: 13px;
    color: var(--color-text-secondary);
    margin: 0 0 10px 0;
    line-height: 1.5;
  }

  .mock-code-block {
    background: var(--color-bg-subtle);
    border: 1px solid var(--color-border);
    padding: 10px 14px;
    border-radius: var(--radius-sm);
    font-family: var(--font-family-mono);
    font-size: 12px;
    color: var(--color-text-primary);
    margin: 0;
  }

  .danger-heading {
    margin: 0;
    font-size: 16px;
    color: var(--color-danger);
  }
</style>
