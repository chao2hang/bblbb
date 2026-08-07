<script lang="ts">
  // M10-UI-06：管理端视频——逐 Provider 策略配置、测试、停用与审计展示。
  //
  // - 每个 Provider 一张原生表单（save 用 If-Match 版本 + reason；test 走
  //   formaction 提交同一表单值），无 JS 可提交；
  // - 停用 = 取消勾选「启用」后保存（立即影响新解析）；
  // - 审计展示：每个 Provider 卡片显示 policy_version 与最近更新时间；
  //   所有写操作要求 reason（服务端写审计）。
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import { formatTime } from '$lib/utils';
  import { videoProviderLabel } from '$lib/video/labels';
  import type { AdminVideoActionData, AdminVideoPageData } from './+page.server';

  let { data, form }: { data: AdminVideoPageData; form?: AdminVideoActionData | null } = $props();

  const state = $derived(data.state);
  const policies = $derived(data.policies);
  const error = $derived(data.error);
  const items = $derived(policies?.items ?? []);
  const message = $derived(form?.message ?? null);
  const conflict = $derived(form?.conflict === true);
  const formProvider = $derived(form?.provider ?? null);
  const testResult = $derived(form?.testResult ?? null);
  const siteEnabled = $derived(policies?.enabled !== false);

  /** 审计时间展示：后端时间戳口径未定（秒或毫秒），统一归一化为秒。 */
  function formatAuditTime(ts: number | null | undefined): string | null {
    if (typeof ts !== 'number' || !Number.isFinite(ts)) return null;
    const seconds = ts > 1e11 ? Math.floor(ts / 1000) : ts;
    return formatTime(seconds);
  }
