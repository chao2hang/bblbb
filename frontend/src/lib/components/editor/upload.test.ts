import { afterEach, describe, expect, it, vi } from 'vitest';
import { formatUploadErrorMessage, uploadEditorAttachment } from './upload';
import { resetUploadPolicyCache } from '$lib/upload/mediaTypes';
import * as client from '$lib/api/client';

vi.mock('$lib/api/client', () => ({
  createAttachment: vi.fn(),
  completeAttachment: vi.fn(),
  attachmentContentUrl: vi.fn((id: string) => `/api/v1/attachments/${id}/content`),
  newClientRequestId: vi.fn(() => 'test-req-id')
}));

describe('upload module', () => {
  afterEach(() => {
    resetUploadPolicyCache();
    vi.clearAllMocks();
  });

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
    // 策略拉取失败 → 乐观全量白名单，扩展名归一化生效
    expect(mockedClient.createAttachment).toHaveBeenCalledWith(
      expect.anything(), // fetchFn
      expect.objectContaining({ declared_media_type: 'image/png' })
    );
  });

  it('扩展名归一化：浏览器未识别的 docx/csv 按扩展名声明', async () => {
    const mockedClient = vi.mocked(client);
    mockedClient.createAttachment.mockResolvedValueOnce({
      id: 'att-doc',
      upload: { mode: 'presigned_put', url: null }
    });
    mockedClient.completeAttachment.mockResolvedValueOnce({
      id: 'att-doc',
      owner_id: 'u1',
      status: 'ready',
      media_type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
      size_bytes: 10,
      original_name: 'doc.docx',
      created_at: 1000
    });

    // 浏览器对 docx 常报空 type
    const docx = new File(['doc'], 'doc.docx', { type: '' });
    const res = await uploadEditorAttachment(docx);
    expect(res.isImage).toBe(false);
    expect(mockedClient.createAttachment).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({
        declared_media_type:
          'application/vnd.openxmlformats-officedocument.wordprocessingml.document'
      })
    );
  });

  it('站点策略关闭的类目在本地预校验即拒绝（不发起 create）', async () => {
    // 策略只放行 image：GET /attachments 返回 allowed_media_types=['image/png']
    const fetchFn = vi.fn(async (input: RequestInfo | URL) => {
      if (String(input).includes('/api/v1/attachments')) {
        return new Response(
          JSON.stringify({ items: [], quota: { allowed_media_types: ['image/png'] } }),
          { status: 200 }
        );
      }
      return new Response('{}', { status: 404 });
    }) as unknown as typeof fetch;

    const pdf = new File(['%PDF-1.4'], 'x.pdf', { type: 'application/pdf' });
    await expect(uploadEditorAttachment(pdf, fetchFn)).rejects.toThrow(/不支持的文件类型/);
    expect(vi.mocked(client).createAttachment).not.toHaveBeenCalled();
  });
});
