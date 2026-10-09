<script lang="ts">
  // M02-UX-05：个人中心固定身份条（≤767px 吸顶，桌面不渲染）。
  // 三个 tag 页（/me · /me/security · /me/level）共用：44px 头像 + 昵称
  // + @用户名 + 信任等级徽标，in-flow sticky 吸附在顶栏正下方，与分区
  // tag 栏叠成一条连续头部——切页与长列表滚动时身份恒定可见。
  // 纯展示（无交互元素），无 JS 依赖。
  import Avatar from '$lib/components/ui/Avatar.svelte';

  let {
    name,
    username,
    level = 0,
    avatarAttachmentId = null,
    seed = null
  }: {
    /** 展示昵称（display_name 优先，回退用户名）。 */
    name: string;
    username: string;
    level?: number;
    avatarAttachmentId?: string | null;
    seed?: string | null;
  } = $props();
</script>

<div class="me-identity-bar">
  <span class="me-identity-avatar">
    <Avatar name={name} size="sm" attachmentId={avatarAttachmentId} seed={seed} />
  </span>
  <span class="me-identity-name">{name}</span>
  <span class="me-identity-handle">@{username}</span>
  <span class="me-identity-spacer"></span>
  <span class="badge badge-level">TL{level}</span>
</div>

<style>
  /* 桌面默认隐藏：桌面端身份由侧栏 / 资料卡承担 */
  .me-identity-bar {
    display: none;
  }
  .me-identity-avatar :global(.avatar) {
    border-width: 1.5px;
  }
  .me-identity-name {
    min-width: 0;
    max-width: 36vw;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 14px;
    font-weight: var(--weight-semibold);
    color: var(--color-text-primary);
  }
  .me-identity-handle {
    flex-shrink: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    color: var(--color-text-tertiary);
  }
  .me-identity-spacer {
    flex: 1;
  }

  @media (max-width: 767px) {
    .me-identity-bar {
      display: flex !important;
      position: sticky;
      top: var(--mobile-header-h, 52px);
      z-index: 10;
      align-items: center;
      gap: 8px;
      width: 100%;
      height: 44px;
      margin: 0 0 10px;
      padding: 0 12px;
      background: var(--color-bg-card);
      border-bottom: 1px solid var(--color-border);
    }
    .me-identity-avatar {
      flex: 0 0 auto;
      display: inline-flex;
    }
    .me-identity-avatar :global(.avatar) {
      width: 24px !important;
      height: 24px !important;
      border-width: 1.5px !important;
    }
    .me-identity-handle {
      flex-shrink: 1;
    }
    .badge-level {
      flex: 0 0 auto;
    }
  }
</style>
