<!-- M07-UI-05/06：衣柜——装备槽（昵称颜色/头像框/挂件/徽章≤3/主页装饰/
  帖子装饰）、过期自动卸下、装饰预览（只渲染白名单 Token，见 tokens.ts）、
  减少动效（prefers-reduced-motion）。
-->
<script lang="ts">
  import { enhance } from '$app/forms';
  import {
    NICKNAME_COLORS,
    NICKNAME_DECORATIONS,
    AVATAR_FRAMES,
    AVATAR_ATTACHMENTS,
    BADGES,
    TITLE_PREFIXES,
    PROFILE_EFFECTS,
    POST_EFFECTS,
    slotLabel
  } from '$lib/components/wardrobe/tokens';
  import Button from '$lib/components/ui/Button.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import ReactionBar from '$lib/components/ReactionBar.svelte';
  import AttachmentUploader from '$lib/components/upload/AttachmentUploader.svelte';
  import AttachmentPicker from '$lib/components/upload/AttachmentPicker.svelte';
  import type { Entitlement, Presentation } from '$lib/api/types';
  import type { WardrobeActionData, WardrobePageData } from './+page.server';

  let { data, form }: { data: WardrobePageData; form?: WardrobeActionData | null } = $props();

  /** 附件演示区当前选中（引用为头像/Cover 的占位）。 */
  let selectedAttachmentId = $state<string | null>(null);

  const presentation = $derived(data.presentation);
  const entitlements = $derived(data.entitlements);
  const error = $derived(data.error);
  const actionMessage = $derived(form?.message ?? null);
  const presentationVersion = $derived(presentation?.version ?? 1);

  // ── 白名单 Token 提取（未知 Token 一律不渲染） ──
  const tokens = $derived<Record<string, string | string[] | null>>(
    presentation?.presentation_tokens ?? {}
  );

  const nicknameColor = $derived.by(() => {
    const v = tokens['nickname_color'];
    return typeof v === 'string' && v in NICKNAME_COLORS ? v : null;
  });
  const nicknameDecoration = $derived.by(() => {
    const v = tokens['nickname_decoration'];
    return typeof v === 'string' && v in NICKNAME_DECORATIONS ? v : null;
  });
  const avatarFrame = $derived.by(() => {
    const v = tokens['avatar_frame'];
    return typeof v === 'string' && v in AVATAR_FRAMES ? AVATAR_FRAMES[v] : null;
  });
  const avatarAttachment = $derived.by(() => {
    const v = tokens['avatar_attachment'];
    return typeof v === 'string' && v in AVATAR_ATTACHMENTS ? v : null;
  });
  const titlePrefix = $derived.by(() => {
    const v = tokens['title_prefix'];
    return typeof v === 'string' && v in TITLE_PREFIXES ? v : null;
  });
  const profileEffect = $derived.by(() => {
    const v = tokens['profile_effect'];
    return typeof v === 'string' && v in PROFILE_EFFECTS ? v : null;
  });
  const postEffect = $derived.by(() => {
    const v = tokens['post_effect'];
    return typeof v === 'string' && v in POST_EFFECTS ? v : null;
  });
  const profileBadges = $derived.by(() => {
    const v = tokens['profile_badges'];
    if (!Array.isArray(v)) return [];
    return v.filter((b): b is string => typeof b === 'string' && b in BADGES);
  });

  /** 徽章最多 3 个（服务端裁决为主，前端仅作入口禁用提示）。 */
  const badgesAtLimit = $derived(profileBadges.length >= 3);

  /** 展示中昵称（颜色/装饰/前缀均来自白名单映射）。 */
  const displayName = $derived.by(() => {
    let name = '我';
    const deco = nicknameDecoration ? NICKNAME_DECORATIONS[nicknameDecoration] : null;
    if (titlePrefix) name = `${TITLE_PREFIXES[titlePrefix].prefix} ${name}`;
    if (deco) name = `${deco.prefix}${name}${deco.suffix}`;
    return name;
  });

  /** 槽位 id 集合（equipped 权益）。 */
  const equippedIds = $derived(new Set(entitlements.filter((e) => e.status === 'equipped').map((e) => e.id)));

  const equippable = $derived(
    entitlements.filter(
      (e) => e.status === 'owned' && e.remaining_quantity > 0 && (!e.expires_at || e.expires_at > Date.now())
    )
  );
  const expired = $derived(
    entitlements.filter((e) => e.status === 'expired' || (e.expires_at && e.expires_at <= Date.now()))
  );
  const revoked = $derived(entitlements.filter((e) => e.status === 'revoked' || e.status === 'consumed'));

  function canEquip(e: Entitlement): boolean {
    if (e.status !== 'owned' && e.status !== 'equipped') return false;
    if (e.slot === 'profile_badges' && e.status !== 'equipped' && badgesAtLimit) return false;
    return true;
  }

  function statusLabel(status: string): string {
    switch (status) {
      case 'owned':
        return '未装备';
      case 'equipped':
        return '已装备';
      case 'expired':
        return '已过期';
      case 'revoked':
        return '已撤销';
      case 'consumed':
        return '已使用';
      default:
        return status;
    }
  }

  function expiring(e: Entitlement): string | null {
    if (!e.expires_at) return null;
    const left = e.expires_at - Date.now();
    if (left <= 0) return '已过期';
    const days = Math.ceil(left / 86400000);
    return days > 1 ? `剩余 ${days} 天` : `剩余 ${Math.max(1, Math.round(left / 3600000))} 小时`;
  }
