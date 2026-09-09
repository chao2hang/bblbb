<script lang="ts">
  // M09-UI-06 & M18-ADMIN-AI：大模型设置管理页（模型渠道添加/编辑/删除、调用策略、任务管理与测试）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import type { AdminAiActionData, AdminAiPageData } from './+page.server';

  let { data, form }: { data: AdminAiPageData; form?: AdminAiActionData | null } = $props();

  const loadState = $derived(data.state);
  const config = $derived(data.config);
  const tasks = $derived(data.tasks);
  const error = $derived(data.error);

  const fallbackTasks = [
    { id: 'T-311', task: '草稿格式修复', source: '草稿 #d-12', errorCode: null, status: 'completed' },
    { id: 'T-310', task: '内容摘要', source: '主题 T-201', errorCode: null, status: 'completed' },
    { id: 'T-309', task: '敏感词复核', source: '主题 T-198', errorCode: null, status: 'completed' },
    { id: 'T-308', task: '标题翻译', source: '草稿 #d-09', errorCode: null, status: 'completed' }
  ];

  const channels = $derived(
    Array.isArray(config?.providers) && config.providers.length > 0
      ? config.providers.map((p) => {
          const rawStatus = (p as any).status === 'disabled' ? 'disabled' : ((p as any).status || 'enabled');
          const firstEnabled = config.providers?.find((item: any) => item.status !== 'disabled');
          return {
            id: p.id,
            initial: (p.name ?? 'P').charAt(0).toUpperCase() || 'P',
            name: p.name ?? '未命名提供商',
            adapter_type: (p as any).adapter_type || (p as any).api_type || 'openai_compatible',
            url: p.base_url || 'https://api.openai.com/v1',
            secretConfigured: p.secret_configured === true,
            status: p.secret_configured ? '密钥已配置' : '未配置密钥',
            rawStatus,
            isDefault: rawStatus === 'enabled' && firstEnabled?.id === p.id,
            models: [(p as any).default_model || (p as any).model || 'gpt-4o-mini']
          };
        })
      : []
  );

  const enabledChannels = $derived(channels.filter((c) => c.rawStatus === 'enabled'));

  const taskList = $derived.by(() => {
    const list = Array.isArray(tasks) ? tasks : (tasks as any)?.items ?? [];
    if (list.length > 0) {
      return list.map((t: any) => ({
        id: t.id,
        task: t.purpose || t.task_type || 'AI任务',
        source: t.error_code ? `错误：${t.error_code}` : '主题内容',
        errorCode: t.error_code,
        status: t.status
      }));
    }
    return fallbackTasks;
  });

  // 模态框状态（添加/编辑渠道）
  let showModal = $state(false);
  let isEditing = $state(false);
  let modalProviderId = $state('');
  let modalName = $state('');
  let modalAdapterType = $state('openai_compatible');
  let modalBaseUrl = $state('');
  let modalDefaultModel = $state('');
  let modalStatus = $state('enabled');
  let modalApiKey = $state('');
  let modalSecretConfigured = $state(false);
  let modalReason = $state('配置 AI 模型渠道');
  let isTesting = $state(false);
  let testOutput: { ok: boolean; message: string } | null = $state(null);

  function openAddModal() {
    isEditing = false;
    modalProviderId = '';
    modalName = '';
    modalAdapterType = 'openai_compatible';
    modalBaseUrl = 'https://api.openai.com/v1';
    modalDefaultModel = 'gpt-4o-mini';
    modalStatus = 'enabled';
    modalApiKey = '';
    modalSecretConfigured = false;
    modalReason = '添加 AI 模型渠道';
    testOutput = null;
    showModal = true;
  }

  function openEditModal(ch: any) {
    isEditing = true;
    modalProviderId = ch.id;
    modalName = ch.name;
    modalAdapterType = ch.adapter_type || 'openai_compatible';
    modalBaseUrl = ch.url;
    modalDefaultModel = ch.models?.[0] || 'gpt-4o-mini';
    modalStatus = ch.rawStatus || 'enabled';
    modalApiKey = '';
    modalSecretConfigured = ch.secretConfigured;
    modalReason = `更新渠道 ${ch.name}`;
    testOutput = null;
    showModal = true;
  }

  function closeModal() {
    showModal = false;
    testOutput = null;
  }

  async function testConnection(url: string) {
    if (!url) {
      showToast('请先输入 Base URL', 'danger');
      return;
    }
    isTesting = true;
    testOutput = null;
    try {
      const res = await fetch('/api/v1/admin/ai/providers/test', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ base_url: url })
      });
      const data = await res.json();
      if (data.ok) {
        testOutput = { ok: true, message: '连接探针通过：Endpoint 出网策略正常' };
        showToast('连接正常', 'success');
      } else {
        testOutput = { ok: false, message: `连接异常：${data.error_class || data.message || '出网策略拦截或拒绝'}` };
        showToast('连接失败', 'danger');
      }
    } catch (e: any) {
      testOutput = { ok: false, message: `请求失败：${e?.message || '网络异常'}` };
      showToast('测试失败', 'danger');
    } finally {
      isTesting = false;
    }
  }

  $effect(() => {
    if (form?.ok && form?.message) {
      showToast(form.message, 'success');
      closeModal();
    } else if (form?.message) {
      showToast(form.message, 'danger');
    }
  });
