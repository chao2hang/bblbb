<script lang="ts">
  // M10-UI-01/02：编辑器视频插入面板。
  //
  // - 手动 URL 输入 → resolve 预览（Provider 状态、标题/封面/时长、可嵌入性
  //   与错误说明）；前端只做格式提示，后端重新解析（VIDEO-PLUGIN.md §2）；
  // - 只把后端投影白名单挑选后的 VideoResolveResult 提交给父级（M10-UI-02：
  //   只提交 resolution_id + 允许字段，Provider Secret/Key 永不进入浏览器
  //   状态或请求体；本面板对 onResolve 返回的任意原始 JSON 再次 pick）；
  // - Feature Flag 未启用（409 feature_disabled）→ 关闭说明，不影响发帖；
  // - 不可嵌入（无嵌入权限/限流/下架/Provider 故障）→ 降级为安全外链方式
  //   插入，不阻塞发帖（VIDEO-PLUGIN.md §3/§9）。
  import Button from '$lib/components/ui/Button.svelte';
  import { problemText, type Problem } from '$lib/errors';
  import { pickVideoResolve, safeHttpsUrl } from '$lib/video/projection';
  import {
    formatVideoDuration,
    videoDegradedLabel,
    videoProviderLabel
  } from '$lib/video/labels';
  import type { VideoResolveResult } from '$lib/api/types';

  let {
    onResolve,
    onAccept
  }: {
    /** 触发后端 resolve；返回任意原始响应，本面板负责白名单挑选。 */
    onResolve: (url: string) => Promise<unknown>;
    /** 用户确认插入；只回调白名单挑选后的 VideoResolveResult。 */
    onAccept: (result: VideoResolveResult) => void;
  } = $props();

  type Phase = 'idle' | 'resolving' | 'preview' | 'error' | 'disabled' | 'accepted';

  let url = $state('');
  let phase = $state<Phase>('idle');
  let preview = $state<VideoResolveResult | null>(null);
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);

  /** 前端只做格式提示（后端重新解析）。 */
  const urlHint = $derived.by(() => {
    const trimmed = url.trim();
    if (!trimmed) return null;
    if (!/^https:\/\/.+/i.test(trimmed)) return '链接必须以 https:// 开头（仅支持 HTTPS 来源）';
    if (trimmed.includes(' ') || /[\u0000-\u001f]/.test(trimmed)) return '链接包含非法字符';
    return null;
  });

  const canResolve = $derived(phase !== 'resolving' && url.trim().length > 0 && urlHint === null);

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    if (!canResolve) return;
    error = null;
    notice = null;
    const trimmed = url.trim();
    phase = 'resolving';
    try {
      const raw = await onResolve(trimmed);
      const result = pickVideoResolve(raw);
      if (!result) {
        error = '解析结果无效，请稍后重试';
        phase = 'error';
        return;
      }
      preview = result;
      phase = 'preview';
    } catch (err: unknown) {
      const problem = err as Problem;
      if (problem?.code === 'feature_disabled') {
        notice = '视频功能未开放（默认关闭）。发布与编辑不受影响。';
        phase = 'disabled';
      } else {
        error = problemText(problem);
        phase = 'error';
      }
    }
  }

  function acceptPreview() {
    if (!preview) return;
    onAccept(preview);
    phase = 'accepted';
  }
</script>