</script>

<svelte:head>
  <title>我的衣柜 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <a href="/me" class="breadcrumb-link">我的主页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">我的衣柜</span>
  </nav>

  {#if error}
    <p class="input-hint is-error" role="alert">{error}</p>
  {/if}
  {#if actionMessage}
    <p class="input-hint is-error" role="alert">{actionMessage}</p>
  {/if}

  <div class="content-grid" style="margin-top:0;">
    <div class="main-col">
      <!-- 预览 -->
      <div class="card" style="margin-bottom:var(--space-4);">
        <div class="card-header"><span class="card-title">装扮预览</span></div>
        <div class="card-body">
          <div class="wardrobe-preview {profileEffect ? 'effect-' + profileEffect : ''}">
            <div class="avatar-stack">
              <div class="preview-avatar {avatarFrame ?? ''}">
                <span aria-hidden="true">😀</span>
                {#if avatarAttachment}
                  <span class="avatar-pendant" aria-hidden="true">{AVATAR_ATTACHMENTS[avatarAttachment]}</span>
                {/if}
              </div>
            </div>
            <div class="preview-name" style={nicknameColor ? `color:${NICKNAME_COLORS[nicknameColor]};` : ''}>
              {displayName}
            </div>
            <div class="preview-badges">
              {#if profileBadges.length === 0}
                <span class="text-secondary" style="font-size:var(--text-xs);">未装备徽章</span>
              {:else}
                {#each profileBadges as badge}
                  <span class="badge badge-success" title={BADGES[badge].label}>
                    {BADGES[badge].icon} {BADGES[badge].label}
                  </span>
                {/each}
              {/if}
            </div>
            {#if postEffect}
              <p class="input-hint" style="margin-top:var(--space-2);">帖子装饰：{postEffect}</p>
            {/if}
          </div>
          <p class="input-hint">
            Token 白名单渲染（昵称颜色/头像框/挂件/徽章/主页与帖子装饰）；未知或未授权的装扮不会显示。
          </p>
        </div>
      </div>

      <!-- 已装备槽位 -->
      <div class="card" style="margin-bottom:var(--space-4);">
        <div class="card-header"><span class="card-title">已装备</span></div>
        <div class="card-body" style="padding:0;">
          {#if entitlements.filter((e) => e.status === 'equipped').length === 0}
            <div style="padding:var(--space-4);">
              <EmptyState icon="palette" title="还没有装备装扮" desc="去商城挑选并购买喜欢的装扮" />
            </div>
          {:else}
            <div style="display:flex;flex-direction:column;">
              {#each entitlements.filter((e) => e.status === 'equipped') as e (e.id)}
                <div class="post-row" style="padding:var(--space-3);border-bottom:var(--border-default);display:flex;gap:var(--space-3);align-items:center;">
                  <div style="min-width:0;flex:1;">
                    <strong>{e.product_title ?? e.product_id}</strong>
                    <span class="badge badge-neutral" style="margin-left:var(--space-2);">{slotLabel(e.slot ?? '')}</span>
                    <p class="text-secondary" style="font-size:var(--text-sm);margin:2px 0 0;">
                      {#if expiring(e)}
                        {expiring(e)}
                      {:else}
                        永久
                      {/if}
                    </p>
                  </div>
                  <form method="POST" action="?/unequip" use:enhance>
                    <input type="hidden" name="entitlement_id" value={e.id} />
                    <input type="hidden" name="expected_presentation_version" value={presentationVersion} />
                    <Button text="卸下" variant="ghost" size="sm" type="submit" />
                  </form>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      </div>

      <!-- 可装备 -->
      <div class="card" style="margin-bottom:var(--space-4);">
        <div class="card-header">
          <span class="card-title">可装备（{equippable.length}）</span>
          <a class="btn btn-primary btn-sm" href="/shop">去商城</a>
        </div>
        <div class="card-body" style="padding:0;">
          {#if equippable.length === 0}
            <div style="padding:var(--space-4);">
              <EmptyState icon="sparkles" title="暂无可装备的装扮" desc="购买后装扮会出现在这里" />
            </div>
          {:else}
            <div style="display:flex;flex-direction:column;">
              {#each equippable as e (e.id)}
                <div class="post-row" style="padding:var(--space-3);border-bottom:var(--border-default);display:flex;gap:var(--space-3);align-items:center;">
                  <div style="min-width:0;flex:1;">
                    <strong>{e.product_title ?? e.product_id}</strong>
                    <span class="badge badge-neutral" style="margin-left:var(--space-2);">{slotLabel(e.slot ?? '')}</span>
                    {#if e.slot === 'profile_badges' && e.remaining_quantity > 1}
                      <span class="badge badge-neutral">×{e.remaining_quantity}</span>
                    {/if}
                    {#if expiring(e)}
                      <span class="badge badge-warning">{expiring(e)}</span>
                    {/if}
                    <p class="text-secondary" style="font-size:var(--text-sm);margin:2px 0 0;">{statusLabel(e.status)}</p>
                  </div>
                  {#if canEquip(e)}
                    <form method="POST" action="?/equip" use:enhance>
                      <input type="hidden" name="entitlement_id" value={e.id} />
                      <input type="hidden" name="expected_presentation_version" value={presentationVersion} />
                      <Button
                        text={e.slot === 'profile_badges' && badgesAtLimit ? '徽章已满（≤3）' : '装备'}
                        variant="secondary"
                        size="sm"
                        type="submit"
                        disabled={e.slot === 'profile_badges' && badgesAtLimit}
                      />
                    </form>
                  {:else if e.slot === 'profile_badges' && badgesAtLimit}
                    <span class="text-secondary" style="font-size:var(--text-sm);">徽章最多 3 个</span>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}
        </div>
      </div>

      <!-- 已过期 / 已撤销 -->
      {#if expired.length > 0 || revoked.length > 0}
        <div class="card">
          <div class="card-header"><span class="card-title">历史（{expired.length + revoked.length}）</span></div>
          <div class="card-body" style="padding:0;">
            <div style="display:flex;flex-direction:column;">
              {#each [...expired, ...revoked] as e (e.id)}
                <div class="post-row" style="padding:var(--space-3);border-bottom:var(--border-default);display:flex;gap:var(--space-3);align-items:center;">
                  <div style="min-width:0;flex:1;">
                    <span>{e.product_title ?? e.product_id}</span>
                    <span class="badge badge-neutral" style="margin-left:var(--space-2);">{slotLabel(e.slot ?? '')}</span>
                    <span class="badge badge-warning" style="margin-left:var(--space-2);">{statusLabel(e.status)}</span>
                  </div>
                  {#if e.status === 'expired'}
                    <span class="text-secondary" style="font-size:var(--text-sm);">已到期自动卸下</span>
                  {/if}
                </div>
              {/each}
            </div>
          </div>
        </div>
      {/if}

      <!-- Reaction 演示（M07-UI-07：独立组件 + /me 演示；接入帖子/评论页后续） -->
      <div class="card" style="margin-top:var(--space-4);">
        <div class="card-header"><span class="card-title">反应组件演示</span></div>
        <div class="card-body">
          <ReactionBar
            targetType="post"
            targetId="demo"
            reactions={[
              { reaction: '👍', count: 3, active: false },
              { reaction: '🎉', count: 1, active: true },
              { reaction: '👏', count: 0, active: false }
            ]}
          />
        </div>
      </div>

      <!-- 附件上传/选择演示（M06-UI-01..04：独立 uploader/picker + /me 集成；
           Cover/头像/封面引用选择本人 ready 附件） -->
      <div class="card" style="margin-top:var(--space-4);">
        <div class="card-header">
          <span class="card-title">头像 / Cover 附件</span>
        </div>
        <div class="card-body">
          <AttachmentUploader
            accept="image/*"
            waitReady={false}
            label="选择图片"
            onReady={(a) => {
              selectedAttachmentId = a.id;
            }}
          />
          <p class="input-hint" style="margin-top:var(--space-2);">已就绪附件（选择后可引用为头像/Cover）：</p>
          <AttachmentPicker
            accept="image/*"
            selectedId={selectedAttachmentId}
            onSelect={(a) => {
              selectedAttachmentId = a.id;
            }}
          />
          <p class="input-hint" style="margin-top:var(--space-2);">
            头像/Cover 引用只接受本人已就绪附件；预览走稳定内容端点，签名 URL 到期自动重取。
          </p>
        </div>
      </div>
    </div>

    <div class="side-col">
      <div class="card">
        <div class="card-header"><span class="card-title">关于衣柜</span></div>
        <div class="card-body">
          <ul class="auth-hint" style="margin:0;padding-left:var(--space-4);display:flex;flex-direction:column;gap:var(--space-2);">
            <li>购买后在此装备；同一槽位一件，徽章最多 3 个。</li>
            <li>限时装扮到期自动卸下，持有历史保留。</li>
            <li>他人页面的装扮可在“设置”中关闭，并支持减少动效偏好。</li>
          </ul>
        </div>
      </div>
    </div>
  </div>
</div>

<style>
  .wardrobe-preview {
    border: 1px solid var(--color-border, #d0d7de);
    border-radius: var(--radius-md, 8px);
    padding: var(--space-5);
    text-align: center;
    background: var(--color-surface, #fff);
  }
  .avatar-stack {
    display: inline-flex;
    position: relative;
  }
  .preview-avatar {
    position: relative;
    width: 72px;
    height: 72px;
    border-radius: 50%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 36px;
    background: var(--color-bg-subtle, #f6f8fa);
    border: 2px solid var(--color-border, #d0d7de);
    overflow: visible;
  }
  .preview-avatar.avatar-frame-gold {
    border: 4px solid #d4a017;
    box-shadow: 0 0 12px rgba(212, 160, 23, 0.6);
  }
  .preview-avatar.avatar-frame-blue {
    border: 4px solid #0969da;
    box-shadow: 0 0 12px rgba(9, 105, 218, 0.5);
  }
  .preview-avatar.avatar-frame-glow {
    border: 3px solid #8250df;
    animation: avatar-pulse 3s ease-in-out infinite;
  }
  .avatar-pendant {
    position: absolute;
    right: -8px;
    bottom: -4px;
    font-size: 22px;
  }
  .preview-name {
    margin-top: var(--space-3);
    font-size: var(--text-lg);
    font-weight: 700;
  }
  .preview-badges {
    display: flex;
    justify-content: center;
    gap: var(--space-2);
    margin-top: var(--space-2);
    flex-wrap: wrap;
  }
  .effect-sparkle {
    background:
      radial-gradient(circle at 20% 30%, rgba(255, 215, 0, 0.15), transparent 40%),
      radial-gradient(circle at 80% 70%, rgba(9, 105, 218, 0.12), transparent 40%);
  }
  .effect-dark-stars {
    background:
      radial-gradient(circle at 30% 40%, rgba(130, 80, 223, 0.15), transparent 45%),
      radial-gradient(circle at 70% 60%, rgba(9, 105, 218, 0.12), transparent 45%);
  }
  @keyframes avatar-pulse {
    0%, 100% { box-shadow: 0 0 8px rgba(130, 80, 223, 0.4); }
    50% { box-shadow: 0 0 20px rgba(130, 80, 223, 0.9); }
  }
  @media (prefers-reduced-motion: reduce) {
    .preview-avatar.avatar-frame-glow {
      animation: none;
    }
  }
</style>