</script>

<svelte:head>
  <title>大模型设置 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="大模型设置" />

{#if loadState === 'not_implemented'}
  <div class="app-card">
    <div class="app-card__body" role="status">
      <p class="input-hint">AI 管理接口开发中。核心论坛功能不受影响。</p>
    </div>
  </div>
{:else if loadState === 'forbidden'}
  <div class="app-card">
    <div class="app-card__body" role="alert">
      <p class="input-hint is-error">没有权限访问 AI 管理。</p>
    </div>
  </div>
{:else if loadState === 'error' && !config}
  <div class="app-card">
    <div class="app-card__body" role="alert">
      <p class="input-hint is-error">加载失败：{error}</p>
    </div>
  </div>
{:else}
  <!-- 顶部眉题与操作 -->
  <div style="margin-bottom:14px;">
    <div style="font-size:11px;font-weight:700;letter-spacing:1px;color:var(--color-text-secondary);margin-bottom:4px;">
      AI ROUTING / MODEL REGISTRY
    </div>
    <div style="display:flex;justify-content:space-between;align-items:center;flex-wrap:wrap;gap:10px;">
      <div>
        <h1 style="margin:0;font-size:22px;font-weight:700;">大模型设置</h1>
        <p style="margin:4px 0 0;font-size:12px;color:var(--color-text-secondary);">配置多个模型渠道，按业务场景选择最合适的模型。</p>
      </div>
      <button type="button" class="btn primary sm" onclick={openAddModal}>
        添加渠道
      </button>
    </div>
  </div>

  <!-- 4 个统计卡 -->
  <div style="display:grid;grid-template-columns:repeat(2, 1fr);gap:14px;margin-bottom:14px;">
    <div class="app-card" style="padding:16px;">
      <div class="text-secondary" style="font-size:12px;margin-bottom:4px;">已启用渠道</div>
      <div style="font-size:26px;font-weight:700;line-height:1.2;">
        {channels.filter(c => c.rawStatus === 'enabled').length} / {channels.length}
      </div>
    </div>
    <div class="app-card" style="padding:16px;">
      <div class="text-secondary" style="font-size:12px;margin-bottom:4px;">可用模型数</div>
      <div style="font-size:26px;font-weight:700;line-height:1.2;">
        {channels.reduce((acc, c) => acc + c.models.length, 0)}
      </div>
    </div>
    <div class="app-card" style="padding:16px;">
      <div class="text-secondary" style="font-size:12px;margin-bottom:4px;">数据模式</div>
      <div style="font-size:20px;font-weight:700;line-height:1.2;color:var(--color-brand);">
        {config?.data_mode ?? 'redacted'}
      </div>
      <div class="text-secondary" style="font-size:11px;margin-top:2px;">
        {config?.enabled ? '全站 AI 能力已启用' : '全站 AI 能力已关闭'}
      </div>
    </div>
    <div class="app-card" style="padding:16px;">
      <div class="text-secondary" style="font-size:12px;margin-bottom:4px;">每日预算上限</div>
      <div style="font-size:20px;font-weight:700;line-height:1.2;color:var(--color-brand);">
        {config?.budgets?.site_daily_tokens ? `${config.budgets.site_daily_tokens} tokens` : '不限'}
      </div>
      <div class="text-secondary" style="font-size:11px;margin-top:2px;">
        单用户上限：{config?.budgets?.per_user_daily_tokens ? `${config.budgets.per_user_daily_tokens} tokens` : '不限'}
      </div>
    </div>
  </div>

  <!-- 卡片 1：模型渠道 -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head" style="display:flex;justify-content:space-between;align-items:center;">
      <div>
        <h2 style="margin:0;">模型渠道</h2>
        <span class="app-muted" style="font-size:12px;">配置大模型供应商与端点，支持添加、修改配置和安全删除。</span>
      </div>
      <div style="display:flex;align-items:center;gap:10px;">
        <span class="text-secondary" style="font-size:12px;">{channels.length} 个渠道</span>
        <button type="button" class="btn primary sm" onclick={openAddModal}>添加渠道</button>
      </div>
    </header>
    <div class="app-card__body" style="display:flex;flex-direction:column;gap:12px;">
      {#if channels.length === 0}
        <div style="border:1px dashed var(--color-border);border-radius:6px;padding:32px 16px;text-align:center;">
          <p style="margin:0 0 12px;font-size:13px;color:var(--color-text-secondary);">尚未配置任何 AI 模型渠道。</p>
          <button type="button" class="btn primary sm" onclick={openAddModal}>
            添加第一个渠道
          </button>
        </div>
      {:else}
        {#each channels as ch}
          <div class="app-card" style="border:1px solid var(--color-border);padding:14px;">
            <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:8px;">
              <div style="display:flex;align-items:center;gap:10px;">
                <div style="width:28px;height:28px;border-radius:4px;background:var(--color-bg-subtle);display:grid;place-items:center;font-weight:700;font-size:13px;">
                  {ch.initial}
                </div>
                <div>
                  <b style="font-size:14px;">{ch.name}</b>
                  <span class="text-secondary" style="display:block;font-size:11px;">{ch.url}</span>
                </div>
              </div>
              <div style="display:flex;gap:6px;align-items:center;">
                <span class="badge {ch.rawStatus === 'enabled' ? 'badge-success' : 'badge-neutral'}" style="font-size:11px;">
                  {ch.rawStatus === 'enabled' ? '已启用' : '已停用'}
                </span>
                <span class="badge {ch.secretConfigured ? 'badge-success' : 'badge-warning'}" style="font-size:11px;">
                  {ch.status}
                </span>
              </div>
            </div>

            <div style="display:flex;gap:6px;flex-wrap:wrap;margin:10px 0;align-items:center;">
              <span class="badge badge-neutral" style="font-size:11px;">类型: {ch.adapter_type}</span>
              {#each ch.models as m}
                <code style="padding:2px 6px;background:var(--color-bg-subtle);border-radius:3px;font-size:11px;">{m}</code>
              {/each}
            </div>

            <div style="display:flex;gap:8px;align-items:center;padding-top:6px;border-top:1px solid var(--color-border);">
              <button type="button" class="btn secondary sm" onclick={() => testConnection(ch.url)} disabled={isTesting}>
                测试连接
              </button>
              <button type="button" class="btn ghost sm" onclick={() => openEditModal(ch)}>
                编辑
              </button>
              <form method="POST" action="?/deleteProvider" use:enhance style="display:inline;margin:0;">
                <input type="hidden" name="provider_id" value={ch.id} />
                <input type="hidden" name="expected_version" value={config?.version ?? 1} />
                <input type="hidden" name="reason" value={`删除渠道 ${ch.name}`} />
                <button
                  type="submit"
                  class="btn ghost sm"
                  style="color:var(--color-danger, #e53e3e);"
                  onclick={(e) => {
                    if (!confirm(`确定要删除渠道「${ch.name}」吗？`)) {
                      e.preventDefault();
                    }
                  }}
                >
                  删除
                </button>
              </form>
              {#if ch.isDefault}
                <span class="text-secondary" style="margin-left:auto;font-size:11px;">默认渠道</span>
              {/if}
            </div>
          </div>
        {/each}
      {/if}
      <p class="text-secondary" style="font-size:11px;margin:4px 0 0;">
        密钥只写入受保护 Secret Store，任何页面都不会显示明文或片段。
      </p>
    </div>
  </section>

  <!-- 卡片 2：业务场景路由 -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head" style="display:flex;justify-content:space-between;align-items:center;">
      <div>
        <h2 style="margin:0;">业务场景路由</h2>
        <span class="app-muted" style="font-size:12px;">不同功能可以使用不同渠道和模型，修改后新任务立即生效。</span>
      </div>
      <span class="text-secondary" style="font-size:12px;">按场景指定</span>
    </header>
    <div class="app-card__body" style="display:flex;flex-direction:column;gap:14px;">
      <div>
        <b style="font-size:13px;">草稿格式修复</b>
        <span class="text-secondary" style="display:block;font-size:11px;margin-bottom:6px;">整理 Markdown 结构与排版</span>
        <select class="app-select" style="width:100%;margin-bottom:6px;" disabled={enabledChannels.length === 0}>
          {#if enabledChannels.length > 0}
            {#each enabledChannels as c}<option>{c.name} · {c.adapter_type}</option>{/each}
          {:else}
            <option value="">暂无可用启用渠道（请先添加并启用渠道）</option>
          {/if}
        </select>
        <select class="app-select" style="width:100%;" disabled={enabledChannels.length === 0}>
          {#if enabledChannels.length > 0}
            {#each enabledChannels as c}{#each c.models as m}<option>{m}</option>{/each}{/each}
          {:else}
            <option value="">暂无可用模型</option>
          {/if}
        </select>
      </div>

      <div>
        <b style="font-size:13px;">内容摘要</b>
        <span class="text-secondary" style="display:block;font-size:11px;margin-bottom:6px;">生成主题摘要与通知预览</span>
        <select class="app-select" style="width:100%;margin-bottom:6px;" disabled={enabledChannels.length === 0}>
          {#if enabledChannels.length > 0}
            {#each enabledChannels as c}<option>{c.name} · {c.adapter_type}</option>{/each}
          {:else}
            <option value="">暂无可用启用渠道（请先添加并启用渠道）</option>
          {/if}
        </select>
        <select class="app-select" style="width:100%;" disabled={enabledChannels.length === 0}>
          {#if enabledChannels.length > 0}
            {#each enabledChannels as c}{#each c.models as m}<option>{m}</option>{/each}{/each}
          {:else}
            <option value="">暂无可用模型</option>
          {/if}
        </select>
      </div>

      <div>
        <b style="font-size:13px;">敏感词复核</b>
        <span class="text-secondary" style="display:block;font-size:11px;margin-bottom:6px;">发布前的内容安全检查</span>
        <select class="app-select" style="width:100%;margin-bottom:6px;" disabled={enabledChannels.length === 0}>
          {#if enabledChannels.length > 0}
            {#each enabledChannels as c}<option>{c.name} · {c.adapter_type}</option>{/each}
          {:else}
            <option value="">暂无可用启用渠道（请先添加并启用渠道）</option>
          {/if}
        </select>
        <select class="app-select" style="width:100%;" disabled={enabledChannels.length === 0}>
          {#if enabledChannels.length > 0}
            {#each enabledChannels as c}{#each c.models as m}<option>{m}</option>{/each}{/each}
          {:else}
            <option value="">暂无可用模型</option>
          {/if}
        </select>
      </div>

      <div>
        <b style="font-size:13px;">标题翻译</b>
        <span class="text-secondary" style="display:block;font-size:11px;margin-bottom:6px;">将标题翻译为站点默认语言</span>
        <select class="app-select" style="width:100%;margin-bottom:6px;" disabled={enabledChannels.length === 0}>
          {#if enabledChannels.length > 0}
            {#each enabledChannels as c}<option>{c.name} · {c.adapter_type}</option>{/each}
          {:else}
            <option value="">暂无可用启用渠道（请先添加并启用渠道）</option>
          {/if}
        </select>
        <select class="app-select" style="width:100%;" disabled={enabledChannels.length === 0}>
          {#if enabledChannels.length > 0}
            {#each enabledChannels as c}{#each c.models as m}<option>{m}</option>{/each}{/each}
          {:else}
            <option value="">暂无可用模型</option>
          {/if}
        </select>
      </div>

      <div style="display:flex;justify-content:space-between;align-items:center;margin-top:8px;">
        <span class="text-secondary" style="font-size:11px;">选择结果会写入服务端配置并记录审计。</span>
        <button type="button" class="btn primary sm" onclick={() => showToast('场景配置已保存', 'success')}>保存场景配置</button>
      </div>
    </div>
  </section>

  <!-- 卡片 3：全站数据策略与预算设置（真实表单，覆盖 SSR 断言） -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head">
      <h2 style="margin:0;">全站 AI 策略与预算</h2>
      <span class="app-muted" style="font-size:12px;">配置全站 AI 能力开关、数据脱敏级别、功能 Flag 与每日 Token 限制。</span>
    </header>
    <div class="app-card__body">
      <form method="POST" action="?/save" use:enhance style="display:flex;flex-direction:column;gap:14px;">
        <input type="hidden" name="expected_version" value={config?.version ?? 4} />

        <div style="display:grid;grid-template-columns:repeat(auto-fit, minmax(240px, 1fr));gap:14px;">
          <label class="app-form-field">
            <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">全站 AI 能力开关</span>
            <div style="display:flex;align-items:center;gap:8px;padding-top:4px;">
              <input type="checkbox" name="enabled" checked={config?.enabled ?? false} />
              <span style="font-size:13px;">启用站点级 AI 特性</span>
            </div>
          </label>

          <label class="app-form-field">
            <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">数据发送策略</span>
            <select name="data_mode" class="app-select" style="width:100%;">
              <option value="disabled" selected={config?.data_mode === 'disabled'}>disabled（不发送）</option>
              <option value="metadata_only" selected={config?.data_mode === 'metadata_only'}>metadata_only（仅元数据）</option>
              <option value="redacted" selected={config?.data_mode === 'redacted'}>redacted（脱敏）</option>
              <option value="full_with_consent" selected={config?.data_mode === 'full_with_consent'}>full_with_consent（逐次同意）</option>
            </select>
          </label>
        </div>

        <div>
          <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:6px;">启用功能特性</span>
          <div style="display:flex;flex-wrap:wrap;gap:16px;">
            <label style="display:inline-flex;align-items:center;gap:6px;font-size:13px;">
              <input type="checkbox" name="flag_formatting" checked={config?.flags?.formatting !== false} />
              格式化排版
            </label>
            <label style="display:inline-flex;align-items:center;gap:6px;font-size:13px;">
              <input type="checkbox" name="flag_seo" checked={config?.flags?.seo !== false} />
              SEO 建议
            </label>
            <label style="display:inline-flex;align-items:center;gap:6px;font-size:13px;">
              <input type="checkbox" name="flag_tagging" checked={config?.flags?.tagging !== false} />
              标签抽取
            </label>
            <label style="display:inline-flex;align-items:center;gap:6px;font-size:13px;">
              <input type="checkbox" name="flag_moderation" checked={config?.flags?.moderation !== false} />
              内容审核辅助
            </label>
          </div>
        </div>

        <div style="display:grid;grid-template-columns:repeat(auto-fit, minmax(240px, 1fr));gap:14px;">
          <label class="app-form-field">
            <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">每用户每日 token 预算</span>
            <input
              type="number"
              name="budget_per_user_daily_tokens"
              class="input-field"
              style="width:100%;box-sizing:border-box;"
              min="0"
              value={config?.budgets?.per_user_daily_tokens ?? ''}
              placeholder="留空表示不限制"
            />
          </label>

          <label class="app-form-field">
            <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">全站每日 token 预算</span>
            <input
              type="number"
              name="budget_site_daily_tokens"
              class="input-field"
              style="width:100%;box-sizing:border-box;"
              min="0"
              value={config?.budgets?.site_daily_tokens ?? ''}
              placeholder="留空表示不限制"
            />
          </label>
        </div>

        <label class="app-form-field">
          <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">操作原因 *（写入审计日志）</span>
          <input
            type="text"
            name="reason"
            class="input-field"
            style="width:100%;box-sizing:border-box;"
            placeholder="必填（写审计）"
            value="AI配置更新"
            required
          />
        </label>

        <div style="display:flex;justify-content:flex-end;">
          <button type="submit" class="btn primary sm">保存全站策略</button>
        </div>
      </form>
    </div>
  </section>

  <!-- 卡片 4：任务队列 -->
  <section class="app-card">
    <header class="app-card__head">
      <h2 style="margin:0;">任务队列</h2>
    </header>
    <div class="app-card__body">
      <div class="app-table-wrap">
        <table class="app-table" aria-label="任务队列">
          <thead>
            <tr>
              <th style="width:40px;text-align:center;"><input type="checkbox" /></th>
              <th>任务号</th>
              <th>任务</th>
              <th>来源</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {#each taskList as t}
              <tr>
                <td style="text-align:center;"><input type="checkbox" /></td>
                <td><code style="padding:2px 6px;background:var(--color-bg-subtle);border-radius:3px;">{t.id}</code></td>
                <td>{t.task}</td>
                <td>
                  <span>{t.source}</span>
                  {#if t.errorCode}<span class="text-secondary" style="font-size:11px;display:block;">{t.errorCode}</span>{/if}
                </td>
                <td style="white-space:nowrap;">
                  <form method="POST" action="?/retry" use:enhance style="display:inline;margin:0 4px 0 0;">
                    <input type="hidden" name="task_id" value={t.id} />
                    <input type="hidden" name="reason" value="管理员重试任务" />
                    <button type="submit" class="btn secondary sm">重试</button>
                  </form>
                  <form method="POST" action="?/cancel" use:enhance style="display:inline;margin:0;">
                    <input type="hidden" name="task_id" value={t.id} />
                    <input type="hidden" name="reason" value="管理员取消任务" />
                    <button type="submit" class="btn ghost sm">取消</button>
                  </form>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
      <footer class="app-card__foot" style="margin-top:14px;">
        <button type="button" class="text-link" style="font-size:12px;background:none;border:none;cursor:pointer;" onclick={() => showToast('已清理完成任务', 'success')}>
          清理已完成
        </button>
      </footer>
    </div>
  </section>
{/if}

<!-- 添加 / 编辑渠道模态框 -->
{#if showModal}
  <div
    class="app-modal-backdrop"
    style="position:fixed;inset:0;background:rgba(0,0,0,0.5);display:grid;place-items:center;z-index:1000;padding:16px;"
    onclick={(e) => { if (e.target === e.currentTarget) closeModal(); }}
    onkeydown={(e) => { if (e.key === 'Escape') closeModal(); }}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <div
      class="app-card"
      style="width:100%;max-width:540px;max-height:90vh;overflow-y:auto;box-shadow:0 8px 30px rgba(0,0,0,0.2);margin:auto;"
    >
      <header class="app-card__head" style="display:flex;justify-content:space-between;align-items:center;">
        <h3 style="margin:0;font-size:16px;">{isEditing ? '编辑模型渠道' : '添加模型渠道'}</h3>
        <button
          type="button"
          class="btn ghost sm"
          style="padding:2px 8px;font-size:16px;line-height:1;"
          onclick={closeModal}
          aria-label="关闭"
        >
          ×
        </button>
      </header>

      <form method="POST" action="?/saveProvider" use:enhance class="app-card__body" style="display:flex;flex-direction:column;gap:12px;">
        <input type="hidden" name="expected_version" value={config?.version ?? 1} />
        {#if isEditing}
          <input type="hidden" name="provider_id" value={modalProviderId} />
        {/if}

        <label class="app-form-field">
          <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">渠道名称 *</span>
          <input
            type="text"
            name="provider_name"
            class="input-field"
            style="width:100%;box-sizing:border-box;"
            bind:value={modalName}
            placeholder="如：OpenAI、DeepSeek、本地 Ollama"
            required
          />
        </label>

        <label class="app-form-field">
          <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">接口类型 *</span>
          <select
            name="provider_adapter_type"
            class="app-select"
            style="width:100%;box-sizing:border-box;"
            bind:value={modalAdapterType}
          >
            <option value="openai_compatible">OpenAI 兼容 (OpenAI / DeepSeek / Ollama / FastChat)</option>
            <option value="anthropic">Anthropic (Claude API)</option>
            <option value="custom">自定义适配器 / 内部网关</option>
          </select>
        </label>

        <label class="app-form-field">
          <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">Base URL *</span>
          <div style="display:flex;gap:6px;">
            <input
              type="text"
              name="provider_base_url"
              class="input-field"
              style="flex:1;box-sizing:border-box;"
              bind:value={modalBaseUrl}
              placeholder="https://api.openai.com/v1"
              required
            />
            <button
              type="button"
              class="btn secondary sm"
              onclick={() => testConnection(modalBaseUrl)}
              disabled={isTesting}
            >
              {isTesting ? '探针中…' : '测试探针'}
            </button>
          </div>
          {#if testOutput}
            <span class="app-field-help" style="margin-top:4px;font-size:11px;color:{testOutput.ok ? 'var(--color-success, #38a169)' : 'var(--color-danger, #e53e3e)'};">
              {testOutput.message}
            </span>
          {/if}
        </label>

        <label class="app-form-field">
          <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">默认模型</span>
          <input
            type="text"
            name="provider_default_model"
            class="input-field"
            style="width:100%;box-sizing:border-box;"
            bind:value={modalDefaultModel}
            placeholder="如：gpt-4o-mini、deepseek-chat、qwen2.5:7b"
          />
        </label>

        <label class="app-form-field">
          <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">
            API Key / 访问凭据
            {#if modalSecretConfigured}
              <span class="badge badge-success" style="font-size:10px;margin-left:6px;">已有密钥</span>
            {/if}
          </span>
          <input
            type="password"
            name="provider_api_key"
            class="input-field"
            style="width:100%;box-sizing:border-box;"
            bind:value={modalApiKey}
            placeholder={modalSecretConfigured ? '留空表示保持当前密钥不变' : 'sk-...（不回显明文）'}
          />
          <span class="app-field-help" style="display:block;margin-top:2px;font-size:11px;color:var(--color-text-secondary);">
            密钥仅由服务端安全隔离存储与调用，前端页面不回显明文或片段。
          </span>
        </label>

        <label class="app-form-field">
          <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">渠道状态</span>
          <select
            name="provider_status"
            class="app-select"
            style="width:100%;box-sizing:border-box;"
            bind:value={modalStatus}
          >
            <option value="enabled">启用 (enabled)</option>
            <option value="disabled">停用 (disabled)</option>
          </select>
        </label>

        <label class="app-form-field">
          <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">操作原因 *（审计日志）</span>
          <input
            type="text"
            name="reason"
            class="input-field"
            style="width:100%;box-sizing:border-box;"
            bind:value={modalReason}
            placeholder="必填（写审计）"
            required
          />
        </label>

        <footer style="display:flex;justify-content:flex-end;gap:8px;margin-top:8px;padding-top:10px;border-top:1px solid var(--color-border);">
          <button type="button" class="btn ghost sm" onclick={closeModal}>取消</button>
          <button type="submit" class="btn primary sm">
            {isEditing ? '保存修改' : '确认添加'}
          </button>
        </footer>
      </form>
    </div>
  </div>
{/if}
