<script lang="ts">
  // M13-UI-01/PLUGIN-06：管理插件页——能力白名单、安装、启停、设置、审计。
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { adminStateLabel } from '$lib/admin';
  import type { AdminPluginsPageData, AdminPluginsActionData } from './+page.server';

  let { data, form }: { data: AdminPluginsPageData; form?: AdminPluginsActionData | null } = $props();

  const state = $derived(data.state);
  const plugins = $derived(data.plugins);
  const capabilities = $derived(data.capabilities);
  const message = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );
  const conflict = $derived(form?.conflict === true);
</script>

<svelte:head>
  <title>插件管理 — BBLBB</title>
</svelte:head>

<div class="card">
  <div class="card-header"><span class="card-title">插件管理（v1 配置型）</span></div>
  <div class="card-body">
    {#if state === 'forbidden'}
      <p class="input-hint is-error" role="alert"><Icon name="lock" size={14} /> {adminStateLabel('forbidden')}</p>
    {:else if state === 'not_implemented'}
      <p class="input-hint" role="note">插件接口开发中。核心论坛功能不受影响。</p>
    {:else if state === 'error'}
      <p class="input-hint is-error" role="alert">{data.error || adminStateLabel('error')}</p>
    {:else if state === 'ok'}
      {#if message}
        <p class="input-hint {conflict ? 'is-error' : ''}" role="status">{message}</p>
      {/if}
      {#if capabilities}
        <div class="card" style="margin-bottom:var(--space-4);">
          <div class="card-header"><span class="card-title">v1 能力边界</span></div>
          <div class="card-body">
            <p class="input-hint">
              插件是配置数据，无在线代码执行路径（{capabilities.note}）。
              只能访问显式输入与白名单动作，不能获得 DB/Session/OAuth Token/S3 Secret 或通用网络。
            </p>
            <ul style="list-style:none;margin:0;padding:0;display:flex;flex-wrap:wrap;gap:var(--space-2);">
              {#each capabilities.capabilities as cap (cap)}
                <li class="badge">{cap}</li>
              {/each}
            </ul>
            <p class="input-hint" style="margin-top:var(--space-3);">受控 Provider Adapter（随应用编译）：</p>
            <ul style="list-style:none;margin:0;padding:0;display:flex;gap:var(--space-2);flex-wrap:wrap;">
              {#each capabilities.provider_adapters as adapter (adapter.provider)}
                <li class="badge badge-primary">{adapter.provider}</li>
              {/each}
            </ul>
          </div>
        </div>
      {/if}

      {#if !plugins || plugins.length === 0}
        <p class="input-hint">暂无已安装插件。</p>
      {:else}
        <ul style="list-style:none;margin:0;padding:0;display:flex;flex-direction:column;gap:var(--space-3);">
          {#each plugins as plugin (plugin.id)}
            <li style="padding:var(--space-3);border:1px solid var(--color-border);border-radius:var(--radius-md);">
              <div style="display:flex;justify-content:space-between;align-items:center;gap:var(--space-3);flex-wrap:wrap;">
                <div>
                  <strong>{plugin.name}</strong>
                  <span class="text-secondary" style="font-size:var(--text-sm);margin-left:var(--space-2);">/{plugin.id} v{plugin.version}</span>
                </div>
                <div style="display:flex;gap:var(--space-2);align-items:center;flex-wrap:wrap;">
                  {#if plugin.status === 'enabled'}
                    <span class="badge badge-success">启用</span>
                  {:else if plugin.status === 'disabled'}
                    <span class="badge">停用</span>
                  {:else}
                    <span class="badge badge-danger">{plugin.status}</span>
                  {/if}
                  <span class="text-secondary" style="font-size:var(--text-sm);">policy v{plugin.policy_revision}</span>
                </div>
              </div>
              <div class="input-hint" style="margin-top:var(--space-2);">
                能力：{plugin.capabilities.join('、') || '无'}｜订阅：{plugin.subscriptions.join('、') || '无'}
              </div>
              {#if plugin.status === 'disabled'}
                <form method="POST" action="?/enable" use:enhance style="display:flex;gap:var(--space-2);margin-top:var(--space-2);flex-wrap:wrap;">
                  <input type="hidden" name="id" value={plugin.id} />
                  <input type="hidden" name="policy_revision" value={String(plugin.policy_revision)} />
                  <input type="text" class="input-field" name="reason" placeholder="操作原因（审计）" required style="max-width:200px;" />
                  <Button text="启用" variant="primary" size="sm" type="submit" />
                </form>
              {:else}
                <form method="POST" action="?/disable" use:enhance style="display:flex;gap:var(--space-2);margin-top:var(--space-2);flex-wrap:wrap;">
                  <input type="hidden" name="id" value={plugin.id} />
                  <input type="hidden" name="policy_revision" value={String(plugin.policy_revision)} />
                  <input type="text" class="input-field" name="reason" placeholder="操作原因（审计）" required style="max-width:200px;" />
                  <Button text="停用" variant="secondary" size="sm" type="submit" />
                </form>
              {/if}
              <form method="POST" action="?/settings" use:enhance style="display:flex;flex-direction:column;gap:var(--space-2);margin-top:var(--space-2);">
                <input type="hidden" name="id" value={plugin.id} />
                <input type="hidden" name="policy_revision" value={String(plugin.policy_revision)} />
                <label class="input-label" for="settings-{plugin.id}">设置（JSON，closed schema）</label>
                <textarea class="input-field" id="settings-{plugin.id}" name="settings_json" rows="4" spellcheck="false">{JSON.stringify(plugin.settings ?? {}, null, 2)}</textarea>
                <div style="display:flex;gap:var(--space-2);flex-wrap:wrap;">
                  <input type="text" class="input-field" name="reason" placeholder="操作原因（审计）" required style="max-width:200px;" />
                  <Button text="保存设置" variant="primary" size="sm" type="submit" />
                </div>
              </form>
            </li>
          {/each}
        </ul>
      {/if}

      <!-- 安装（默认 disabled 隔离态） -->
      <form method="POST" action="?/install" use:enhance class="card" style="margin-top:var(--space-4);">
        <div class="card-header"><span class="card-title">安装配置型插件</span></div>
        <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
          <div class="input-wrapper">
            <label class="input-label" for="plugin-id">插件 ID</label>
            <input type="text" class="input-field" id="plugin-id" name="id" maxlength="64" pattern="[a-z0-9-]+" required />
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="plugin-name">名称</label>
            <input type="text" class="input-field" id="plugin-name" name="name" maxlength="120" />
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="plugin-caps">capabilities（JSON 数组）</label>
            <input type="text" class="input-field" id="plugin-caps" name="capabilities" value='["notification.create"]' spellcheck="false" />
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="plugin-subs">subscriptions（JSON 数组）</label>
            <input type="text" class="input-field" id="plugin-subs" name="subscriptions" value='["user.verified.v1"]' spellcheck="false" />
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="plugin-schema">settings_schema（JSON）</label>
            <textarea class="input-field" id="plugin-schema" name="settings_schema" rows="6" spellcheck="false">{'{"type":"object","properties":{},"required":[],"additionalProperties":false}'}</textarea>
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="plugin-install-reason">操作原因（审计）</label>
            <input type="text" class="input-field" id="plugin-install-reason" name="reason" required placeholder="记录到审计日志" />
          </div>
          <div><Button text="安装插件" variant="primary" size="sm" type="submit" /></div>
        </div>
      </form>
    {/if}
  </div>
</div>
