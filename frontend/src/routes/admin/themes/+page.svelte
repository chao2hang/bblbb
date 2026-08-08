<script lang="ts">
  // M13-UI-03：管理主题页——列表/上传/设默认/Token 编辑/预览/回退/版本冲突。
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { adminStateLabel } from '$lib/admin';
  import { THEME_TOKEN_KEYS, applyThemeTokens, prefersReducedMotion } from '$lib/theme/projection';
  import type { AdminThemesPageData, AdminThemesActionData, AdminThemeItem } from './+page.server';

  let { data, form }: { data: AdminThemesPageData; form?: AdminThemesActionData | null } = $props();

  const state = $derived(data.state);
  const themes = $derived(data.themes);
  const preview = $derived(data.preview);
  const message = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );
  const conflict = $derived(form?.conflict === true);

  // M13-UI-03：预览（仅把后端校验过的安全 Token 应用到当前页面）。
  $effect(() => {
    if (preview && typeof document !== 'undefined') {
      applyThemeTokens(preview as never, document.documentElement);
    }
  });

  const reduced = $derived(preview ? prefersReducedMotion(preview as never) : false);

  const tokenLabels: Record<string, string> = {
    'color.background': '背景色',
    'color.surface': '表面色',
    'color.text': '文字色',
    'color.muted': '弱化文字',
    'color.accent': '强调色',
    'color.border': '边框色',
    'font.body': '正文字体',
    'font.mono': '等宽字体',
    'radius.control': '控件圆角',
    'radius.card': '卡片圆角',
    'space.density': '密度',
    'shadow.card': '卡片阴影',
    'motion.duration': '动画时长',
    'motion.reduced': '减少动效'
  };

  function tokensJson(theme: AdminThemeItem | null): string {
    if (!theme?.tokens) return '{}';
    const picked: Record<string, unknown> = {};
    for (const key of THEME_TOKEN_KEYS) {
      if (key in theme.tokens) picked[key] = theme.tokens[key];
    }
    return JSON.stringify(picked, null, 2);
  }
</script>

<svelte:head>
  <title>主题管理 — BBLBB</title>
</svelte:head>

