// M06-UI-01..04：附件上传器组件测试——选择→创建→预签名 PUT（进度/取消/
// 重试/URL 过期重签）→complete→onReady；进度条与错误播报。
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, waitFor } from '@testing-library/svelte';
import AttachmentUploader from './AttachmentUploader.svelte';
import * as client from '$lib/api/client';

vi.mock('$lib/api/client', () => ({
  attachmentContentUrl: vi.fn((id: string) => `/api/v1/attachments/${id}/content`),
  createAttachment: vi.fn(),
  completeAttachment: vi.fn(),
  deleteAttachment: vi.fn(async () => undefined),
  getAttachment: vi.fn(),
  listMyAttachments: vi.fn(async () => ({ items: [], quota: null })),
  newClientRequestId: vi.fn(() => 'req-1234567890123456')
}));

const mocked = client as unknown as {
  createAttachment: ReturnType<typeof vi.fn>;
  completeAttachment: ReturnType<typeof vi.fn>;
  deleteAttachment: ReturnType<typeof vi.fn>;
};

/** 可编程的 XHR 桩：捕获实例供测试逐步驱动（进度/完成/失败/中止）。 */
class FakeXHR {
  static instances: FakeXHR[] = [];
  upload: { onprogress: ((ev: { lengthComputable: boolean; loaded: number; total: number }) => void) | null };
  onload: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onabort: (() => void) | null = null;
  status = 0;
  openedUrl = '';
  headers: Record<string, string> = {};
  aborted = false;
  sent = false;
  constructor() {
    this.upload = { onprogress: null };
    FakeXHR.instances.push(this);
  }
  open(_method: string, url: string): void {
    this.openedUrl = url;
  }
  setRequestHeader(k: string, v: string): void {
    this.headers[k] = v;
  }
  send(_body: unknown): void {
    this.sent = true;
  }
  abort(): void {
    this.aborted = true;
  }
  /** 测试辅助：完成上传。 */
  finish(status: number): void {
    this.status = status;
    this.onload?.();
  }
  /** 测试辅助：网络错误。 */
  failNetwork(): void {
    this.onerror?.();
  }
}

function makeFile(): File {
  return new File(['hello world'], 'hello.png', { type: 'image/png' });
}

async function selectFile(container: HTMLElement): Promise<HTMLInputElement> {
  const input = container.querySelector('input[type="file"]') as HTMLInputElement;
  const file = makeFile();
  Object.defineProperty(input, 'files', { value: [file], configurable: true });
  await fireEvent.change(input); // await 使 Svelte 5 状态刷新（microtask）
  return input;
}

beforeEach(() => {
  FakeXHR.instances = [];
  vi.clearAllMocks();
  (globalThis as unknown as { XMLHttpRequest: typeof XMLHttpRequest }).XMLHttpRequest = FakeXHR as unknown as typeof XMLHttpRequest;
});

