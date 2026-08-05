<script lang="ts">
  // M03-UI-02：/settings 页——资料编辑 SSR 表单（无 JS 可提交）+ If-Match
  // 乐观并发 + 字段错误 + 保存后投影刷新。
  // - load 已取 user（含 version），form action `?/profile` 走服务端代理；
  // - 原生 form[method=POST] + use:enhance 渐进增强；
  // - 版本冲突（409）→ 提示横幅 + “加载最新资料”入口（enhance 下
  //   invalidateAll，无 JS 下整页刷新）；
  // - 保存成功 → form.user（更新后投影）直接渲染，use:enhance 默认
  //   invalidateAll 使 data 同步新版本；
  // - 只输出本人公开/账号字段，不输出任何会话 token（SSR 守卫见
  //   settings-nojs.test）。
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import Button from '$lib/components/ui/Button.svelte';
  import Card from '$lib/components/ui/Card.svelte';
  import { PROFILE_TEXT_LIMITS } from '$lib/profile';
  import type { SettingsFormResult, SettingsPageData } from './+page.server';

  let { data, form }: { data: SettingsPageData; form?: SettingsFormResult } = $props();

  const error = $derived(data.error);
  // 保存成功后优先展示 action 返回的新投影；否则用 load 数据。
  const user = $derived(form?.user ?? data.user);
  const conflict = $derived(form?.conflict === true);
  const topMessage = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );

  const limit = PROFILE_TEXT_LIMITS;
</script>

<svelte:head>
  <title>账号设置 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">账号设置</span>
  </nav>

  <div class="settings-layout">
    <nav class="settings-nav" aria-label="设置导航">
      <a href="/settings" class="settings-nav-item is-active">基本资料</a>
      <a href="/me" class="settings-nav-item">我的主页</a>
      <a href="/notifications" class="settings-nav-item">通知</a>
    </nav>

    <div class="settings-content">
      {#if error && !user}
        <p class="input-hint is-error" role="alert">{error}</p>
      {/if}

      {#if user}
        <form
          class="card"
          method="POST"
          action="?/profile"
          use:enhance={() => {
            return async ({ result, update }) => {
              await update();
              // 保存成功（非 fail）→ 刷新 load 数据，让投影与版本保持最新。
              if (result.type === 'success') await invalidateAll();
            };
          }}
        >
          <input type="hidden" name="version" value={user.version} />

          <div class="card-header"><span class="card-title">基本资料</span></div>
          <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-4);">
            {#if conflict}
              <div class="alert alert-warning" role="alert" style="padding:var(--space-3);border:1px solid var(--color-warning);border-radius:var(--radius-md);">
                <p style="margin:0 0 var(--space-2);">资料已在其他窗口被修改（版本冲突）。请加载最新资料后再编辑，避免覆盖他人修改。</p>
                <div style="display:flex;gap:var(--space-2);">
                  <a class="btn btn-primary btn-sm" href="/settings">加载最新资料</a>
                </div>
              </div>
            {/if}
            {#if topMessage && !conflict}
              <p class="input-hint is-error" role="alert">{topMessage}</p>
            {/if}

            <div class="input-wrapper">
              <label class="input-label" for="set-display-name">昵称</label>
              <input
                type="text"
                class="input-field"
                id="set-display-name"
                name="display_name"
                value={user.display_name ?? ''}
                placeholder="显示昵称"
                maxlength={limit.display_name}
              />
              <p class="input-hint">用于帖子、回复和主页展示；留空则使用用户名（最多 {limit.display_name} 字）。</p>
            </div>

            <div class="input-wrapper">
              <label class="input-label" for="set-bio">简介</label>
              <textarea
                class="input-field"
                id="set-bio"
                name="bio"
                rows="4"
                placeholder="简单介绍一下自己"
                maxlength={limit.bio}
              >{user.bio ?? ''}</textarea>
              <p class="input-hint">显示在你的主页；封禁/注销中不对外展示（最多 {limit.bio} 字）。</p>
            </div>

            <div class="input-wrapper">
              <label class="input-label" for="set-signature">签名</label>
              <input
                type="text"
                class="input-field"
                id="set-signature"
                name="signature"
                value={user.signature ?? ''}
                placeholder="帖子下方展示的签名"
                maxlength={limit.signature}
              />
              <p class="input-hint">显示在你帖子与回复的下方（最多 {limit.signature} 字）。</p>
            </div>

            <div>
              <Button text="保存修改" variant="primary" size="sm" type="submit" />
            </div>
          </div>
        </form>

        <Card>
          <div class="card-header"><span class="card-title">当前公开投影</span></div>
          <div class="card-body">
            <dl class="profile-about-list">
              <div class="profile-about-item"><dt>昵称</dt><dd>{user.display_name || user.username}</dd></div>
              <div class="profile-about-item"><dt>用户名</dt><dd>{user.username}</dd></div>
              {#if user.bio}<div class="profile-about-item"><dt>简介</dt><dd>{user.bio}</dd></div>{/if}
              {#if user.signature}<div class="profile-about-item"><dt>签名</dt><dd>{user.signature}</dd></div>{/if}
            </dl>
            <p class="input-hint">保存后主页与资料卡将按此公开投影展示（版本 v{user.version}）。</p>
          </div>
        </Card>

        <div class="card" style="margin-top:var(--space-4);">
          <div class="card-header"><span class="card-title">账号信息</span></div>
          <div class="card-body">
            <dl class="profile-about-list">
              <div class="profile-about-item"><dt>邮箱</dt><dd>{user.email}</dd></div>
              <div class="profile-about-item"><dt>状态</dt><dd>{user.status}</dd></div>
              <div class="profile-about-item"><dt>等级</dt><dd>LV.{user.level}</dd></div>
            </dl>
          </div>
        </div>
      {:else if !error}
        <div class="empty-state"><div class="empty-state-title">加载中…</div></div>
      {/if}
    </div>
  </div>
</div>
