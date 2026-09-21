<script lang="ts">
  // M03-UI-05：资料 Cover——安全渲染 + 失败降级（P0 隐私）。
  //
  // 隐私契约：
  // - 页面/资料卡数据只携带「附件引用」（avatar/cover_attachment_id），
  //   永不持久化带签名的 S3/CDN URL；签名 URL 由 M6 适配器在渲染期临时
  //   解析，不进入组件状态、不写入 data-* 属性、不缓存 localStorage；
  // - 图片加载失败/缺省 → 安全降级为渐变占位（.profile-cover 视觉），
  //   不显示破图图标、不输出媒体元数据、不报错泄漏；
  // - 装饰性 cover（无 label）→ aria-hidden，不进入可访问性树；
  // - SSR：src 缺省时只输出占位 div，不输出任何 URL 或私有字段。
  // - 支持 Steam 动态短视频与静态封面回退。
  import { attachmentContentUrl } from '$lib/api/client';

  let {
    src = null,
    fallbackSrc = null,
    videoWebm = null,
    videoMp4 = null,
    attachmentId = null,
    label = '',
    class: klass = '',
    style = ''
  }: {
    /** 渲染期已解析的媒体 URL（临时，绝不持久化）；null → 渐变占位。 */
    src?: string | null;
    /** 备用静态图 URL（本地 404 时回退至 CDN 静态缩略图）。 */
    fallbackSrc?: string | null;
    /** 动态 WebM 视频 URL。 */
    videoWebm?: string | null;
    /** 动态 MP4 视频 URL。 */
    videoMp4?: string | null;
    /** 附件 UUID 引用（优先通过稳定端点 /api/v1/attachments/{id}/content 解析）。 */
    attachmentId?: string | null;
    /** 有 label 时承载封面语义；空则装饰性（aria-hidden）。 */
    label?: string;
    /** 追加的样式类（如 profile-cover / user-hover-cover）。 */
    class?: string;
    /** 服务端校验后的结构化装饰变量；不接受任意 CSS 输入。 */
    style?: string;
  } = $props();

  let failed = $state(false);
  let videoFailed = $state(false);
  let videoReady = $state(false);
  const attachmentSrc = $derived(attachmentId ? attachmentContentUrl(attachmentId) : null);
  const mediaKey = $derived(
    JSON.stringify([src ?? '', fallbackSrc ?? '', videoWebm ?? '', videoMp4 ?? '', attachmentId ?? ''])
  );
  let currentImageSrc = $state<string | null>(null);
  const activeImageSrc = $derived(currentImageSrc ?? src ?? attachmentSrc);

  // 初始化与响应所有媒体 props 变化；首屏 SSR 也保留静态海报，不等待 effect。
  $effect(() => {
    currentImageSrc = src || attachmentSrc;
    failed = false;
    videoFailed = false;
    videoReady = false;
    // Track fallback/video props too: source changes must reset stale load state.
    void fallbackSrc;
    void videoWebm;
    void videoMp4;
  });

  const renderImageSrc = $derived(activeImageSrc);
  const showVideo = $derived(Boolean((videoWebm || videoMp4) && !videoFailed && videoReady));

  function handleImageError() {
    const current = activeImageSrc;
    if (current && fallbackSrc && current !== fallbackSrc) {
      currentImageSrc = fallbackSrc;
    } else if (current && attachmentSrc && current !== attachmentSrc) {
      currentImageSrc = attachmentSrc;
    } else {
      failed = true;
    }
  }

  function handleVideoReady() {
    videoReady = true;
  }

  function handleVideoError() {
    videoFailed = true;
    videoReady = false;
  }
</script>

<div
  class="profile-cover {klass}"
  style={style || undefined}
  class:has-error={failed && (!videoWebm && !videoMp4 || videoFailed)}
  role={label ? 'img' : undefined}
  aria-label={label || undefined}
  aria-hidden={label ? undefined : 'true'}
>
  {#if renderImageSrc && !failed}
    <!-- 装饰性 cover 图片 alt 恒空：媒体内容仅供视觉，标题/资料正文已可读。 -->
    <img
      class="profile-cover-img"
      src={renderImageSrc}
      alt=""
      loading="eager"
      style={showVideo ? 'z-index: 1;' : 'z-index: 2;'}
      onerror={handleImageError}
    />
  {/if}
  {#key mediaKey}
    {#if videoWebm || videoMp4}
      <video
        class="profile-cover-video"
        autoplay
        loop
        muted
        playsinline
        preload="auto"
        disablepictureinpicture
        style={showVideo ? 'z-index: 2;' : 'z-index: 1; opacity: 0;'}
        oncanplay={handleVideoReady}
        onloadeddata={handleVideoReady}
        onplaying={handleVideoReady}
        onerror={handleVideoError}
      >
        {#if videoWebm}
          <source src={videoWebm} type="video/webm" />
        {/if}
        {#if videoMp4}
          <source src={videoMp4} type="video/mp4" />
        {/if}
      </video>
    {/if}
  {/key}
</div>
