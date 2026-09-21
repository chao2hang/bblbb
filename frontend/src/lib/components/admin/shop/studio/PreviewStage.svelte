<!-- M07-SHOP-STUDIO：多场景实时试穿舞台（STUDIO-UX 轮：场景 Tab + 亮/暗画布）。
  按设计草稿把同一样式投射到真实使用场景：帖子行（昵称/头像框/称号）、
  头像特写、徽章行、个人主页 mock、帖子特效 mock；头像框支持 PNG/APNG
  自定义图片模式（assetAttachmentId）。所有场景复用 wardrobe 渲染组件，
  与线上渲染路径一致——所见即所得。
  JS：场景以 Tab 切换，画布可切亮/暗底（检验深色场景可读性）；
  无 JS：Tab 不渲染，全部场景堆叠（与旧版一致）。 -->
<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import CosmeticAvatar from '$lib/components/wardrobe/CosmeticAvatar.svelte';
  import CosmeticName from '$lib/components/wardrobe/CosmeticName.svelte';
  import type { PublicPresentationTokens } from '$lib/api/types';
  import { draftToPresentation, draftToStyle, type StyleDraft } from './style-draft';
  import ProfileEffectPreview from './ProfileEffectPreview.svelte';

  let {
    draft,
    assetAttachmentId = null,
    userName = '用户昵称'
  }: {
    draft: StyleDraft;
    /** PNG/APNG 头像框模式：ready 公共附件 id（服务端校验过）。 */
    assetAttachmentId?: string | null;
    userName?: string;
  } = $props();

  const style = $derived(draftToStyle(draft));
  const presentation = $derived.by((): PublicPresentationTokens => {
    const p: PublicPresentationTokens = { ...draftToPresentation(draft) };
    if (assetAttachmentId && draft.kind === 'avatar_frame') {
      // 自定义图片模式：图片帧优先于样式环（与 CosmeticAvatar 渲染顺序一致）。
      p.avatar_frame_attachment_id = assetAttachmentId;
    }
    return p;
  });

  type SceneId = 'post' | 'avatar' | 'profile';
  interface Scene {
    id: SceneId;
    label: string;
  }

  /** 当前 kind 有意义的场景（固定顺序，仅保留帖子列表、头像特写与个人主页）。 */
  const scenes = $derived.by((): Scene[] => {
    const out: Scene[] = [];
    const k = draft.kind;
    if (k === 'nickname_color' || k === 'avatar_frame') out.push({ id: 'post', label: '帖子列表' });
    if (k === 'avatar_frame') out.push({ id: 'avatar', label: '头像特写' });
    if (k === 'profile_effect' || k === 'nickname_color' || k === 'avatar_frame') {
      out.push({ id: 'profile', label: '个人主页 / 封面' });
    }
    return out;
  });

  // 用 onMount 而非 $effect：$effect 在 SSR（svelte/server）下抛 effect_orphan。
  let hasJs = $state(false);
  onMount(() => {
    hasJs = true;
  });

  /** 用户点选的场景；kind 切换后若不再有效则回退到第一个可用场景。 */
  let userScene = $state<SceneId | ''>('');
  const activeScene = $derived(scenes.some((s) => s.id === userScene) ? (userScene as SceneId) : (scenes[0]?.id ?? 'post'));

  let dark = $state(false);

  function visible(id: SceneId): boolean {
    return hasJs ? activeScene === id : true;
  }
</script>

