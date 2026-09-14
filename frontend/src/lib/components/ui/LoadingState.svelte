<script lang="ts">
  // 加载状态组件（M00-FRONTEND-05）：统一「加载中」占位，带 aria-live 播报。
  // 动画与 blbui aui-spinner 对齐：三个变形方块（pulse），不再使用旋转圆环。
  let { title = '加载中…', desc = '' }: { title?: string; desc?: string } = $props();
</script>

<div class="empty-state" role="status" aria-live="polite">
  <div class="loading-blocks" aria-hidden="true">
    <i></i><i></i><i></i>
  </div>
  <div class="empty-state-title">{title}</div>
  {#if desc}<div class="empty-state-desc">{desc}</div>{/if}
</div>

<style>
  /* 三个变形方块：与 aui-spinner 同一视觉语言（scaleY + opacity 脉冲） */
  .loading-blocks {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 3px;
    height: 18px;
    margin-bottom: var(--space-2);
  }

  .loading-blocks i {
    width: 5px;
    height: 100%;
    background: var(--color-brand, var(--color-accent));
    animation: bblbb-blocks-pulse 800ms ease-in-out infinite;
  }

  .loading-blocks i:nth-child(2) {
    animation-delay: 100ms;
  }

  .loading-blocks i:nth-child(3) {
    animation-delay: 200ms;
  }

  @keyframes bblbb-blocks-pulse {
    0%,
    100% {
      opacity: 0.35;
      transform: scaleY(0.7);
    }
    50% {
      opacity: 1;
      transform: scaleY(1);
    }
  }

  /* M00-FRONTEND-07：减少动效偏好下停止动画。 */
  @media (prefers-reduced-motion: reduce) {
    .loading-blocks i {
      animation: none;
    }
  }
</style>
