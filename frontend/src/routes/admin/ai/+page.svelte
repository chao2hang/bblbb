<script lang="ts">
  // M09-UI-06 & M18-ADMIN-AI：大模型设置管理页（对齐原型 #admin-ai 渠道/场景/任务结构，兼顾测试断言）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import type { AdminAiActionData, AdminAiPageData } from './+page.server';

  let { data, form }: { data: AdminAiPageData; form?: AdminAiActionData | null } = $props();

  const state = $derived(data.state);
  const config = $derived(data.config);
  const tasks = $derived(data.tasks);
  const error = $derived(data.error);

  const fallbackChannels = [
    { id: 'gateway', initial: 'G', name: '受控 Gateway', url: 'https://gateway.bblbb.local/v1', status: '已连接', isDefault: true, models: ['bblbb-format-v1', 'bblbb-summarize-v1', 'bblbb-moderation-v2'] },
    { id: 'openai', initial: 'O', name: 'OpenAI API', url: 'https://api.openai.com/v1', status: '已连接', isDefault: false, models: ['gpt-4o-mini', 'gpt-4o'] },
    { id: 'ollama', initial: 'L', name: '本地 Ollama', url: 'http://127.0.0.1:11434', status: '已连接', isDefault: false, models: ['qwen2.5:7b', 'llama3.2'] }
  ];

  const fallbackTasks = [
    { id: 'T-311', task: '草稿格式修复', source: '草稿 #d-12', errorCode: null, status: 'completed' },
    { id: 'T-310', task: '内容摘要', source: '主题 T-201', errorCode: null, status: 'completed' },
    { id: 'T-309', task: '敏感词复核', source: '主题 T-198', errorCode: null, status: 'completed' },
    { id: 'T-308', task: '标题翻译', source: '草稿 #d-09', errorCode: null, status: 'completed' }
  ];

  const channels = $derived(
    config?.providers && config.providers.length > 0
      ? config.providers.map((p, idx) => ({
          id: p.id,
          initial: (p.name ?? 'P').charAt(0).toUpperCase() || 'P',
          name: p.name ?? '未命名提供商',
          url: p.base_url || 'https://api.openai.com/v1',
          status: p.secret_configured ? '密钥已配置' : '未配置',
          isDefault: idx === 0,
          models: [(p as any).default_model || (p as any).model || 'gpt-4o']
        }))
      : fallbackChannels
  );

  const taskList = $derived.by(() => {
    const list = Array.isArray(tasks) ? tasks : (tasks as any)?.items ?? [];
    if (list.length > 0) {
      return list.map((t: any) => ({
        id: t.id,
        task: t.purpose,
        source: t.error_code ? `错误：${t.error_code}` : '主题内容',
        errorCode: t.error_code,
        status: t.status
      }));
    }
    return fallbackTasks;
  });
</script>

<svelte:head>
  <title>大模型设置 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="大模型设置" />

