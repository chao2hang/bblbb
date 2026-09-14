import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { infiniteScroll, type InfiniteScrollOptions } from './infinite-scroll';

describe('infiniteScroll action', () => {
  let callbacks: ((entries: IntersectionObserverEntry[]) => void)[] = [];
  let observedNodes: HTMLElement[] = [];
  let disconnectMock: ReturnType<typeof vi.fn>;
  let originalIntersectionObserver: typeof window.IntersectionObserver;

  beforeEach(() => {
    callbacks = [];
    observedNodes = [];
    disconnectMock = vi.fn();
    originalIntersectionObserver = window.IntersectionObserver;

    // Mock IntersectionObserver
    window.IntersectionObserver = vi.fn().mockImplementation((cb) => {
      callbacks.push(cb);
      return {
        observe: vi.fn((node: HTMLElement) => {
          observedNodes.push(node);
        }),
        unobserve: vi.fn(),
        disconnect: disconnectMock
      };
    }) as unknown as typeof window.IntersectionObserver;
  });

  afterEach(() => {
    window.IntersectionObserver = originalIntersectionObserver;
    vi.restoreAllMocks();
  });

  it('在没有 IntersectionObserver 环境下安全运行（无异常）', () => {
    // @ts-expect-error test without IntersectionObserver
    delete window.IntersectionObserver;
    const node = document.createElement('div');
    const onLoadMore = vi.fn();

    const handle = infiniteScroll(node, {
      onLoadMore,
      hasMore: true
    });

    expect(handle).toBeDefined();
    expect(() => handle.update({ onLoadMore, hasMore: false })).not.toThrow();
    expect(() => handle.destroy()).not.toThrow();
    expect(onLoadMore).not.toHaveBeenCalled();
  });

  it('元素进入视口（isIntersecting=true）且有更多数据时自动触发 onLoadMore', () => {
    const node = document.createElement('div');
    const onLoadMore = vi.fn();

    infiniteScroll(node, {
      onLoadMore,
      hasMore: true,
      loading: false
    });

    expect(window.IntersectionObserver).toHaveBeenCalled();
    expect(callbacks.length).toBe(1);

    // 模拟相交事件
    callbacks[0]([{ isIntersecting: true } as IntersectionObserverEntry]);
    expect(onLoadMore).toHaveBeenCalledTimes(1);
  });

  it('loading 为 true 时即使进入视口也不重复触发', () => {
    const node = document.createElement('div');
    const onLoadMore = vi.fn();

    infiniteScroll(node, {
      onLoadMore,
      hasMore: true,
      loading: true
    });

    callbacks[0]([{ isIntersecting: true } as IntersectionObserverEntry]);
    expect(onLoadMore).not.toHaveBeenCalled();
  });

  it('hasMore 为 false 时不触发', () => {
    const node = document.createElement('div');
    const onLoadMore = vi.fn();

    infiniteScroll(node, {
      onLoadMore,
      hasMore: false,
      loading: false
    });

    callbacks[0]([{ isIntersecting: true } as IntersectionObserverEntry]);
    expect(onLoadMore).not.toHaveBeenCalled();
  });

  it('disabled 为 true 时暂停自动触发', () => {
    const node = document.createElement('div');
    const onLoadMore = vi.fn();

    const handle = infiniteScroll(node, {
      onLoadMore,
      hasMore: true,
      loading: false,
      disabled: true
    });

    callbacks[0]([{ isIntersecting: true } as IntersectionObserverEntry]);
    expect(onLoadMore).not.toHaveBeenCalled();

    // 更新为解除禁用
    handle.update({
      onLoadMore,
      hasMore: true,
      loading: false,
      disabled: false
    });

    callbacks[0]([{ isIntersecting: true } as IntersectionObserverEntry]);
    expect(onLoadMore).toHaveBeenCalledTimes(1);
  });

  it('调用 destroy 时断开 observer', () => {
    const node = document.createElement('div');
    const onLoadMore = vi.fn();

    const handle = infiniteScroll(node, {
      onLoadMore,
      hasMore: true
    });

    handle.destroy();
    expect(disconnectMock).toHaveBeenCalled();
  });

  it('distance 改变时重建 observer 并重新监听', () => {
    const node = document.createElement('div');
    const onLoadMore = vi.fn();

    const handle = infiniteScroll(node, {
      onLoadMore,
      hasMore: true,
      distance: 200
    });

    expect(window.IntersectionObserver).toHaveBeenCalledTimes(1);

    handle.update({
      onLoadMore,
      hasMore: true,
      distance: 400
    });

    expect(window.IntersectionObserver).toHaveBeenCalledTimes(2);
    expect(disconnectMock).toHaveBeenCalledTimes(1);
  });

  it('加载完成后若依然在视口内可自动连续触发', async () => {
    const node = document.createElement('div');
    vi.spyOn(node, 'getBoundingClientRect').mockReturnValue({
      top: 100,
      bottom: 200,
      left: 0,
      right: 100,
      width: 100,
      height: 100,
      x: 0,
      y: 100,
      toJSON: () => {}
    });

    const onLoadMore = vi.fn();
    const handle = infiniteScroll(node, {
      onLoadMore,
      hasMore: true,
      loading: true
    });

    // 触发相交（此时 loading 为 true，不执行）
    callbacks[0]([{ isIntersecting: true } as IntersectionObserverEntry]);
    expect(onLoadMore).not.toHaveBeenCalled();

    // 加载完成更新状态
    handle.update({
      onLoadMore,
      hasMore: true,
      loading: false
    });

    // 等待 requestAnimationFrame
    await new Promise((r) => requestAnimationFrame(r));
    expect(onLoadMore).toHaveBeenCalledTimes(1);
  });
});