<div class="card">
  <div class="card-header"><span class="card-title">主题管理</span></div>
  <div class="card-body">
    {#if state === 'forbidden'}
      <p class="input-hint is-error" role="alert"><Icon name="lock" size={14} /> {adminStateLabel('forbidden')}</p>
    {:else if state === 'not_implemented'}
      <p class="input-hint" role="note">主题接口开发中。核心论坛功能不受影响。</p>
    {:else if state === 'error'}
      <p class="input-hint is-error" role="alert">{data.error || adminStateLabel('error')}</p>
    {:else if state === 'ok'}
      {#if reduced}
        <p class="input-hint" role="note">减少动效已启用（主题或系统偏好）。</p>
      {/if}
      {#if message}
        <p class="input-hint {conflict ? 'is-error' : ''}" role="status">{message}</p>
      {/if}
      {#if conflict}
        <p class="input-hint is-error" role="alert">主题版本已变化，请刷新页面后重试（revision 乐观锁）。</p>
      {/if}

      {#if preview}
        <div class="card" style="margin-bottom:var(--space-4);">
          <div class="card-header"><span class="card-title">预览：{preview.name}（revision v{preview.revision}）</span></div>
          <div class="card-body" style="display:flex;gap:var(--space-3);align-items:center;flex-wrap:wrap;">
            <span style="width:1.5rem;height:1.5rem;border-radius:var(--bb-radius-control,0.5rem);background:var(--bb-color-accent,#2563eb);"></span>
            <span style="background:var(--bb-color-surface,#fff);color:var(--bb-color-text,#1f2937);border:1px solid var(--bb-color-border,#e5e7eb);border-radius:var(--bb-radius-card,0.75rem);padding:var(--space-2) var(--space-3);">
              {preview.name} 预览卡片
            </span>
            <a class="btn btn-secondary btn-sm" href="/">查看站点</a>
          </div>
        </div>
      {/if}

      {#if !themes || themes.length === 0}
        <p class="input-hint">暂无主题数据（内置 default 兜底）。</p>
      {:else}
        <ul style="list-style:none;margin:0;padding:0;display:flex;flex-direction:column;gap:var(--space-2);">
          {#each themes as theme (theme.name)}
            <li style="padding:var(--space-3);border:1px solid var(--color-border);border-radius:var(--radius-md);">
              <div style="display:flex;justify-content:space-between;align-items:center;gap:var(--space-3);flex-wrap:wrap;">
                <div>
                  <strong>{theme.display_name}</strong>
                  <span class="text-secondary" style="font-size:var(--text-sm);margin-left:var(--space-2);">/{theme.name} v{theme.version}</span>
                </div>
                <div style="display:flex;gap:var(--space-2);align-items:center;flex-wrap:wrap;">
                  {#if theme.is_default}
                    <span class="badge badge-primary">站点默认</span>
                  {/if}
                  {#if theme.status === 'active'}
                    <span class="badge badge-success">激活</span>
                  {:else if theme.status === 'disabled'}
                    <span class="badge">隔离（disabled）</span>
                  {:else if theme.status === 'corrupt'}
                    <span class="badge badge-danger">损坏（已回退 default）</span>
                  {/if}
                  <span class="text-secondary" style="font-size:var(--text-sm);">revision v{theme.revision}</span>
                  {#if !theme.is_default && theme.status !== 'active'}
                    <form method="POST" action="?/set-default" use:enhance>
                      <input type="hidden" name="name" value={theme.name} />
                      <div style="display:flex;gap:var(--space-2);">
                        <input type="text" class="input-field" name="reason" placeholder="操作原因（审计）" required style="max-width:180px;" />
                        <Button text="设为默认" variant="primary" size="sm" type="submit" />
                      </div>
                    </form>
                  {/if}
                </div>
              </div>
            </li>
          {/each}
        </ul>
      {/if}

      <!-- 上传数据主题（M13-THEME-06：走附件安全处理语义，上传即隔离） -->
      <form method="POST" action="?/upload" use:enhance class="card" style="margin-top:var(--space-4);">
        <div class="card-header"><span class="card-title">上传数据型主题</span></div>
        <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
          <div class="input-wrapper">
            <label class="input-label" for="theme-name">主题名</label>
            <input type="text" class="input-field" id="theme-name" name="name" maxlength="64" pattern="[a-z0-9-]+" required />
            <p class="input-hint">小写字母/数字/连字符（&lt;=64），上传后为 disabled 隔离态。</p>
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="theme-display">显示名</label>
            <input type="text" class="input-field" id="theme-display" name="display_name" maxlength="120" />
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="theme-tokens">Token（JSON，封闭 schema）</label>
            <textarea class="input-field" id="theme-tokens" name="tokens_json" rows="10" required spellcheck="false">{tokensJson(themes?.[0] ?? null)}</textarea>
            <p class="input-hint">只接受 {THEME_TOKEN_KEYS.length} 个已知 Token key；拒绝 CSS/HTML/JS/SVG/远程资源。</p>
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="theme-upload-reason">操作原因（审计）</label>
            <input type="text" class="input-field" id="theme-upload-reason" name="reason" required placeholder="记录到审计日志" />
          </div>
          <div><Button text="上传主题" variant="primary" size="sm" type="submit" /></div>
        </div>
      </form>

      <!-- 编辑激活主题 Token（If-Match revision 乐观锁） -->
      {#if preview && preview.name !== 'default'}
        <form method="POST" action="?/save-settings" use:enhance class="card" style="margin-top:var(--space-4);">
          <div class="card-header"><span class="card-title">编辑 {preview.name} Token（当前 revision v{preview.revision}）</span></div>
          <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
            <input type="hidden" name="name" value={preview.name} />
            <input type="hidden" name="revision" value={String(preview.revision)} />
            <div class="input-wrapper">
              <label class="input-label" for="settings-tokens">Token（JSON）</label>
              <textarea class="input-field" id="settings-tokens" name="tokens_json" rows="10" spellcheck="false">{tokensJson(data.themes?.find((t) => t.name === preview.name) ?? null)}</textarea>
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="settings-reason">操作原因（审计）</label>
              <input type="text" class="input-field" id="settings-reason" name="reason" required placeholder="记录到审计日志" />
            </div>
            <div><Button text="保存 Token（提升 revision）" variant="primary" size="sm" type="submit" /></div>
          </div>
        </form>
      {/if}
    {/if}
  </div>
</div>
