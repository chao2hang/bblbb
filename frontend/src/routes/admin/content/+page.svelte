<script lang="ts">
  // M18-ADMIN-CONTENT：内容审核版本对比页（对齐原型 #admin-article-audit）。
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import Button from '$lib/components/ui/Button.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import type { AdminContentActionData, AdminContentPageData } from './+page.server';

  let { data, form }: { data: AdminContentPageData; form?: AdminContentActionData | null } = $props();

  const posts = $derived(data.posts);
  const currentPost = $derived(posts[0] ?? null);

  let showRejectForm = $state(false);
  let rejectReason = $state('');
</script>

<svelte:head>
  <title>内容审核 — BBLBB Admin</title>
</svelte:head>

<div style="margin-bottom:12px;">
  <a href="/admin/posts" class="text-link" style="font-size:13px;display:inline-flex;align-items:center;gap:4px;">
    返回内容列表
  </a>
</div>

{#if form?.message}
  <div class="alert alert-success" role="status" style="margin-bottom:14px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-md);border-left:3px solid var(--color-success);">
    {form.message}
  </div>
{/if}
{#if form?.error}
  <div class="alert alert-danger" role="alert" style="margin-bottom:14px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-md);border-left:3px solid var(--color-danger);">
    {form.error}
  </div>
{/if}

{#if !currentPost}
  <div class="app-card">
    <div class="app-card__body">
      <EmptyState icon="file-text" title="没有待审核的内容" desc="当前没有需要对比审核的内容" />
    </div>
  </div>
{:else}
  <section class="app-card">
    <header class="app-card__head" style="padding:16px 20px;">
      <h2 style="margin:0;font-size:17px;font-weight:700;line-height:1.4;">
        版本对比 · {currentPost.title || '使用 SvelteKit 构建博客与轻量论坛是否合理？'}
      </h2>
    </header>

    <div class="app-card__body" style="padding:16px 20px;">
      <!-- 变动类别徽章 -->
      <div style="display:flex;gap:8px;margin-bottom:16px;">
        <span class="sbadge sb-success" style="padding:2px 8px;border-radius:4px;font-size:11px;background:#eaf5ec;color:#237804;">新增</span>
        <span class="sbadge sb-danger" style="padding:2px 8px;border-radius:4px;font-size:11px;background:#fff1f0;color:#cf1322;">删除</span>
        <span class="sbadge sb-hot" style="padding:2px 8px;border-radius:4px;font-size:11px;background:#fff7e6;color:#d46b08;">变更</span>
      </div>

      <!-- 双栏/纵向 diff 对比块（原型同款布局） -->
      <div style="display:grid;grid-template-columns:1fr;gap:14px;margin-bottom:20px;">
        <!-- 修改前 -->
        <div>
          <div style="font-size:12px;font-weight:600;color:var(--color-text-secondary);margin-bottom:6px;">修改前</div>
          <div style="background:var(--color-bg-subtle, rgba(0,0,0,0.02));padding:14px 16px;border-left:3px solid var(--color-text-secondary);border-radius:0 var(--radius-sm) var(--radius-sm) 0;font-size:14px;line-height:1.7;color:var(--color-text-primary);">
            这个月用 SvelteKit 写完 BBLBB 的核心页面，包括 SSR、SEO、CSRF 与表单接入。
          </div>
        </div>

        <!-- 修改后 -->
        <div>
          <div style="font-size:12px;font-weight:600;color:#237804;margin-bottom:6px;">修改后</div>
          <div style="background:#f6ffed;padding:14px 16px;border-left:3px solid #52c41a;border-radius:0 var(--radius-sm) var(--radius-sm) 0;font-size:14px;line-height:1.7;color:#135200;">
            这个月用 SvelteKit 写完 BBLBB 的核心页面，包括 SSR、SEO、CSRF、表单与 OIDC 接入，整体体验比想象中好。
          </div>
        </div>
      </div>

      <!-- 审核操作区 -->
      <div style="display:flex;gap:10px;align-items:center;flex-wrap:wrap;">
        <form
          method="POST"
          action="?/approve"
          use:enhance={() => {
            return async ({ result, update }) => {
              if (result.type === 'success') showToast('已通过审核', 'success');
              await update();
              await invalidateAll();
            };
          }}
          style="margin:0;"
        >
          <input type="hidden" name="id" value={currentPost.id} />
          <Button text="通过审核" variant="primary" type="submit" />
        </form>

        <button
          type="button"
          class="btn secondary"
          onclick={() => (showRejectForm = !showRejectForm)}
        >
          {showRejectForm ? '取消驳回' : '填写驳回理由'}
        </button>
      </div>

      {#if showRejectForm}
        <form
          method="POST"
          action="?/reject"
          use:enhance={() => {
            return async ({ result, update }) => {
              if (result.type === 'success') {
                showToast('已驳回', 'success');
                showRejectForm = false;
                rejectReason = '';
              }
              await update();
              await invalidateAll();
            };
          }}
          style="margin-top:14px;display:flex;gap:8px;align-items:center;flex-wrap:wrap;"
        >
          <input type="hidden" name="id" value={currentPost.id} />
          <input
            type="text"
            name="reason"
            bind:value={rejectReason}
            class="input-field"
            placeholder="填写驳回理由（必填，写入审计）"
            required
            style="flex:1;min-width:200px;"
          />
          <Button text="确认驳回" variant="danger" type="submit" />
        </form>
      {/if}
    </div>
  </section>
{/if}
