<!-- M07-SHOP-STUDIO-UX：类型卡片 / 预设卡片的迷你实时预览（仅保留彩色昵称、Steam动效头像框、个人资料背景三大品类）。 -->
<script lang="ts">
  import CosmeticAvatar from '$lib/components/wardrobe/CosmeticAvatar.svelte';
  import CosmeticName from '$lib/components/wardrobe/CosmeticName.svelte';
  import { draftToPresentation, draftToStyle, type StyleDraft } from './style-draft';

  let { draft }: { draft: StyleDraft } = $props();

  const presentation = $derived(draftToPresentation(draft));
  const style = $derived(draftToStyle(draft));
</script>

{#if draft.kind === 'nickname_color'}
  <span class="pv-name"><CosmeticName name={draft.name || '昵称预览'} {presentation} /></span>
{:else if draft.kind === 'avatar_frame'}
  <CosmeticAvatar name="预" seed="studio-preset" size={34} {presentation} />
{:else if draft.kind === 'profile_effect'}
  {#if draft.profileBackgroundUrl}
    <span class="pv-swatch" style:background="url('{draft.profileBackgroundUrl}') center/cover no-repeat" aria-hidden="true"></span>
  {:else}
    <span class="pv-swatch" style:background="linear-gradient(135deg, {draft.baseColor}, {draft.accentColor})" aria-hidden="true"></span>
  {/if}
{/if}

<style>
  .pv-name {
    font-weight: 700;
    font-size: var(--text-sm, 13px);
    white-space: nowrap;
  }
  .pv-swatch {
    display: inline-block;
    width: 34px;
    height: 34px;
    border-radius: var(--radius-sm, 6px);
    border: 1px solid var(--color-border);
  }
</style>