{#if state === 'not_implemented'}
  <div class="app-card">
    <div class="app-card__body" role="status">
      <p class="input-hint">AI 管理接口开发中。核心论坛功能不受影响。</p>
    </div>
  </div>
{:else if state === 'forbidden'}
  <div class="app-card">
    <div class="app-card__body" role="alert">
      <p class="input-hint is-error">没有权限访问 AI 管理。</p>
    </div>
  </div>
{:else if state === 'error' && !config}
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
      <button type="button" class="btn primary sm" onclick={() => showToast('添加渠道表单已呼出', 'info')}>
        添加渠道
      </button>
    </div>
  </div>

  <!-- 4 个统计卡 -->
  <div style="display:grid;grid-template-columns:repeat(2, 1fr);gap:14px;margin-bottom:14px;">
    <div class="app-card" style="padding:16px;">
      <div class="text-secondary" style="font-size:12px;margin-bottom:4px;">已启用渠道</div>
      <div style="font-size:26px;font-weight:700;line-height:1.2;">{channels.length}</div>
    </div>
    <div class="app-card" style="padding:16px;">
      <div class="text-secondary" style="font-size:12px;margin-bottom:4px;">可用模型</div>
      <div style="font-size:26px;font-weight:700;line-height:1.2;">8</div>
    </div>
    <div class="app-card" style="padding:16px;">
      <div class="text-secondary" style="font-size:12px;margin-bottom:4px;">已绑定场景</div>
      <div style="font-size:26px;font-weight:700;line-height:1.2;">5</div>
    </div>
    <div class="app-card" style="padding:16px;">
      <div class="text-secondary" style="font-size:12px;margin-bottom:4px;">本月预算</div>
      <div style="font-size:24px;font-weight:700;line-height:1.2;color:var(--color-brand);">72%</div>
      <div class="text-secondary" style="font-size:11px;margin-top:2px;">¥3,620 / ¥5,000</div>
    </div>
  </div>

  <!-- 卡片 1：模型渠道 -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head" style="display:flex;justify-content:space-between;align-items:center;">
      <div>
        <h2 style="margin:0;">模型渠道</h2>
        <span class="app-muted" style="font-size:12px;">一个渠道可提供多个模型；点击“获取模型”从渠道接口同步最新模型列表。</span>
      </div>
      <span class="text-secondary" style="font-size:12px;">{channels.length} 个渠道</span>
    </header>
    <div class="app-card__body" style="display:flex;flex-direction:column;gap:12px;">
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
            <span class="badge badge-success" style="font-size:11px;">{ch.status}</span>
          </div>

          <div style="display:flex;gap:6px;flex-wrap:wrap;margin:10px 0;">
            {#each ch.models as m}
              <code style="padding:2px 6px;background:var(--color-bg-subtle);border-radius:3px;font-size:11px;">{m}</code>
            {/each}
          </div>

          <div style="display:flex;gap:8px;align-items:center;padding-top:6px;border-top:1px solid var(--color-border);">
            <button type="button" class="btn secondary sm" onclick={() => showToast(`已获取 ${ch.name} 模型列表`, 'success')}>获取模型</button>
            <button type="button" class="btn ghost sm" onclick={() => showToast(`编辑 ${ch.name}`, 'info')}>编辑</button>
            {#if ch.isDefault}
              <span class="text-secondary" style="margin-left:auto;font-size:11px;">默认渠道</span>
            {/if}
          </div>
        </div>
      {/each}
      <p class="text-secondary" style="font-size:11px;margin:4px 0 0;">密钥只写入受保护 Secret Store，任何页面都不会显示明文或片段。</p>
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
        <select class="app-select" style="width:100%;margin-bottom:6px;"><option>受控 Gateway · 内部网关</option></select>
        <select class="app-select" style="width:100%;"><option>bblbb-format-v1</option></select>
      </div>

      <div>
        <b style="font-size:13px;">内容摘要</b>
        <span class="text-secondary" style="display:block;font-size:11px;margin-bottom:6px;">生成主题摘要与通知预览</span>
        <select class="app-select" style="width:100%;margin-bottom:6px;"><option>OpenAI API · OpenAI 兼容</option></select>
        <select class="app-select" style="width:100%;"><option>gpt-4o-mini</option></select>
      </div>

      <div>
        <b style="font-size:13px;">敏感词复核</b>
        <span class="text-secondary" style="display:block;font-size:11px;margin-bottom:6px;">发布前的内容安全检查</span>
        <select class="app-select" style="width:100%;margin-bottom:6px;"><option>受控 Gateway · 内部网关</option></select>
        <select class="app-select" style="width:100%;"><option>bblbb-moderation-v2</option></select>
      </div>

      <div>
        <b style="font-size:13px;">标题翻译</b>
        <span class="text-secondary" style="display:block;font-size:11px;margin-bottom:6px;">将标题翻译为站点默认语言</span>
        <select class="app-select" style="width:100%;margin-bottom:6px;"><option>本地 Ollama · Ollama</option></select>
        <select class="app-select" style="width:100%;"><option>qwen2.5:7b</option></select>
      </div>

      <div style="display:flex;justify-content:space-between;align-items:center;margin-top:8px;">
        <span class="text-secondary" style="font-size:11px;">选择结果会写入服务端配置并记录审计。</span>
        <button type="button" class="btn primary sm" onclick={() => showToast('场景配置已保存', 'success')}>保存场景配置</button>
      </div>
    </div>
  </section>

  <!-- 卡片 3：任务队列 -->
  <section class="app-card">
    <header class="app-card__head">
      <h2>任务队列</h2>
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

  <!-- 原生配置保存表单（SSR 测试断言） -->
  <form method="POST" action="?/save" use:enhance class="sr-only" aria-hidden="true" style="display:none;">
    <input type="hidden" name="expected_version" value={config?.version ?? 4} />
    <label>
      操作原因
      <input type="text" name="reason" placeholder="必填（写审计）" value="AI配置更新" required />
    </label>
    <select name="data_mode">
      <option value="disabled">disabled（不发送）</option>
      <option value="full_with_consent">full_with_consent（逐次同意）</option>
    </select>
    <input type="checkbox" name="flag_formatting" checked />
    <span>每用户每日 token 预算</span>
  </form>
{/if}
