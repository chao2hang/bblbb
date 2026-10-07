<script lang="ts">
  import { navigating } from '$app/state';
  import { onDestroy } from 'svelte';

  interface Props {
    /** 外部或测试覆盖导航状态（缺省时读取 SvelteKit 原生 navigating） */
    testNavigating?: unknown;
  }

  let { testNavigating }: Props = $props();

  const isNavigating = $derived(
    testNavigating !== undefined ? Boolean(testNavigating) : Boolean(navigating.to)
  );

  let visible = $state(false);
  let progress = $state(0);
  let fading = $state(false);

  let trickleInterval: ReturnType<typeof setInterval> | null = null;
  let completeTimeout: ReturnType<typeof setTimeout> | null = null;
  let fadeTimeout: ReturnType<typeof setTimeout> | null = null;

  function clearAllTimers() {
    if (trickleInterval) {
      clearInterval(trickleInterval);
      trickleInterval = null;
    }
    if (completeTimeout) {
      clearTimeout(completeTimeout);
      completeTimeout = null;
    }
    if (fadeTimeout) {
      clearTimeout(fadeTimeout);
      fadeTimeout = null;
    }
  }

  function startProgress() {
    clearAllTimers();
    fading = false;
    visible = true;
    progress = 20; // 初始立即给出 20% 反馈

    if (typeof document !== 'undefined') {
      document.documentElement.setAttribute('data-navigating', 'true');
      document.documentElement.setAttribute('aria-busy', 'true');
    }

    // 步进衰减逻辑（Trickle）：渐近推进到 95%，永不卡死停滞
    trickleInterval = setInterval(() => {
      if (progress < 50) {
        progress += 12;
      } else if (progress < 75) {
        progress += 6;
      } else if (progress < 90) {
        progress += 2.5;
      } else if (progress < 95) {
        progress += 0.8;
      }
    }, 200);
  }

  function finishProgress() {
    if (!visible) return;

    if (trickleInterval) {
      clearInterval(trickleInterval);
      trickleInterval = null;
    }

    progress = 100;

    completeTimeout = setTimeout(() => {
      fading = true;
      fadeTimeout = setTimeout(() => {
        visible = false;
        fading = false;
        progress = 0;
        if (typeof document !== 'undefined') {
          document.documentElement.removeAttribute('data-navigating');
          document.documentElement.removeAttribute('aria-busy');
        }
      }, 200);
    }, 150);
  }

  $effect(() => {
    if (isNavigating) {
      startProgress();
    } else {
      finishProgress();
    }
    return clearAllTimers;
  });

  onDestroy(() => {
    clearAllTimers();
    if (typeof document !== 'undefined') {
      document.documentElement.removeAttribute('data-navigating');
      document.documentElement.removeAttribute('aria-busy');
    }
  });
</script>

{#if visible}
  <div
    class="nav-progress-bar"
    class:is-fading={fading}
    role="progressbar"
    aria-label="页面加载中"
    aria-valuemin={0}
    aria-valuemax={100}
    aria-valuenow={Math.round(progress)}
  >
    <div
      class="nav-progress-fill"
      style="width: {progress}%;"
    >
      <div class="nav-progress-peg"></div>
    </div>
  </div>
{/if}

<style>
  .nav-progress-bar {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    height: 3px;
    z-index: 99999;
    pointer-events: none;
    background: transparent;
    opacity: 1;
    transition: opacity 200ms ease;
  }

  .nav-progress-bar.is-fading {
    opacity: 0;
  }

  .nav-progress-fill {
    position: relative;
    height: 100%;
    background: linear-gradient(
      90deg,
      var(--color-brand),
      var(--color-accent)
    );
    box-shadow: 0 0 8px color-mix(in srgb, var(--color-brand) 60%, transparent);
    transition: width 200ms cubic-bezier(0.4, 0, 0.2, 1);
  }

  .nav-progress-peg {
    position: absolute;
    right: 0;
    top: 0;
    bottom: 0;
    width: 70px;
    opacity: 0.85;
    box-shadow:
      0 0 10px var(--color-brand),
      0 0 5px var(--color-accent);
    transform: rotate(3deg) translateY(-2px);
  }

  @media (prefers-reduced-motion: reduce) {
    .nav-progress-bar {
      transition: none;
    }
    .nav-progress-fill {
      transition: none;
    }
    .nav-progress-peg {
      display: none;
    }
  }
</style>
