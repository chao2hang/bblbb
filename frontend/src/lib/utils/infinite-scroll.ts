/**
 * 滚动到底部自动加载 Action (infiniteScroll)
 *
 * 用于列表底部「加载更多」或哨兵元素。
 * 基于浏览器原生 IntersectionObserver 实现，在元素接近或进入视口时自动触发 onLoadMore。
 *
 * 特性：
 * - 纯原生高效监听（无 scroll 事件抖动与重排开销）；
 * - 完备的防重复触发保护（loading / disabled / hasMore 状态门禁）；
 * - 异常容错与手动重试友好（加载失败后可置 disabled=true 暂停自动触发，等待用户点击重试）；
 * - SSR 与无 JS 完全向后兼容（服务端无副作用，配合 原生链接/按钮 优雅降级）；
 * - 大屏内容未填满自适应（若一页数据不足以将加载按钮推出视口，在单次请求完成后继续检测并补齐）。
 */

export interface InfiniteScrollOptions {
  /** 触发加载下一页的回调函数 */
  onLoadMore: () => void | Promise<void>;
  /** 是否还有更多数据待加载 */
  hasMore: boolean;
  /** 当前是否正在加载中（为 true 时禁止重复触发） */
  loading?: boolean;
  /** 距离视口底部的提前触发距离（px），默认 250 */
  distance?: number;
  /** 是否禁用自动触发（如出错重试状态、用户关闭自动翻页等） */
  disabled?: boolean;
  /** 自定义滚动容器，默认 null 即浏览器视口 (viewport) */
  root?: HTMLElement | null;
}

export interface InfiniteScrollHandle {
  update(options: InfiniteScrollOptions): void;
  destroy(): void;
}

export function infiniteScroll(node: HTMLElement, options: InfiniteScrollOptions): InfiniteScrollHandle {
  let {
    onLoadMore,
    hasMore,
    loading = false,
    distance = 250,
    disabled = false,
    root = null
  } = options;

  let observer: IntersectionObserver | null = null;
  let isIntersecting = false;
  let scheduledCheck: number | null = null;

  function trigger(): void {
    if (hasMore && !loading && !disabled) {
      void onLoadMore();
    }
  }

  function handleIntersect(entries: IntersectionObserverEntry[]): void {
    const entry = entries[0];
    if (!entry) return;
    isIntersecting = entry.isIntersecting;
    if (isIntersecting) {
      trigger();
    }
  }

  function setupObserver(): void {
    if (typeof window === 'undefined' || typeof IntersectionObserver === 'undefined') return;
    cleanupObserver();
    try {
      observer = new IntersectionObserver(handleIntersect, {
        root,
        rootMargin: `0px 0px ${distance}px 0px`,
        threshold: 0
      });
      observer.observe(node);
    } catch {
      observer = null;
    }
  }

  function cleanupObserver(): void {
    if (observer) {
      observer.disconnect();
      observer = null;
    }
    if (scheduledCheck !== null && typeof cancelAnimationFrame !== 'undefined') {
      cancelAnimationFrame(scheduledCheck);
      scheduledCheck = null;
    }
  }

  function checkNearViewport(): boolean {
    if (typeof window === 'undefined') return false;
    const rect = node.getBoundingClientRect();
    const viewportHeight = root
      ? root.clientHeight
      : window.innerHeight || document.documentElement.clientHeight;
    return rect.top <= viewportHeight + distance && rect.bottom >= 0;
  }

  setupObserver();

  return {
    update(newOptions: InfiniteScrollOptions): void {
      const prevDistance = distance;
      const prevRoot = root;
      const wasLoading = loading;

      onLoadMore = newOptions.onLoadMore;
      hasMore = newOptions.hasMore;
      loading = newOptions.loading ?? false;
      distance = newOptions.distance ?? 250;
      disabled = newOptions.disabled ?? false;
      root = newOptions.root ?? null;

      if (prevDistance !== distance || prevRoot !== root) {
        setupObserver();
      }

      // 如果之前正在加载，现在加载完成（wasLoading && !loading），
      // 且依然有更多数据、未被禁用，检查当前节点是否仍在视口内（例如高分辨率大屏未占满）
      if (wasLoading && !loading && hasMore && !disabled) {
        if (typeof requestAnimationFrame !== 'undefined') {
          if (scheduledCheck !== null) cancelAnimationFrame(scheduledCheck);
          scheduledCheck = requestAnimationFrame(() => {
            scheduledCheck = null;
            if (hasMore && !loading && !disabled && (isIntersecting || checkNearViewport())) {
              trigger();
            }
          });
        }
      }
    },
    destroy(): void {
      cleanupObserver();
    }
  };
}
