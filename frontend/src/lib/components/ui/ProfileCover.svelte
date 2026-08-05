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
  let {
    src = null,
    label = '',
    class: klass = ''
  }: {
    /** 渲染期已解析的媒体 URL（临时，绝不持久化）；null → 渐变占位。 */
    src?: string | null;
    /** 有 label 时承载封面语义；空则装饰性（aria-hidden）。 */
    label?: string;
    /** 追加的样式类（如 profile-cover / user-hover-cover）。 */
    class?: string;
  } = $props();

  let failed = $state(false);

  // 任何依赖变化（如复用组件换 src）都重置失败态，允许重新尝试加载。
  $effect(() => {
    failed = false;
  });
</script>

<div
  class="profile-cover {klass}"
  class:has-error={failed}
  role={label ? 'img' : undefined}
  aria-label={label || undefined}
  aria-hidden={label ? undefined : 'true'}
>
  {#if src && !failed}
    <!-- 装饰性 cover 图片 alt 恒空：媒体内容仅供视觉，标题/资料正文已可读。 -->
    <img class="profile-cover-img" src={src} alt="" loading="lazy" onerror={() => (failed = true)} />
  {/if}
</div>