describe('M06-UI-01 上传流程', () => {
  it('选择文件 → 开始上传 → presigned PUT（进度）→ complete → onReady', async () => {
    mocked.createAttachment.mockResolvedValueOnce({
      id: 'a1',
      upload: { mode: 'presigned_put', url: 'https://s3.example.com/put', headers: { 'Content-Type': 'image/png' } },
      quota: null
    });
    mocked.completeAttachment.mockResolvedValueOnce({ id: 'a1', media_type: 'image/png', size_bytes: 11, status: 'ready', created_at: 0 });

    const onReady = vi.fn();
    const { container, getByText } = render(AttachmentUploader, { props: { showQuota: false, waitReady: false, onReady } });

    await selectFile(container);
    await fireEvent.click(getByText('开始上传'));
    await waitFor(() => expect(mocked.createAttachment).toHaveBeenCalledTimes(1));

    expect(FakeXHR.instances).toHaveLength(1);
    const xhr = FakeXHR.instances[0];
    expect(xhr.openedUrl).toBe('https://s3.example.com/put');
    expect(xhr.sent).toBe(true);

    // 进度：5/10 → 50%
    xhr.upload.onprogress?.({ lengthComputable: true, loaded: 5, total: 10 });
    await waitFor(() => expect(container.textContent).toContain('50%'));

    xhr.finish(200);
    await waitFor(() => expect(mocked.completeAttachment).toHaveBeenCalledWith(expect.anything(), 'a1', 'req-1234567890123456'));
    await waitFor(() => expect(onReady).toHaveBeenCalledWith(expect.objectContaining({ id: 'a1', status: 'ready' })));
    await waitFor(() => expect(container.textContent).toContain('上传完成'));
  });

  it('进度条带 role=progressbar；错误用 role=alert 播报', async () => {
    mocked.createAttachment.mockResolvedValueOnce({ id: 'a1', upload: { mode: 'presigned_put', url: 'https://s3.example.com/put' }, quota: null });
    mocked.completeAttachment.mockResolvedValueOnce({ id: 'a1', media_type: 'image/png', size_bytes: 11, status: 'ready', created_at: 0 });
    const { container, getByText } = render(AttachmentUploader, { props: { showQuota: false, waitReady: false } });
    await selectFile(container);
    await fireEvent.click(getByText('开始上传'));
    await waitFor(() => expect(FakeXHR.instances.length).toBe(1));
    const xhr = FakeXHR.instances[0];
    xhr.upload.onprogress?.({ lengthComputable: true, loaded: 3, total: 10 });
    await waitFor(() => expect(container.querySelector('[role="progressbar"]')).not.toBeNull());
    xhr.finish(500); // 上传失败
    await waitFor(() => expect(container.querySelector('[role="alert"]')).not.toBeNull());
    expect(container.textContent).toContain('重试');
  });

  it('取消：中止 XHR 并尽力删除服务端附件', async () => {
    mocked.createAttachment.mockResolvedValueOnce({ id: 'a1', upload: { mode: 'presigned_put', url: 'https://s3.example.com/put' }, quota: null });
    const { container, getByText } = render(AttachmentUploader, { props: { showQuota: false, waitReady: false } });
    await selectFile(container);
    await fireEvent.click(getByText('开始上传'));
    await waitFor(() => expect(FakeXHR.instances.length).toBe(1));
    const xhr = FakeXHR.instances[0];
    await fireEvent.click(getByText('取消'));
    expect(xhr.aborted).toBe(true);
    await waitFor(() => expect(mocked.deleteAttachment).toHaveBeenCalled());
  });

  it('URL 过期（403）→ 自动重新 create 获取新 URL，不删除附件', async () => {
    mocked.createAttachment
      .mockResolvedValueOnce({ id: 'a1', upload: { mode: 'presigned_put', url: 'https://s3.example.com/expired', headers: {} }, quota: null })
      .mockResolvedValueOnce({ id: 'a1', upload: { mode: 'presigned_put', url: 'https://s3.example.com/fresh', headers: {} }, quota: null });
    mocked.completeAttachment.mockResolvedValue({ id: 'a1', media_type: 'image/png', size_bytes: 11, status: 'ready', created_at: 0 });

    const { container, getByText } = render(AttachmentUploader, { props: { showQuota: false, waitReady: false } });
    await selectFile(container);
    await fireEvent.click(getByText('开始上传'));
    await waitFor(() => expect(FakeXHR.instances.length).toBe(1));
    FakeXHR.instances[0].finish(403); // 签名过期

    await waitFor(() => expect(mocked.createAttachment).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(FakeXHR.instances.length).toBe(2));
    expect(FakeXHR.instances[1].openedUrl).toBe('https://s3.example.com/fresh');
    FakeXHR.instances[1].finish(200);
    await waitFor(() => expect(mocked.completeAttachment).toHaveBeenCalledTimes(1));
    expect(mocked.deleteAttachment).not.toHaveBeenCalled();
    await waitFor(() => expect(container.textContent).toContain('上传完成'));
  });

  it('重试：失败后点击重试重新走完整流程', async () => {
    mocked.createAttachment
      .mockResolvedValueOnce({ id: 'a1', upload: { mode: 'presigned_put', url: 'https://s3.example.com/put1' }, quota: null })
      .mockResolvedValueOnce({ id: 'a2', upload: { mode: 'presigned_put', url: 'https://s3.example.com/put2' }, quota: null });
    mocked.completeAttachment.mockResolvedValue({ id: 'a2', media_type: 'image/png', size_bytes: 11, status: 'ready', created_at: 0 });

    const { container, getByText } = render(AttachmentUploader, { props: { showQuota: false, waitReady: false } });
    await selectFile(container);
    await fireEvent.click(getByText('开始上传'));
    await waitFor(() => expect(FakeXHR.instances.length).toBe(1));
    FakeXHR.instances[0].failNetwork(); // 网络错误
    await waitFor(() => expect(getByText('重试')).toBeTruthy());
    await fireEvent.click(getByText('重试'));
    await waitFor(() => expect(mocked.createAttachment).toHaveBeenCalledTimes(2));
    expect(FakeXHR.instances[1].openedUrl).toBe('https://s3.example.com/put2');
    FakeXHR.instances[1].finish(200);
    await waitFor(() => expect(container.textContent).toContain('上传完成'));
  });

  it('文件类型校验：不允许的类型显示错误', async () => {
    const { container } = render(AttachmentUploader, { props: { showQuota: false, accept: 'image/*', waitReady: false } });
    const input = container.querySelector('input[type="file"]') as HTMLInputElement;
    const file = new File(['x'], 'evil.sh', { type: 'text/x-sh' });
    Object.defineProperty(input, 'files', { value: [file], configurable: true });
    fireEvent.change(input);
    await waitFor(() => expect(container.querySelector('[role="alert"]')).not.toBeNull());
    expect(container.textContent).toContain('文件类型不被允许');
    expect(mocked.createAttachment).not.toHaveBeenCalled();
  });

  it('键盘可达：label[for] 指向文件输入', async () => {
    const { container } = render(AttachmentUploader, { props: { showQuota: false, label: '选择文件' } });
    const label = container.querySelector('label') as HTMLLabelElement;
    const input = container.querySelector('input[type="file"]') as HTMLInputElement;
    expect(label.getAttribute('for')).toBe(input.id);
  });

  it('本地直传（无 presigned URL）→ 创建后直接 complete', async () => {
    mocked.createAttachment.mockResolvedValueOnce({ id: 'a1', upload: { mode: 'local', url: null }, quota: null });
    mocked.completeAttachment.mockResolvedValueOnce({ id: 'a1', media_type: 'image/png', size_bytes: 11, status: 'ready', created_at: 0 });
    const { container, getByText } = render(AttachmentUploader, { props: { showQuota: false, waitReady: false } });
    await selectFile(container);
    await fireEvent.click(getByText('开始上传'));
    await waitFor(() => expect(mocked.completeAttachment).toHaveBeenCalledTimes(1));
    expect(FakeXHR.instances).toHaveLength(0);
    await waitFor(() => expect(container.textContent).toContain('上传完成'));
  });
});
