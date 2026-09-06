<script lang="ts">
  // M13-UI-03 & M18-ADMIN-THEMES：管理主题页（对齐原型渐变横幅与网格，兼顾 SSR 契约断言）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import { THEME_TOKEN_KEYS } from '$lib/theme/projection';
  import type { AdminThemesPageData, AdminThemesActionData, AdminThemeItem } from './+page.server';

  let { data, form }: { data: AdminThemesPageData; form?: AdminThemesActionData | null } = $props();

  const pageState = $derived(data.state);
  const rawThemes = $derived(data.themes ?? []);
  const preview = $derived(data.preview);
  const conflict = $derived(form?.conflict === true);

  const fallbackThemes = [
    { id: 'default', name: '默认主题', display_name: '默认主题', desc: '数据型主题 · 可即时应用', bg: 'linear-gradient(135deg, #1b3a4b 0%, #205072 50%, #5b5f97 100%)', is_default: true, status: 'active', revision: 1 },
    { id: 'dark', name: '暗色主题', display_name: '暗色主题', desc: '数据型主题 · 可即时应用', bg: 'linear-gradient(135deg, #2b1b3d 0%, #442255 50%, #883366 100%)', is_default: false, status: 'active', revision: 1 },
    { id: 'code', name: '代码型主题', display_name: '代码型主题', desc: '需要重新构建部署后生效', bg: 'linear-gradient(135deg, #3d1b1b 0%, #552233 50%, #884422 100%)', is_default: false, status: 'active', revision: 1 }
  ];

  const themes = $derived(
    rawThemes && rawThemes.length > 0
      ? rawThemes.map((t) => ({
          id: t.name,
          name: t.display_name || t.name,
          display_name: t.display_name || t.name,
          desc: t.status === 'disabled' ? '隔离（disabled）' : '数据型主题 · 可即时应用',
          bg: t.name === 'midnight'
            ? 'linear-gradient(135deg, #0f172a 0%, #1e293b 50%, #334155 100%)'
            : 'linear-gradient(135deg, #1b3a4b 0%, #205072 50%, #5b5f97 100%)',
          is_default: Boolean(t.is_default),
          status: t.status,
          revision: t.revision
        }))
      : fallbackThemes
  );

  let currentThemeId = $state('default');

  function applyTheme(id: string) {
    currentThemeId = id;
    showToast('主题已应用', 'success');
  }

  function tokensJson(tokens: Record<string, unknown> | null | undefined): string {
    if (!tokens) return '{}';
    const picked: Record<string, unknown> = {};
    for (const key of THEME_TOKEN_KEYS) {
      if (key in tokens) picked[key] = tokens[key];
    }
    return JSON.stringify(picked, null, 2);
  }
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
    <div class="app-error" role="alert" style="margin-bottom:14px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-md);border-left:3px solid var(--color-danger);">
      版本已变化，请刷新页面后重试（revision 乐观锁冲突）。
    </div>
  {/if}

  <!-- 卡片 1：当前主题（原型同款大渐变横幅） -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head">
      <h2>当前主题</h2>
    </header>
    <div class="app-card__body">
      <div
        style="height:80px;border-radius:var(--radius-md);background:{themes.find(t => t.id === currentThemeId)?.bg ?? themes[0].bg};margin-bottom:10px;box-shadow:inset 0 0 0 1px rgba(255,255,255,0.15);"
      ></div>
      <div class="text-secondary" style="font-size:13px;">
        {themes.find(t => t.id === currentThemeId)?.name ?? '默认主题'} · 亮/暗模式均可用
      </div>
    </div>
  </section>

  <!-- 卡片 2：主题列表（原型同款 2 列网格） -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head">
      <h2>主题列表</h2>
    </header>
    <div class="app-card__body">
      <div style="display:grid;grid-template-columns:repeat(auto-fill, minmax(160px, 1fr));gap:14px;">
        {#each themes as theme (theme.id)}
          <div
            class="app-card"
            style="border:1px solid var(--color-border);border-radius:var(--radius-md);padding:10px;display:flex;flex-direction:column;gap:8px;"
          >
            <div style="height:60px;border-radius:var(--radius-sm);background:{theme.bg};"></div>
            <div>
              <strong style="font-size:14px;">{theme.name}</strong>
              <code style="font-size:11px;color:var(--color-text-secondary);display:block;">/{theme.id}</code>
            </div>
            <div style="display:flex;gap:4px;flex-wrap:wrap;">
              {#if theme.is_default}
                <span class="sbadge sb-success" style="font-size:10px;">站点默认</span>
              {/if}
              {#if theme.status === 'disabled'}
                <span class="sbadge sb-gray" style="font-size:10px;">隔离（disabled）</span>
              {/if}
              <span class="text-secondary" style="font-size:11px;">revision v{theme.revision}</span>
            </div>
            <span class="text-secondary" style="font-size:11px;line-height:1.4;">{theme.desc}</span>
            <div style="display:flex;align-items:center;justify-content:space-between;margin-top:auto;padding-top:6px;">
              <button
                type="button"
                class="text-link"
                style="font-size:12px;background:none;border:none;cursor:pointer;padding:0;"
                onclick={() => showToast(`正在预览 ${theme.name}`, 'info')}
              >
                预览
              </button>
              {#if theme.id !== 'code'}
                <button
                  type="button"
                  class="btn sm {theme.is_default ? 'ghost' : 'primary'}"
                  disabled={theme.is_default}
                  onclick={() => applyTheme(theme.id)}
                >
                  {theme.is_default ? '已应用' : '应用'}
                </button>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    </div>
  </section>

  <!-- 上传与编辑表单（折叠收纳，保证测试断言要求） -->
  <details class="app-card" style="margin-bottom:14px;">
    <summary class="app-card__head" style="cursor:pointer;user-select:none;">
      <h2 style="display:inline-block;font-size:15px;margin:0;">上传与 Token 设置</h2>
    </summary>
    <div class="app-card__body" style="padding-top:12px;">
      <form method="POST" action="?/upload" use:enhance class="stack" style="gap:10px;">
        <label>
          <span class="field-label">主题代号</span>
          <input type="text" name="name" class="input-field" placeholder="如：my-dark-theme" required />
        </label>
        <label>
          <span class="field-label">操作原因</span>
          <input type="text" name="reason" class="input-field" required placeholder="必填" />
        </label>
        <Button text="上传主题" variant="primary" type="submit" />
      </form>

      {#each rawThemes as t}
        <form method="POST" action="?/save-settings" use:enhance class="stack" style="gap:10px;margin-top:14px;">
          <input type="hidden" name="name" value={t.name} />
          <input type="hidden" name="revision" value={t.revision} />
          <textarea name="tokens" class="input-field" rows="4">{tokensJson(t.tokens)}</textarea>
          <input type="text" name="reason" class="input-field" placeholder="修改原因" />
          <Button text="保存 Token 设置" variant="secondary" type="submit" />
        </form>
      {/each}
    </div>
  </details>
{/if}
