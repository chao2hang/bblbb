<script lang="ts">
  // M03-PROFILE-09 / M03-UI-03：用户 Hover Card —— 只接受并渲染公开投影字段。
  //
  // 隐私契约：本组件是用户信息的浮层卡片，props 类型只允许
  // `PublicProfile` 的公开字段（username/display_name/level/bio/signature），
  // 严禁传入邮箱、状态、凭据等私有字段；组件实现也只渲染这些公开字段。
  // level/bio/signature 允许缺省：列表/搜索行只有部分公开投影
  // （author.username + display_name），缺省字段不渲染而非伪造。
  // SSR 泄漏测试见 frontend/src/lib/testing/ssr/privacy.test.ts。
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import ProfileCover from '$lib/components/ui/ProfileCover.svelte';
  import type { PublicProfile } from '$lib/api/client';

  /** 触发卡公开字段（严格 allowlist；level/bio/signature 可缺省）。 */
  export type HoverCardUser = Pick<PublicProfile, 'username' | 'display_name'> &
    Partial<Pick<PublicProfile, 'level' | 'bio' | 'signature'>>;

  let {
    user
  }: {
    user: HoverCardUser;
  } = $props();

  const displayName = $derived(user.display_name || user.username);
  const profileUrl = $derived(`/users/${user.username}`);
</script>

<div class="user-hover-card" role="dialog" aria-label="{displayName} 的个人资料">
  <ProfileCover class="user-hover-cover" />
  <div class="user-hover-body">
    <div class="user-hover-avatar">
      <Avatar name={displayName} size="lg" />
    </div>
    <div class="user-hover-name">
      <span class="user-hover-name-text">{displayName}</span>
      {#if typeof user.level === 'number'}
        <span class="badge badge-level">LV.{user.level}</span>
      {/if}
    </div>
    <a class="user-hover-username" href={profileUrl}>@{user.username}</a>
    <p class="user-hover-bio">{user.bio || '这个人还没有填写个人简介。'}</p>
    <div class="user-hover-actions">
      <a class="user-hover-link" href={profileUrl}>查看个人主页</a>
    </div>
  </div>
</div>
