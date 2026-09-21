import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, waitFor } from '@testing-library/svelte';
import AttachmentPicker from './AttachmentPicker.svelte';
import * as client from '$lib/api/client';
import type { Attachment } from '$lib/api/types';

vi.mock('$lib/api/client', () => ({
  attachmentContentUrl: vi.fn((id: string) => `/api/v1/attachments/${id}/content`),
  listMyAttachments: vi.fn(),
}));

const mocked = client as unknown as {
  listMyAttachments: ReturnType<typeof vi.fn>;
  attachmentContentUrl: ReturnType<typeof vi.fn>;
};

const mockAttachments: Attachment[] = [
  {
    id: 'att-img-1',
    owner_id: 'user-1',
    storage_backend: 'local',
    original_name: 'photo1.jpg',
    media_type: 'image/jpeg',
    size_bytes: 1024,
    status: 'ready',
    is_public: true,
    created_at: 1000
  },
  {
    id: 'att-img-2',
    owner_id: 'user-1',
    storage_backend: 'local',
    original_name: 'photo2.png',
    media_type: 'image/png',
    size_bytes: 2048,
    status: 'ready',
    is_public: true,
    created_at: 1000
  },
  {
    id: 'att-doc-3',
    owner_id: 'user-1',
    storage_backend: 'local',
    original_name: 'doc.pdf',
    media_type: 'application/pdf',
    size_bytes: 4096,
    status: 'ready',
    is_public: true,
    created_at: 1000
  }
];

beforeEach(() => {
  vi.clearAllMocks();
});

describe('AttachmentPicker 预览防雪崩与容灾测试', () => {
  it('正确加载并渲染已就绪附件列表', async () => {
    mocked.listMyAttachments.mockResolvedValueOnce({
      items: mockAttachments,
      quota: null
    });

    const { container, getByText } = render(AttachmentPicker, { props: {} });

    await waitFor(() => expect(getByText('photo1.jpg')).toBeInTheDocument());
    expect(getByText('photo2.png')).toBeInTheDocument();
    expect(getByText('doc.pdf')).toBeInTheDocument();

    const imgs = container.querySelectorAll('img.picker-thumb');
    expect(imgs).toHaveLength(2);
    expect(imgs[0].getAttribute('src')).toBe('/api/v1/attachments/att-img-1/content');
    expect(imgs[1].getAttribute('src')).toBe('/api/v1/attachments/att-img-2/content');
  });

  it('单个图片加载失败时仅重试该图片，不影响其他图片且最多重试 1 次后降级', async () => {
    mocked.listMyAttachments.mockResolvedValueOnce({
      items: mockAttachments,
      quota: null
    });

    const { container } = render(AttachmentPicker, { props: {} });

    await waitFor(() => expect(container.querySelectorAll('img.picker-thumb')).toHaveLength(2));

    const [img1, img2] = container.querySelectorAll('img.picker-thumb');
    expect(img1.getAttribute('src')).toBe('/api/v1/attachments/att-img-1/content');
    expect(img2.getAttribute('src')).toBe('/api/v1/attachments/att-img-2/content');

    // 触发 img1 第 1 次 onerror（重试）
    await fireEvent.error(img1);

    // img1 带有 retry 参数，而 img2 保持原 URL 不受任何影响（防全局联动雪崩）
    const updatedImg1 = container.querySelector('img.picker-thumb[src*="att-img-1"]') as HTMLImageElement;
    const updatedImg2 = container.querySelector('img.picker-thumb[src*="att-img-2"]') as HTMLImageElement;
    expect(updatedImg1).not.toBeNull();
    expect(updatedImg1.getAttribute('src')).toBe('/api/v1/attachments/att-img-1/content?r=1');
    expect(updatedImg2.getAttribute('src')).toBe('/api/v1/attachments/att-img-2/content');

    // 触发 img1 第 2 次 onerror（超过重试上限，必须彻底降级停止请求）
    await fireEvent.error(updatedImg1);

    // img1 元素应被移除，替换为错误占位图标，不再有可报错的 img 标签
    expect(container.querySelector('img.picker-thumb[src*="att-img-1"]')).toBeNull();
    expect(container.querySelector('span[title="预览加载失败"]')).not.toBeNull();

    // img2 依然安然无恙
    expect(container.querySelector('img.picker-thumb[src*="att-img-2"]')).not.toBeNull();
  });
});
