<!-- P1 整改：/admin/moderation/risk —— 风险审核策略编辑（版本化 + 审计）。
     规则（M05-RISK）：新用户前 N 帖 / 链接数 / 敏感词 / 频率 / 重复内容；
     命中进入人工队列（pending_review），不自动封禁。 -->
<script lang="ts">
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { toastActionResult } from '$lib/ui/action-toast';
  import type { AdminRiskPageData, AdminRiskActionData } from './+page.server';

  let { data, form }: { data: AdminRiskPageData; form?: AdminRiskActionData | null } = $props();

  let hasJs = $state(false);
  let isSubmitting = $state(false);
  $effect(() => {
    hasJs = true;
  });

  const message = $derived(form?.message ?? null);
  const conflict = $derived(form?.conflict === true);
  const t = $derived(data.thresholds);
</script>

<svelte:head>
  <title>风控策略 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="风控策略" />

{#if form?.message && !hasJs}
  <div class="alert alert-info" role="status" style="margin-bottom:12px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);font-size:13px;">
    {form.message}
  </div>
{/if}
{#if conflict}
  <div class="app-error" role="alert" style="margin-bottom:12px;padding:10px 14px;">
    {form?.message}
  </div>
{/if}

{#if data.state === 'ok' && t}
  <form
    method="POST"
    action="?/save"
    use:enhance={() => {
      isSubmitting = true;
      return async ({ result, update }) => {
        isSubmitting = false;
        toastActionResult(result);
        if (result.type === 'failure' && (result.data as any)?.conflict) {
          await invalidateAll();
        } else {
          await update();
        }
      };
    }}
  >
    <input type="hidden" name="expected_version" value={data.version} />
    <section class="app-card" style="margin-bottom:14px;">
      <header class="app-card__head">
        <h2>发布风险规则</h2>
        <span class="app-muted" style="font-size:12px;">当前版本 v{data.version}（每次保存生成新版本并写审计）</span>
      </header>
      <div class="app-card__body" style="display:grid;gap:14px;">
        <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:12px;">
          <label>
            <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:4px;display:block;">新用户前 N 帖进入待审</span>
            <input type="number" name="new_user_max_posts" class="input-field" min="0" value={t.new_user_max_posts} required />
          </label>
          <label>
            <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:4px;display:block;">新用户判定窗口（秒）</span>
            <input type="number" name="new_user_grace_secs" class="input-field" min="0" value={t.new_user_grace_secs} required />
          </label>
          <label>
            <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:4px;display:block;">正文链接数上限</span>
            <input type="number" name="max_links" class="input-field" min="0" value={t.max_links} required />
          </label>
        </div>
        <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:12px;">
          <label>
            <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:4px;display:block;">频率窗口（秒）</span>
            <input type="number" name="frequency_window_secs" class="input-field" min="1" value={t.frequency_window_secs} required />
          </label>
          <label>
            <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:4px;display:block;">窗口内最多发帖数</span>
            <input type="number" name="max_frequency_posts" class="input-field" min="0" value={t.max_frequency_posts} required />
          </label>
          <label>
            <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:4px;display:block;">重复内容判定窗口（秒）</span>
            <input type="number" name="duplicate_window_secs" class="input-field" min="1" value={t.duplicate_window_secs} required />
          </label>
        </div>
        <label>
          <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:4px;display:block;">敏感词（逗号/换行分隔；仅内部匹配，作者只看到类别）</span>
          <textarea name="sensitive_words" class="input-field" rows="3" placeholder="词1, 词2, …">{t.sensitive_words.join(', ')}</textarea>
        </label>
        <p class="app-muted" style="font-size:12px;margin:0;">
          命中规则的内容进入人工审核队列（pending_review），不会自动封禁；AI 建议只作参考，超时/失败按规则结果兜底。
        </p>
      </div>
      <footer class="app-card__foot" style="display:flex;align-items:center;justify-content:flex-end;gap:10px;padding:12px 16px;">
        <label style="flex:1;min-width:220px;">
          <span class="app-muted" style="display:block;font-size:11px;margin-bottom:4px;">操作原因（写入审计日志，必填）</span>
          <input type="text" name="reason" class="input-field" required placeholder="如：收紧垃圾广告规则" />
        </label>
        <Button text={isSubmitting ? '保存中...' : '保存策略'} variant="primary" size="sm" type="submit" disabled={isSubmitting} />
      </footer>
    </section>
  </form>
{:else if data.state === 'forbidden'}
  <section class="app-card">
    <div class="app-card__body">
      <EmptyState icon="shield" title="无权限" desc="需要 admin.manage 权限管理风控策略" />
    </div>
  </section>
{:else}
  <section class="app-card">
    <div class="app-card__body">
      <p class="input-hint" role="alert">风控策略接口不可用（{data.state}）：{data.error}</p>
    </div>
  </section>
{/if}