<div class="card" style="border-color:var(--border-default);">
  <div class="card-header">
    <span class="card-title" id="video-insert-title">插入视频</span>
  </div>
  <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
    <form onsubmit={handleSubmit}>
      <div class="input-wrapper">
        <label class="input-label" for="video-insert-url">视频链接（https）</label>
        <input
          id="video-insert-url"
          class="input-field"
          type="url"
          inputmode="url"
          placeholder="https://example.com/video.mp4 或西瓜视频页面链接"
          bind:value={url}
          autocomplete="off"
          aria-describedby={urlHint ? 'video-insert-url-hint' : undefined}
          aria-invalid={urlHint ? 'true' : undefined}
        />
        {#if urlHint}
          <p class="input-hint is-error" id="video-insert-url-hint" role="alert">{urlHint}</p>
        {/if}
        <p class="input-hint">支持直链视频（mp4/webm/ogv/mov）、HLS（m3u8）与西瓜视频公开页面链接。仅格式提示，实际校验由服务端完成。</p>
        <div style="margin-top:var(--space-2);">
          <Button
            text={phase === 'resolving' ? '解析中…' : '解析预览'}
            variant="secondary"
            size="sm"
            icon="video"
            type="submit"
            disabled={phase === 'resolving' || !canResolve}
          />
        </div>
      </div>
    </form>

    {#if error}
      <p class="input-hint is-error" role="alert" style="margin:0;">{error}</p>
    {/if}
    {#if notice}
      <p class="input-hint" role="status" style="margin:0;">{notice}</p>
    {/if}

    {#if phase === 'accepted' && preview}
      <div style="border:var(--border-default);border-radius:var(--radius-md);padding:var(--space-2);display:flex;flex-wrap:wrap;gap:var(--space-2);align-items:center;">
        <span class="badge badge-success">已加入视频引用</span>
        <span class="text-secondary" style="font-size:var(--text-sm);">{preview.title ?? videoProviderLabel(preview.provider)}</span>
        <span class="input-hint" style="margin-left:auto;margin-bottom:0;">发布帖子时一并保存视频引用。</span>
      </div>
    {:else if phase === 'preview' && preview}
      <div style="border:var(--border-default);border-radius:var(--radius-md);padding:var(--space-2);display:flex;flex-direction:column;gap:var(--space-2);">
        <div style="display:flex;gap:var(--space-2);align-items:flex-start;">
          {#if safeHttpsUrl(preview.poster_url)}
            <img
              src={safeHttpsUrl(preview.poster_url)!}
              alt=""
              style="width:96px;height:54px;object-fit:cover;border-radius:var(--radius-sm);flex:none;"
            />
          {/if}
          <div style="display:flex;flex-direction:column;gap:var(--space-1);min-width:0;">
            {#if preview.title}
              <strong style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">{preview.title}</strong>
            {/if}
            <div style="display:flex;flex-wrap:wrap;gap:var(--space-1);align-items:center;">
              {#if preview.provider}
                <span class="badge badge-neutral">{videoProviderLabel(preview.provider)}</span>
              {/if}
              {#if preview.media_type}
                <span class="badge badge-neutral">{preview.media_type}</span>
              {/if}
              {#if preview.duration_seconds}
                <span class="text-secondary" style="font-size:var(--text-xs);">{formatVideoDuration(preview.duration_seconds)}</span>
              {/if}
            </div>
            {#if preview.provider_status}
              <div style="display:flex;flex-wrap:wrap;gap:var(--space-1);align-items:center;">
                <span class="badge {preview.provider_status.available === false ? 'badge-warning' : 'badge-success'}">
                  {preview.provider_status.available === false ? 'Provider 不可用' : 'Provider 可用'}
                </span>
                {#if preview.provider_status.enabled === false}
                  <span class="badge badge-warning">已停用</span>
                {/if}
              </div>
            {/if}
            <p class="input-hint" style="margin:0;">
              {preview.embeddable ? '可安全嵌入播放。' : `当前不可嵌入：${videoDegradedLabel(preview.degraded_reason)}`}
            </p>
          </div>
        </div>
        <div style="display:flex;gap:var(--space-2);align-items:center;">
          <Button
            text={preview.embeddable ? '插入视频' : '以外链方式插入'}
            variant="primary"
            size="sm"
            type="button"
            onclick={acceptPreview}
          />
          {#if safeHttpsUrl(preview.source_url)}
            <a class="text-link" style="font-size:var(--text-xs);" href={safeHttpsUrl(preview.source_url)!} rel="noopener noreferrer nofollow" target="_blank">来源 / 外链</a>
          {/if}
        </div>
      </div>
    {/if}

    <p class="input-hint" style="margin:0;">
      你应拥有或获准分享该视频链接；本站不复制第三方视频。Provider 状态只显示脱敏信息，密钥不会出现在任何页面。
    </p>
  </div>
</div>