</script>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <a href="/admin" class="breadcrumb-link">管理后台</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">视频管理</span>
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
        <p class="input-hint" role="status">视频管理接口开发中（后端未实现）。核心论坛功能不受影响。</p>
      </div>
    </div>
  {:else if state === 'forbidden'}
    <div class="card">
      <div class="card-body">
        <p class="input-hint is-error" role="alert">你没有权限访问视频管理。</p>
      </div>
    </div>
  {:else if state === 'error' && !policies}
    <div class="card">
      <div class="card-body">
        <p class="input-hint is-error" role="alert">加载失败：{error}</p>
      </div>
    </div>
  {:else if policies}
    <div class="card" style="margin-bottom:var(--space-4);">
      <div class="card-header"><span class="card-title">站点视频能力与 Provider 策略</span></div>
      <div class="card-body" style="display:flex;flex-wrap:wrap;gap:var(--space-2);align-items:center;">
        <span class="badge {siteEnabled ? 'badge-success' : 'badge-warning'}">
          {siteEnabled ? '视频功能已开放' : '视频功能未开放（Feature Flag 默认关闭）'}
        </span>
        {#if policies.version}
          <span class="text-secondary" style="font-size:var(--text-xs);">策略集版本 v{policies.version}</span>
        {/if}
        <p class="input-hint" style="margin:0;flex-basis:100%;">
          Provider 策略修改立即影响新解析与新渲染；历史引用在下次检查时按新策略决定继续嵌入或降级为外链。
        </p>
      </div>
    </div>

    {#if items.length === 0}
      <div class="card">
        <div class="card-body">
          <p class="input-hint" role="status">暂无已注册的 Provider 策略。</p>
        </div>
      </div>
    {:else}
      {#each items as policy (policy.provider)}
        <div class="card" style="margin-bottom:var(--space-4);">
          <div class="card-header" style="display:flex;flex-wrap:wrap;gap:var(--space-2);align-items:center;">
            <span class="card-title">{videoProviderLabel(policy.provider)}（{policy.provider}）</span>
            <span class="badge {policy.enabled ? 'badge-success' : 'badge-warning'}">
              {policy.enabled ? '已启用' : '已停用'}
            </span>
            <span class="text-secondary" style="font-size:var(--text-xs);">
              审计：策略版本 v{policy.policy_version}
              {#if formatAuditTime(policy.updated_at)}· 更新于 {formatAuditTime(policy.updated_at)}{/if}
            </span>
          </div>
          <div class="card-body">
            <form method="POST" action="?/save" use:enhance>
              <input type="hidden" name="provider" value={policy.provider} />
              <input type="hidden" name="expected_version" value={policy.policy_version} />
              <div class="input-wrapper" style="margin-bottom:var(--space-2);">
                <label style="display:flex;align-items:center;gap:var(--space-2);">
                  <input type="checkbox" name="enabled" checked={policy.enabled} />
                  启用此 Provider（取消勾选后保存即停用，立即影响新解析）
                </label>
              </div>
              <div class="admin-form-grid" style="display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:var(--space-2);">
                <div class="input-wrapper">
                  <label class="input-label" for="allowed_hosts-{policy.provider}">允许的来源 host（逗号/换行分隔）</label>
                  <textarea id="allowed_hosts-{policy.provider}" name="allowed_hosts" class="input-field" rows="3" placeholder="example.com">{policy.allowed_hosts.join('\n')}</textarea>
                </div>
                <div class="input-wrapper">
                  <label class="input-label" for="embed_hosts-{policy.provider}">允许的嵌入 host（iframe 官方来源）</label>
                  <textarea id="embed_hosts-{policy.provider}" name="embed_hosts" class="input-field" rows="3" placeholder="embed.example.com">{policy.embed_hosts.join('\n')}</textarea>
                </div>
                <div class="input-wrapper">
                  <label class="input-label" for="allowed_media_types-{policy.provider}">允许的媒体类型</label>
                  <textarea id="allowed_media_types-{policy.provider}" name="allowed_media_types" class="input-field" rows="3" placeholder="video/mp4">{policy.allowed_media_types.join('\n')}</textarea>
                </div>
              </div>
              <div class="admin-form-grid" style="display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:var(--space-2);margin-top:var(--space-2);">
                <div class="input-wrapper">
                  <label class="input-label" for="max_duration_seconds-{policy.provider}">最大时长（秒）</label>
                  <input id="max_duration_seconds-{policy.provider}" name="max_duration_seconds" type="number" min="0" class="input-field" value={policy.max_duration_seconds ?? ''} placeholder="不限" />
                </div>
                <div class="input-wrapper">
                  <label class="input-label" for="max_bytes-{policy.provider}">最大字节数</label>
                  <input id="max_bytes-{policy.provider}" name="max_bytes" type="number" min="0" class="input-field" value={policy.max_bytes ?? ''} placeholder="不限" />
                </div>
                <div class="input-wrapper">
                  <label class="input-label" for="max_redirects-{policy.provider}">最大重定向次数</label>
                  <input id="max_redirects-{policy.provider}" name="max_redirects" type="number" min="0" class="input-field" value={policy.max_redirects ?? ''} placeholder="不限" />
                </div>
                <div class="input-wrapper">
                  <label class="input-label" for="timeout_ms-{policy.provider}">超时（毫秒）</label>
                  <input id="timeout_ms-{policy.provider}" name="timeout_ms" type="number" min="0" class="input-field" value={policy.timeout_ms ?? ''} placeholder="不限" />
                </div>
                <div class="input-wrapper">
                  <label class="input-label" for="hls_max_depth-{policy.provider}">HLS 最大递归深度</label>
                  <input id="hls_max_depth-{policy.provider}" name="hls_max_depth" type="number" min="0" class="input-field" value={policy.hls_max_depth ?? ''} placeholder="不限" />
                </div>
                <div class="input-wrapper">
                  <label class="input-label" for="hls_max_segments-{policy.provider}">HLS 最大分片数</label>
                  <input id="hls_max_segments-{policy.provider}" name="hls_max_segments" type="number" min="0" class="input-field" value={policy.hls_max_segments ?? ''} placeholder="不限" />
                </div>
                <div class="input-wrapper">
                  <label class="input-label" for="hls_max_bytes-{policy.provider}">HLS 最大总字节</label>
                  <input id="hls_max_bytes-{policy.provider}" name="hls_max_bytes" type="number" min="0" class="input-field" value={policy.hls_max_bytes ?? ''} placeholder="不限" />
                </div>
              </div>
              <div class="input-wrapper" style="margin-top:var(--space-2);">
                <label class="input-label" for="reason-{policy.provider}">操作原因</label>
                <input id="reason-{policy.provider}" name="reason" class="input-field" required placeholder="必填（写审计）" />
              </div>
              <div style="display:flex;gap:var(--space-2);margin-top:var(--space-2);flex-wrap:wrap;align-items:center;">
                <Button text="保存配置" variant="primary" size="sm" type="submit" />
                <Button text="测试此 Provider" variant="secondary" size="sm" type="submit" formaction="?/test" />
                {#if formProvider === policy.provider && testResult}
                  <span class="input-hint {testResult.ok ? '' : 'is-error'}" role="status" style="margin:0;">
                    测试结果：{testResult.ok ? '连接成功' : `失败（${testResult.code ?? '未知'}）`} —— {testResult.message}
                    {#if typeof testResult.elapsed_ms === 'number'}
                      （{testResult.elapsed_ms} ms）
                    {/if}
                  </span>
                {/if}
              </div>
            </form>
          </div>
        </div>
      {/each}

      <div class="card">
        <div class="card-header"><span class="card-title">审计与边界说明</span></div>
        <div class="card-body">
          <ul style="margin:0;padding-left:var(--space-4);display:flex;flex-direction:column;gap:var(--space-2);">
            <li>所有写操作（保存/停用/测试）都要求操作原因，服务端写入审计；页面只展示每个 Provider 的当前策略版本与更新时间。</li>
            <li>Provider 不存 Secret；即使存在内部字段也不会进入本页面（只显示脱敏状态）。</li>
            <li>来源/嵌入 host 与媒体类型由服务端最终校验；前端提交仅做格式提示。</li>
          </ul>
        </div>
      </div>
    {/if}
  {/if}
</div>
