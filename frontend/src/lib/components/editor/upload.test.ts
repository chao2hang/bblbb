import { describe, expect, it, vi } from 'vitest';
import { formatUploadErrorMessage, uploadEditorAttachment } from './upload';
import * as client from '$lib/api/client';

vi.mock('$lib/api/client', () => ({
  createAttachment: vi.fn(),
  completeAttachment: vi.fn(),
  attachmentContentUrl: vi.fn((id: string) => `/api/v1/attachments/${id}/content`),
  newClientRequestId: vi.fn(() => 'test-req-id')
}));

describe('upload module', () => {
  it('formatUploadErrorMessage 能正确提取后端 Problem 错误文案', () => {
    // RFC 9457 Problem
    expect(
      formatUploadErrorMessage({
        code: 'authentication_required',
        status: 401
      })
    ).toContain('请先登录');

    expect(
      formatUploadErrorMessage({
        detail: 'Idempotency-Key header is required',
        status: 400
      })
    ).toBe('Idempotency-Key header is required');

    expect(formatUploadErrorMessage(new Error('S3 权限不足'))).toBe('S3 权限不足');
    expect(formatUploadErrorMessage(null)).toBe('上传服务未就绪');
  });

  it('uploadEditorAttachment 正常走完创建并返回 URL', async () => {
    const mockedClient = vi.mocked(client);
    mockedClient.createAttachment.mockResolvedValueOnce({
      id: 'att-123',
      upload: {
        mode: 'presigned_put',
        url: null
      }
    });
    mockedClient.completeAttachment.mockResolvedValueOnce({
      id: 'att-123',
      owner_id: 'u1',
      status: 'ready',
      media_type: 'image/png',
      size_bytes: 1024,
      original_name: 'test.png',
      created_at: 1000
    });

    const file = new File(['test'], 'test.png', { type: 'image/png' });
    const res = await uploadEditorAttachment(file);

    expect(res.id).toBe('att-123');
    expect(res.isImage).toBe(true);
    expect(res.url).toBe('/api/v1/attachments/att-123/content');
  });
});