<div class="stage-canvas" class:is-dark={dark}>
  {#if hasJs && scenes.length > 1}
    <div class="stage-toolbar">
      <div class="stage-tabs" role="tablist" aria-label="预览场景">
        {#each scenes as s (s.id)}
          <button
            type="button"
            class="stage-tab"
            class:is-active={activeScene === s.id}
            role="tab"
            aria-selected={activeScene === s.id}
            onclick={() => (userScene = s.id)}
          >
            {s.label}
          </button>
        {/each}
      </div>
      <div class="stage-actions">
        {#if draft.css}
          <span class="stage-code-pill" title="已生效自定义 CSS">CSS 运行中</span>
        {/if}
        {#if draft.js}
          <span class="stage-code-pill stage-code-pill--js" title="已激活自定义 JS">JS 运行中</span>
        {/if}
        <button type="button" class="stage-dark-toggle" onclick={() => (dark = !dark)} aria-pressed={dark}>
          <Icon name={dark ? 'sun' : 'moon'} size={12} />
          {dark ? '亮色画布' : '暗色画布'}
        </button>
      </div>
    </div>
  {/if}

  <div class="stage">
    {#if scenes.some((s) => s.id === 'post') && visible('post')}
      <section class="stage__scene">
        <h4 class="stage__label">帖子列表</h4>
        <div class="stage-post-row">
          <CosmeticAvatar name="用户" seed="studio-preview" size={40} {presentation} />
          <div class="stage-post-row__meta">
            <CosmeticName name={userName} {presentation} />
            <p class="stage-post-row__text">示例帖子标题：样式的实际穿戴效果会随场景实时更新。</p>
          </div>
        </div>
      </section>
    {/if}

    {#if scenes.some((s) => s.id === 'avatar') && visible('avatar')}
      <section class="stage__scene">
        <h4 class="stage__label">头像特写</h4>
        <div class="stage-avatar-row">
          <CosmeticAvatar name="用户" seed="studio-preview" size={80} {presentation} />
          <CosmeticAvatar name="用户" seed="studio-preview" size={40} {presentation} />
          <CosmeticAvatar name="用户" seed="studio-preview" size={28} {presentation} />
        </div>
      </section>
    {/if}

    {#if scenes.some((s) => s.id === 'profile') && visible('profile')}
      <section class="stage__scene">
        <h4 class="stage__label">个人主页 / 封面背景</h4>
        <ProfileEffectPreview
          style={draft.kind === 'profile_effect' ? style : {}}
          presentation={draft.kind === 'profile_effect' ? null : presentation}
          name={userName}
        />
      </section>
    {/if}
  </div>
</div>

<style>
  /* 舞台卡：浮在页面画布上的「试衣间」本体 */
  .stage-canvas {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md, 8px);
    padding: 14px 16px 16px;
    background: var(--color-bg-card);
    box-shadow: var(--shadow-sm, 0 1px 3px rgba(0, 0, 0, 0.04));
    transition: background 0.15s ease, border-color 0.15s ease;
  }
  .stage-canvas.is-dark {
    background: #0D1014;
    border-color: #262C35;
    color: #EDEFF2;
  }
  .stage-canvas.is-dark .stage__label,
  .stage-canvas.is-dark .stage-post-row__text {
    color: #A2AAB6;
  }
  .stage-canvas.is-dark .stage__scene {
    background: #14181E;
    border-color: #262C35;
  }
  .stage-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2, 8px);
    flex-wrap: wrap;
    margin-bottom: var(--space-3, 12px);
  }
  /* 场景切换：内嵌式分段胶囊（选中态浮起） */
  .stage-tabs {
    display: inline-flex;
    gap: 2px;
    max-width: 100%;
    padding: 3px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm, 6px);
    background: var(--color-bg-subtle);
  }
  .stage-tab {
    border: none;
    background: transparent;
    color: var(--color-text-secondary);
    border-radius: var(--radius-sm, 4px);
    padding: 4px 12px;
    font-size: var(--text-xs, 12px);
    cursor: pointer;
    white-space: nowrap;
    transition: background 120ms ease, color 120ms ease;
  }
  .stage-tab:hover {
    color: var(--color-text-primary);
  }
  .stage-tab.is-active {
    background: var(--color-bg-card);
    color: var(--color-brand);
    font-weight: 600;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.08);
  }
  .is-dark .stage-tabs {
    border-color: #262C35;
    background: #171C23;
  }
  .is-dark .stage-tab {
    color: #A2AAB6;
  }
  .is-dark .stage-tab:hover {
    color: #EDEFF2;
  }
  .is-dark .stage-tab.is-active {
    background: #1C2436;
    color: #7B92FF;
    box-shadow: none;
  }
  .stage-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .stage-code-pill {
    display: inline-flex;
    align-items: center;
    padding: 2px 8px;
    font-size: 11px;
    font-weight: 600;
    border-radius: var(--radius-sm, 4px);
    background: var(--color-brand-soft);
    color: var(--color-brand);
    border: 1px solid color-mix(in srgb, var(--color-brand) 30%, transparent);
  }
  .stage-code-pill--js {
    background: var(--color-warning-soft);
    color: var(--color-warning);
    border-color: color-mix(in srgb, var(--color-warning) 30%, transparent);
  }
  .stage-dark-toggle {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    border: 1px solid var(--color-border);
    background: var(--color-bg-card);
    color: var(--color-text-secondary);
    font-size: var(--text-xs, 12px);
    cursor: pointer;
    border-radius: var(--radius-sm, 6px);
    padding: 4px 11px;
    transition: border-color 120ms ease, color 120ms ease;
  }
  .stage-dark-toggle:hover {
    border-color: var(--color-border-strong);
    color: var(--color-text-primary);
  }
  .is-dark .stage-dark-toggle {
    border-color: #262C35;
    background: #171C23;
    color: #A2AAB6;
  }
  .is-dark .stage-dark-toggle:hover {
    border-color: #3B434F;
    color: #EDEFF2;
  }
  .stage {
    display: flex;
    flex-direction: column;
    gap: var(--space-3, 12px);
  }
  .stage__scene {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm, 6px);
    padding: var(--space-3, 12px);
    background: var(--color-bg-card);
  }
  .stage__label {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 0 var(--space-2, 8px);
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-tertiary);
    letter-spacing: 0.06em;
  }
  /* 场景标签前的小方块标记：安静地标注「这是 mock 场景」 */
  .stage__label::before {
    content: '';
    width: 5px;
    height: 5px;
    border-radius: 2px;
    background: var(--color-border-strong);
  }
  .is-dark .stage__label::before {
    background: #3B434F;
  }
  .stage-post-row {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2, 8px);
  }
  .stage-post-row__meta {
    min-width: 0;
    font-weight: 600;
  }
  .stage-post-row__text {
    margin: 2px 0 0;
    font-weight: 400;
    color: var(--color-text-secondary);
    font-size: var(--text-sm, 13px);
  }
  .stage-avatar-row {
    display: flex;
    align-items: center;
    gap: var(--space-3, 12px);
  }
  @media (prefers-reduced-motion: reduce) {
    .stage-tab,
    .stage-dark-toggle,
    .stage-canvas {
      transition: none;
    }
  }
</style>
