// M10-UI-01/02：VideoInsertPanel 编辑器面板测试。
//
// - 手动 URL 输入 + 格式提示（仅提示，后端重新解析）；
// - resolve 预览：Provider 状态、标题、时长、可嵌入性、错误说明；
// - 只回调投影白名单后的结果（对抗性 Secret/内部字段不进入 onAccept）；
// - 409 feature_disabled → 关闭说明（不影响发帖）；不可嵌入 → 外链降级。
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, waitFor } from '@testing-library/svelte';
import VideoInsertPanel from './VideoInsertPanel.svelte';

function rawResolve(overrides: Record<string, unknown> = {}) {
  return {
    resolution_id: 'res-1',
    provider: 'xigua',
    title: '测试视频',
    poster_url: 'https://cdn.xigua.example/cover.webp',
    official_url: 'https://player.xigua.example/embed/1',
    source_url: 'https://www.xigua.example/video/1',
    duration_seconds: 120,
    policy_version: 2,
    embeddable: true,
    provider_status: { provider: 'xigua', enabled: true, available: true },
    ...overrides
  };
}

async function typeUrl(container: HTMLElement, url: string) {
  const input = container.querySelector('#video-insert-url') as HTMLInputElement;
  await fireEvent.input(input, { target: { value: url } });
  return input;
}

async function submitForm(container: HTMLElement) {
  const form = container.querySelector('form')!;
  await fireEvent.submit(form);
}

function findButton(container: HTMLElement, text: string): HTMLButtonElement {
  const button = Array.from(container.querySelectorAll('button')).find((b) => b.textContent?.includes(text));
  if (!button) throw new Error(`未找到按钮「${text}」`);
  return button as HTMLButtonElement;
}

beforeEach(() => vi.clearAllMocks());

describe('M10-UI-01 URL 输入与格式提示', () => {
  it('初始 SSR 态：表单 + 提示，无敏感信息', () => {
    const { container } = render(VideoInsertPanel, {
      props: { onResolve: vi.fn(), onAccept: vi.fn() }
    });
    expect(container.querySelector('#video-insert-url')).not.toBeNull();
    expect(container.textContent).toContain('插入视频');
    expect(container.textContent).toContain('仅格式提示，实际校验由服务端完成');
  });

  it('非 https 输入 → 前端格式提示（role=alert），不触发 resolve', async () => {
    const onResolve = vi.fn();
    const { container } = render(VideoInsertPanel, { props: { onResolve, onAccept: vi.fn() } });
    await typeUrl(container, 'ftp://example.com/v.mp4');
    await submitForm(container);
    expect(onResolve).not.toHaveBeenCalled();
    expect(container.textContent).toContain('链接必须以 https:// 开头');
    expect(container.querySelector('#video-insert-url')).toHaveAttribute('aria-invalid', 'true');
  });
});

describe('M10-UI-01 resolve 预览与 Provider 状态', () => {
  it('解析成功 → 预览标题/Provider/时长/状态，插入按钮回调白名单结果', async () => {
    const onResolve = vi.fn().mockResolvedValue(rawResolve());
    const onAccept = vi.fn();
    const { container } = render(VideoInsertPanel, { props: { onResolve, onAccept } });
    await typeUrl(container, 'https://www.xigua.example/video/1');
    await submitForm(container);
    await waitFor(() => expect(container.textContent).toContain('测试视频'));
    expect(container.textContent).toContain('西瓜视频');
    expect(container.textContent).toContain('02:00');
    expect(container.textContent).toContain('Provider 可用');
    expect(container.textContent).toContain('可安全嵌入播放');

    await fireEvent.click(findButton(container, '插入视频'));
    expect(onAccept).toHaveBeenCalledTimes(1);
    const accepted = onAccept.mock.calls[0][0] as Record<string, unknown>;
    expect(accepted.resolution_id).toBe('res-1');
    expect(accepted.embeddable).toBe(true);
    expect(container.textContent).toContain('已加入视频引用');
  });

  it('不可嵌入 → 以外链方式插入 + 降级原因', async () => {
    const onAccept = vi.fn();
    const { container } = render(VideoInsertPanel, {
      props: {
        onResolve: vi.fn().mockResolvedValue(rawResolve({ embeddable: false, degraded_reason: 'no_embed_permission', official_url: null })),
        onAccept
      }
    });
    await typeUrl(container, 'https://www.xigua.example/video/2');
    await submitForm(container);
    await waitFor(() => expect(container.textContent).toContain('来源未授权嵌入，仅可外链打开'));
    await fireEvent.click(findButton(container, '以外链方式插入'));
    expect(onAccept).toHaveBeenCalledTimes(1);
    expect(onAccept.mock.calls[0][0].embeddable).toBe(false);
  });

  it('解析失败 → 错误说明（role=alert）', async () => {
    const { container } = render(VideoInsertPanel, {
      props: {
        onResolve: vi.fn().mockRejectedValue({ status: 400, code: 'invalid_url', detail: 'bad url' }),
        onAccept: vi.fn()
      }
    });
    await typeUrl(container, 'https://example.com/broken');
    await submitForm(container);
    await waitFor(() => expect(container.querySelector('[role="alert"]')?.textContent).toContain('链接地址无效'));
  });
});

describe('M10-UI-02 只提交允许字段（Secret 不进浏览器状态）', () => {
  it('对抗性 resolve 响应：预览与 onAccept 均不含 Secret/内部字段', async () => {
    const onResolve = vi.fn().mockResolvedValue(
      rawResolve({
        secret_key: 'XIGUA-SECRET-KEY',
        signed_play_url: 'https://internal.example/signed?sig=XYZ',
        access_token: 'TOKEN-123'
      })
    );
    const onAccept = vi.fn();
    const { container } = render(VideoInsertPanel, { props: { onResolve, onAccept } });
    await typeUrl(container, 'https://www.xigua.example/video/1');
    await submitForm(container);
    await waitFor(() => expect(container.textContent).toContain('测试视频'));
    expect(container.textContent).not.toContain('XIGUA-SECRET-KEY');
    expect(container.textContent).not.toContain('TOKEN-123');

    await fireEvent.click(findButton(container, '插入视频'));
    const accepted = onAccept.mock.calls[0][0] as Record<string, unknown>;
    expect(accepted).not.toHaveProperty('secret_key');
    expect(accepted).not.toHaveProperty('signed_play_url');
    expect(accepted).not.toHaveProperty('access_token');
  });
});

describe('M10-UI-01 Feature Flag 关闭与认证降级', () => {
  it('409 feature_disabled → 关闭说明（role=status），不渲染错误', async () => {
    const { container } = render(VideoInsertPanel, {
      props: {
        onResolve: vi.fn().mockRejectedValue({ status: 409, code: 'feature_disabled', detail: 'disabled' }),
        onAccept: vi.fn()
      }
    });
    await typeUrl(container, 'https://example.com/v.mp4');
    await submitForm(container);
    await waitFor(() => expect(container.querySelector('[role="status"]')?.textContent).toContain('视频功能未开放'));
    expect(container.querySelector('[role="alert"]')).toBeNull();
  });

  it('401 未登录 → 登录提示（后端透传文案）', async () => {
    const { container } = render(VideoInsertPanel, {
      props: {
        onResolve: vi.fn().mockRejectedValue({ status: 401, code: 'authentication_required', detail: 'x' }),
        onAccept: vi.fn()
      }
    });
    await typeUrl(container, 'https://example.com/v.mp4');
    await submitForm(container);
    await waitFor(() => expect(container.querySelector('[role="alert"]')?.textContent).toContain('请先登录'));
  });
});
