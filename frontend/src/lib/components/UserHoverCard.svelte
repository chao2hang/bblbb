<script lang="ts">
  // M03-PROFILE-09：用户 Hover Card —— 只接受并渲染公开投影字段。
  //
  // 隐私契约：本组件是用户信息的浮层卡片，props 类型只允许
  // `PublicProfile` 的公开字段（username/display_name/level），
  // 严禁传入邮箱、状态、凭据等私有字段；组件实现也只渲染这些公开字段。
  // SSR 泄漏测试见 frontend/src/lib/testing/ssr/privacy.test.ts。
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import type { PublicProfile } from '$lib/api/client';

  let {
    user
  }: {
    user: Pick<PublicProfile, 'username' | 'display_name' | 'level'>
  } = $props();

  const displayName = $derived(user.display_name || user.username);
  const profileUrl = $derived(`/users/${user.username}`);
</script>

<div class="user-hover-card" role="tooltip" aria-label="用户信息">
  <div class="user-hover-top">
    <Avatar name={displayName} size="sm" />
    <div class="user-hover-id">
      <span class="user-hover-name">{displayName}</span>
      <a class="user-hover-link" href={profileUrl}>@{user.username}</a>
    </div>
    <span class="badge badge-level">LV.{user.level}</span>
  </div>
</div>
