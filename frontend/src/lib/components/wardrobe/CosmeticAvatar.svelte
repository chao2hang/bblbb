<script lang="ts">
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import { attachmentContentUrl } from '$lib/api/client';
  import type { PublicPresentationTokens } from '$lib/api/types';
  import { AVATAR_FRAMES, AVATAR_ATTACHMENTS } from './tokens';

  let {
    name,
    size = 'md',
    presentation = null,
    avatarAttachmentId = null,
    seed = null,
    username = null,
    userId = null,
    title,
    class: klass = ''
  }: {
    name: string;
    size?: string | number;
    presentation?: PublicPresentationTokens | { presentation_tokens?: PublicPresentationTokens | null } | null;
    avatarAttachmentId?: string | null;
    seed?: string | null;
    username?: string | null;
    userId?: string | null;
    title?: string;
    class?: string;
  } = $props();

  let frameFailed = $state(false);
  function resolveTokens(
    input: PublicPresentationTokens | { presentation_tokens?: PublicPresentationTokens | null } | null
  ): PublicPresentationTokens {
    if (input && 'presentation_tokens' in input) return input.presentation_tokens ?? {};
    return (input as PublicPresentationTokens) ?? {};
  }

  const tokens = $derived(resolveTokens(presentation));
  const frameClass = $derived(
    typeof tokens.avatar_frame === 'string' ? AVATAR_FRAMES[tokens.avatar_frame] ?? null : null
  );
  const frameAssetId = $derived(
    typeof tokens.avatar_frame_attachment_id === 'string' ? tokens.avatar_frame_attachment_id : null
  );
  const avatarAttachment = $derived(
    typeof tokens.avatar_attachment === 'string' ? AVATAR_ATTACHMENTS[tokens.avatar_attachment] ?? null : null
  );
</script>

<span class="cosmetic-avatar {klass}" class:has-frame={Boolean(frameClass || (frameAssetId && !frameFailed))}>
  <Avatar {name} {size} {title} attachmentId={avatarAttachmentId} seed={seed ?? username ?? userId} />
  {#if avatarAttachment}
    <span class="cosmetic-avatar__attachment" aria-hidden="true">{avatarAttachment}</span>
  {/if}
  {#if frameAssetId && !frameFailed}
    <img class="cosmetic-avatar__asset cosmetic-avatar__frame" src={attachmentContentUrl(frameAssetId)} alt="" aria-hidden="true" onerror={() => (frameFailed = true)} />
  {:else if frameClass}
    <span class="cosmetic-avatar__fallback-frame {frameClass}" aria-hidden="true"></span>
  {/if}
</span>

<style>
  .cosmetic-avatar { position: relative; display: inline-flex; flex: 0 0 auto; width: max-content; height: max-content; vertical-align: middle; }
  .cosmetic-avatar__asset,
  .cosmetic-avatar__fallback-frame { position: absolute; pointer-events: none; }
  .cosmetic-avatar__attachment { position: absolute; bottom: -2px; right: -2px; font-size: 14px; line-height: 1; z-index: 3; pointer-events: none; }
  /* 帧环不设 z-index：绝对定位本身已高于非定位的头像本体。若给正值
     （曾为 2），在参与者头像栈（后续头像 -7px 叠加）里会穿透到栈的
     层叠上下文顶层、画在下一个头像之上——产品语义是帧环与头像本体
     同层、随栈被后续头像遮住，故保持 auto（attachment 徽标 z-index:3
     仍在其上）。 */
  .cosmetic-avatar__frame { inset: 0; width: 100%; height: 100%; object-fit: contain; }
  .cosmetic-avatar__fallback-frame { inset: 0; width: 100%; height: 100%; object-fit: contain; border: 3px solid #d4a017; border-radius: 0; box-shadow: 0 0 10px rgba(212, 160, 23, .48); }
  .cosmetic-avatar__fallback-frame.avatar-frame-blue { border-color: #0969da; box-shadow: 0 0 10px rgba(9, 105, 218, .42); }
  .cosmetic-avatar__fallback-frame.avatar-frame-glow { border-color: #8250df; box-shadow: 0 0 14px rgba(130, 80, 223, .75); animation: cosmetic-avatar-pulse 3s ease-in-out infinite; }
  @keyframes cosmetic-avatar-pulse { 50% { box-shadow: 0 0 22px rgba(130, 80, 223, .95); } }
  @media (prefers-reduced-motion: reduce) { .cosmetic-avatar__fallback-frame.avatar-frame-glow { animation: none; } }
</style>
