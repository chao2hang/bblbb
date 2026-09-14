<script lang="ts">
  // 全局壳·Toast 容器 —— 挂在 +layout.svelte，消费 lib/ui/toast store。
  // 复用 ui/Toast.svelte 渲染单条；容器样式来自 components.css 的
  // .toast-container（fixed 顶部居中、pointer-events 穿透、z-toast）。
  import Toast from './Toast.svelte';
  import { toasts, dismiss } from '$lib/ui/toast';
</script>

<div class="toast-container" aria-live="polite">
  {#each $toasts as item (item.id)}
    <Toast message={item.message} detail={item.detail} type={item.type} onclose={() => dismiss(item.id)} />
  {/each}
</div>
