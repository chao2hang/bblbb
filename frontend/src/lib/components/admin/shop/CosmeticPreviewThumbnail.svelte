<!-- M07-SHOP-ADMIN：装扮样式与商品缩略图实时预览（支持 Steam 动效头像框、全景/动态个人资料背景、彩色昵称渐变） -->
<script lang="ts">
  import CosmeticAvatar from '$lib/components/wardrobe/CosmeticAvatar.svelte';
  import ProfileCover from '$lib/components/ui/ProfileCover.svelte';
  import { resolveSteamPanoramaMedia, profileEffectStyle } from '$lib/components/wardrobe/profile-effect';
  import type { CosmeticDefStyle } from '$lib/api/types';

  interface Props {
    kind?: string | null;
    id?: string | null;
    name?: string | null;
    style?: CosmeticDefStyle | null;
    size?: 'sm' | 'card';
  }
  let { kind = '', id = '', name = '', style = {}, size = 'sm' }: Props = $props();

  const safeStyle = $derived(style ?? {});
  const media = $derived(
    kind === 'profile_effect'
      ? resolveSteamPanoramaMedia(safeStyle, id, name)
      : null
  );
</script>

{#if size === 'card'}
  {#if kind === 'avatar_frame'}
    <div
      class="cosmetic-thumb-stage cosmetic-thumb-stage--card"
      style="width:100%;height:100px;border-radius:var(--radius-md, 8px);background:radial-gradient(circle, rgba(139,92,246,0.12) 0%, rgba(13,17,23,0.85) 100%), #0d1117;border:1px solid var(--border-default);display:flex;align-items:center;justify-content:center;overflow:hidden;"
    >
      <CosmeticAvatar
        name="预"
        seed="preview"
        size={60}
        presentation={{ avatar_frame: id ?? undefined, avatar_frame_style: safeStyle }}
      />
    </div>
  {:else if kind === 'profile_effect'}
    <div
      class="cosmetic-thumb-bg cosmetic-thumb-bg--card"
      aria-hidden="true"
      title={name ?? '背景预览'}
      style="width:100%;height:100px;border-radius:var(--radius-md, 8px);overflow:hidden;border:1px solid var(--border-default);background:#090514;position:relative;display:flex;align-items:center;justify-content:center;"
    >
      {#if media && (media.image || media.fallbackSrc || media.webm || media.mp4)}
        <ProfileCover
          src={media.image}
          fallbackSrc={media.fallbackSrc}
          videoWebm={media.webm}
          videoMp4={media.mp4}
          label={name ?? '个人资料背景'}
          style="position:absolute;inset:0;width:100%;height:100%;"
        />
      {:else if safeStyle.url || safeStyle.image}
        <img
          src={safeStyle.url ?? (safeStyle.image ? `/api/v1/steam-assets/backgrounds/${safeStyle.image}` : '')}
          alt=""
          style="position:absolute;inset:0;width:100%;height:100%;object-fit:cover;"
        />
      {:else}
        <span
          style="display:block;width:100%;height:100%;background:{safeStyle.texture ? '' : `linear-gradient(135deg, ${safeStyle.baseColor ?? '#101827'}, ${safeStyle.accentColor ?? '#8b5cf6'})`};{profileEffectStyle(safeStyle)}"
        ></span>
      {/if}
      {#if media?.webm || media?.mp4}
        <span class="badge badge-neutral" style="position:absolute;bottom:4px;right:4px;font-size:10px;padding:2px 6px;background:rgba(0,0,0,0.65);backdrop-filter:blur(4px);color:#fff;border:none;">
          🎬 动态背景
        </span>
      {/if}
    </div>
  {:else}
    <div
      class="cosmetic-thumb-nickname cosmetic-thumb-nickname--card"
      style="width:100%;height:100px;border-radius:var(--radius-md, 8px);background:var(--color-bg-subtle, #1a1d24);border:1px solid var(--border-default);display:flex;flex-direction:column;align-items:center;justify-content:center;gap:8px;padding:var(--space-2);"
    >
      <span
        style="font-size:16px;font-weight:700;letter-spacing:0.5px;{safeStyle.mode === 'gradient' && safeStyle.colors ? `background:linear-gradient(90deg,${safeStyle.colors.join(',')});-webkit-background-clip:text;-webkit-text-fill-color:transparent;` : safeStyle.color ? `color:${safeStyle.color};` : ''}"
      >
        {name || '彩色昵称'}
      </span>
      {#if safeStyle.mode === 'gradient' && safeStyle.colors}
        <span style="display:inline-block;width:64px;height:6px;border-radius:3px;background:linear-gradient(90deg,{safeStyle.colors.join(',')});"></span>
      {:else if safeStyle.color}
        <span style="display:inline-block;width:14px;height:14px;border-radius:50%;background:{safeStyle.color};border:1px solid rgba(255,255,255,0.2);"></span>
      {/if}
    </div>
  {/if}
{:else}
  {#if kind === 'avatar_frame'}
    <CosmeticAvatar
      name="预"
      seed="preview"
      size={36}
      presentation={{ avatar_frame: id ?? undefined, avatar_frame_style: safeStyle }}
    />
  {:else if kind === 'profile_effect'}
    <div
      class="cosmetic-thumb-bg"
      aria-hidden="true"
      title={name ?? '背景预览'}
      style="width:56px;height:36px;border-radius:var(--radius-sm, 6px);overflow:hidden;border:1px solid var(--border-default);background:#090514;position:relative;display:flex;align-items:center;justify-content:center;flex:0 0 auto;"
    >
      {#if media && (media.image || media.fallbackSrc || media.webm || media.mp4)}
        <ProfileCover
          src={media.image}
          fallbackSrc={media.fallbackSrc}
          videoWebm={media.webm}
          videoMp4={media.mp4}
          label={name ?? '个人资料背景'}
          style="position:absolute;inset:0;width:100%;height:100%;"
        />
      {:else if safeStyle.url || safeStyle.image}
        <img
          src={safeStyle.url ?? (safeStyle.image ? `/api/v1/steam-assets/backgrounds/${safeStyle.image}` : '')}
          alt=""
          style="position:absolute;inset:0;width:100%;height:100%;object-fit:cover;"
        />
      {:else}
        <span
          style="display:block;width:100%;height:100%;background:{safeStyle.texture ? '' : `linear-gradient(135deg, ${safeStyle.baseColor ?? '#101827'}, ${safeStyle.accentColor ?? '#8b5cf6'})`};{profileEffectStyle(safeStyle)}"
        ></span>
      {/if}
    </div>
  {:else if safeStyle.mode === 'gradient' && safeStyle.colors}
    <span
      aria-hidden="true"
      title="渐变色"
      style="display:inline-block;width:36px;height:18px;border-radius:9px;background:linear-gradient(90deg,{safeStyle.colors.join(',')});flex:0 0 auto;"
    ></span>
  {:else if safeStyle.color}
    <span
      aria-hidden="true"
      title={safeStyle.color}
      style="display:inline-block;width:18px;height:18px;border-radius:50%;background:{safeStyle.color};flex:0 0 auto;"
    ></span>
  {:else}
    <span
      aria-hidden="true"
      style="display:inline-block;width:18px;height:18px;border-radius:50%;background:{safeStyle.color ?? '#ccc'};flex:0 0 auto;"
    ></span>
  {/if}
{/if}
