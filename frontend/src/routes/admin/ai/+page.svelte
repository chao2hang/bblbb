<script lang="ts">
  // M09-UI-06：管理端 AI——Provider 脱敏状态、预算、Flag 配置、任务
  // 重试/取消。所有写操作要求 reason（审计）；Secret 只显示布尔。
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import { aiTaskStatusLabel, aiPurposeLabel, aiDataModeLabel } from '$lib/api/client';
  import type { AdminAiActionData, AdminAiPageData } from './+page.server';

  let { data, form }: { data: AdminAiPageData; form?: AdminAiActionData | null } = $props();

  const state = $derived(data.state);
  const config = $derived(data.config);
  const tasks = $derived(data.tasks);
  const error = $derived(data.error);
  const message = $derived(form?.message ?? null);
  const conflict = $derived(form?.conflict === true);
  const testResult = $derived(form?.testResult ?? null);

  const flags = $derived(config?.flags ?? {});
</script>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <a href="/admin" class="breadcrumb-link">管理后台</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">AI 管理</span>
  </nav>

  {#if error && state !== 'not_implemented'}
    <p class="input-hint is-error" role="alert">{error}</p>
  {/if}
  {#if message}
    <p class="input-hint {conflict ? 'is-error' : ''}" role="alert">{message}</p>
  {/if}

  {#if state === 'not_implemented'}
    <div class="card">
      <div class="card-body">
        <p class="input-hint" role="status">AI 管理接口开发中（后端未实现）。核心论坛功能不受影响。</p>
      </div>
    </div>
  {:else if state === 'forbidden'}
    <div class="card">
      <div class="card-body">
        <p class="input-hint is-error" role="alert">你没有权限访问 AI 管理。</p>
      </div>
    </div>
  {:else if state === 'error' && !config}
    <div class="card">
      <div class="card-body">
        <p class="input-hint is-error" role="alert">加载失败：{error}</p>
      </div>
    </div>
  {:else if config}
    <div class="card" style="margin-bottom:var(--space-4);">
      <div class="card-header"><span class="card-title">AI 能力总开关与数据策略（v{config.version}）</span></div>
      <div class="card-body">
        <form method="POST" action="?/save" use:enhance>
          <input type="hidden" name="expected_version" value={config.version} />
          <div class="admin-form-grid" style="display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:var(--space-2);">
            <div class="input-wrapper">
              <span class="input-label">AI 能力（Feature Flag，默认关闭）</span>
              <label style="display:flex;align-items:center;gap:var(--space-2);">
                <input type="checkbox" name="enabled" checked={config.enabled} />
                启用 AI 能力
              </label>
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="ai-data-mode">数据发送策略</label>
              <select id="ai-data-mode" name="data_mode" class="input-field">
                <option value="disabled" selected={config.data_mode === 'disabled'}>disabled（不发送）</option>
                <option value="metadata_only" selected={config.data_mode === 'metadata_only'}>metadata_only（仅元数据）</option>
                <option value="redacted" selected={config.data_mode === 'redacted'}>redacted（脱敏）</option>
                <option value="full_with_consent" selected={config.data_mode === 'full_with_consent'}>full_with_consent（逐次同意）</option>
              </select>
              <p class="input-hint">当前：{aiDataModeLabel(config.data_mode)}。全站策略优先于作者选择。</p>
            </div>
          </div>

          <div class="input-wrapper" style="margin-top:var(--space-2);">
            <span class="input-label" id="ai-flags-label">功能 Flag</span>
            <div style="display:flex;flex-wrap:wrap;gap:var(--space-3);" role="group" aria-labelledby="ai-flags-label">
              <label style="display:flex;align-items:center;gap:var(--space-1);">
                <input type="checkbox" name="flag_formatting" checked={flags.formatting !== false} />
                格式化
              </label>
              <label style="display:flex;align-items:center;gap:var(--space-1);">
                <input type="checkbox" name="flag_seo" checked={flags.seo !== false} />
                SEO
              </label>
              <label style="display:flex;align-items:center;gap:var(--space-1);">
                <input type="checkbox" name="flag_tagging" checked={flags.tagging !== false} />
                标签建议
              </label>
              <label style="display:flex;align-items:center;gap:var(--space-1);">
                <input type="checkbox" name="flag_moderation" checked={flags.moderation !== false} />
                内容审核辅助
              </label>
            </div>
          </div>

          <div class="admin-form-grid" style="display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:var(--space-2);margin-top:var(--space-2);">
            <div class="input-wrapper">
              <label class="input-label" for="ai-budget-user">每用户每日 token 预算</label>
              <input id="ai-budget-user" name="budget_per_user_daily_tokens" type="number" min="0" class="input-field" value={config.budgets?.per_user_daily_tokens ?? ''} placeholder="不限" />
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="ai-budget-site">站点每日 token 预算</label>
              <input id="ai-budget-site" name="budget_site_daily_tokens" type="number" min="0" class="input-field" value={config.budgets?.site_daily_tokens ?? ''} placeholder="不限" />
            </div>
          </div>

          <div class="input-wrapper" style="margin-top:var(--space-2);">
            <label class="input-label" for="ai-reason">操作原因</label>
            <input id="ai-reason" name="reason" class="input-field" required placeholder="必填（写审计）" />
          </div>
          <div style="display:flex;gap:var(--space-2);margin-top:var(--space-2);">
            <Button text="保存配置" variant="primary" size="sm" type="submit" />
            <Button text="测试 Provider（当前表单值）" variant="secondary" size="sm" type="submit" formaction="?/test" />
          </div>
        </form>

        {#if testResult}
          <p class="input-hint {testResult.ok ? '' : 'is-error'}" role="status" style="margin-top:var(--space-2);">
            测试结果：{testResult.ok ? '连接成功' : `连接失败（${testResult.code ?? '未知'}）`} —— {testResult.message}
            {#if typeof testResult.elapsed_ms === 'number'}
              （{testResult.elapsed_ms} ms）
            {/if}
          </p>
        {/if}
      </div>
    </div>

    <div class="card" style="margin-bottom:var(--space-4);">
      <div class="card-header"><span class="card-title">Provider（脱敏状态）</span></div>
      <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-2);">
        {#if !config.providers || config.providers.length === 0}
          <p class="input-hint" style="margin:0;">尚未配置 Provider。</p>
        {:else}
          {#each config.providers as provider (provider.id)}
            <div style="border:var(--border-default);border-radius:var(--radius-md);padding:var(--space-2);display:flex;flex-wrap:wrap;gap:var(--space-2);align-items:center;">
              <strong>{provider.name ?? '未命名'}</strong>
              {#if provider.api_type}<span class="badge badge-neutral">{provider.api_type}</span>{/if}
              {#if provider.model}<span class="badge badge-neutral">{provider.model}</span>{/if}
              <span class="badge {provider.secret_configured ? 'badge-success' : 'badge-warning'}">
                {provider.secret_configured ? '密钥已配置' : '密钥未配置'}
              </span>
              <span class="badge {provider.available === false ? 'badge-warning' : 'badge-success'}">
                {provider.available === false ? '不可用' : '可用'}
              </span>
              {#if provider.purposes && provider.purposes.length > 0}
                <span class="text-secondary" style="font-size:var(--text-xs);">{provider.purposes.map(aiPurposeLabel).join('、')}</span>
              {/if}
            </div>
          {/each}
        {/if}
        <p class="input-hint" style="margin:0;">密钥只写入受保护 Secret Store，任何页面都不会显示明文或片段。</p>
      </div>
    </div>

    <div class="card">
      <div class="card-header"><span class="card-title">任务（重试/取消）</span></div>
      <div class="card-body" style="padding:0;">
        {#if tasks.length === 0}
          <p class="input-hint" style="padding:var(--space-3);margin:0;">暂无任务。任务失败不会阻塞普通发帖与人工审核。</p>
        {:else}
          <ul class="post-list" style="list-style:none;margin:0;padding:0;">
            {#each tasks as task (task.id)}
              <li style="padding:var(--space-3);border-bottom:var(--border-default);">
                <div style="display:flex;flex-wrap:wrap;gap:var(--space-2);align-items:center;">
                  <span class="text-secondary" style="font-size:var(--text-xs);">{task.id.slice(0, 8)}</span>
                  <span class="badge badge-neutral">{aiPurposeLabel(task.purpose ?? task.task_type)}</span>
                  <span class="badge {task.status === 'dead' ? 'badge-warning' : task.status === 'succeeded' ? 'badge-success' : 'badge-neutral'}" role="status">
                    {aiTaskStatusLabel(task.status)}
                  </span>
                  {#if task.user_id}
                    <span class="text-secondary" style="font-size:var(--text-xs);">用户 {task.user_id.slice(0, 8)}</span>
                  {/if}
                  {#if task.error_code}
                    <span class="text-secondary" style="font-size:var(--text-xs);">错误码 {task.error_code}</span>
                  {/if}
                </div>
                {#if task.status === 'dead' || task.status === 'retry_wait' || task.status === 'queued' || task.status === 'running'}
                  <div style="display:flex;flex-wrap:wrap;gap:var(--space-2);margin-top:var(--space-1);">
                    {#if task.status === 'dead' || task.status === 'retry_wait'}
                      <form method="POST" action="?/retry">
                        <input type="hidden" name="task_id" value={task.id} />
                        <input type="hidden" name="client_request_id" value={data.clientRequestId} />
                        <label class="input-label" style="font-size:var(--text-xs);" for="retry-reason-{task.id}">原因</label>
                        <input id="retry-reason-{task.id}" name="reason" class="input-field" style="font-size:var(--text-sm);padding:var(--space-1);" required placeholder="必填（写审计）" />
                        <Button text="重试" variant="secondary" size="sm" type="submit" />
                      </form>
                    {/if}
                    {#if task.status === 'queued' || task.status === 'running'}
                      <form method="POST" action="?/cancel">
                        <input type="hidden" name="task_id" value={task.id} />
                        <input type="hidden" name="client_request_id" value={data.clientRequestId} />
                        <label class="input-label" style="font-size:var(--text-xs);" for="cancel-reason-{task.id}">原因</label>
                        <input id="cancel-reason-{task.id}" name="reason" class="input-field" style="font-size:var(--text-sm);padding:var(--space-1);" required placeholder="必填（写审计）" />
                        <Button text="取消" variant="ghost" size="sm" type="submit" />
                      </form>
                    {/if}
                  </div>
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
        <p class="input-hint" style="padding:0 var(--space-3) var(--space-3);margin:0;">
          管理员端点不能扩大任务内容可见性；错误只显示稳定码与脱敏信息。
        </p>
      </div>
    </div>
  {/if}
</div>
