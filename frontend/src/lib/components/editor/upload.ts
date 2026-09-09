import {
  createAttachment,
  completeAttachment,
  attachmentContentUrl,
  newClientRequestId
} from '$lib/api/client';
import { problemMessage, type Problem } from '$lib/errors';

export interface UploadResult {
  id: string;
  url: string;
  filename: string;
  isImage: boolean;
  size: number;
}

function resolveMediaType(file: File): string {
  if (file.type && file.type !== 'application/octet-stream') {
    return file.type;
  }
  const lowerName = file.name.toLowerCase();
  if (lowerName.endsWith('.png')) return 'image/png';
  if (lowerName.endsWith('.jpg') || lowerName.endsWith('.jpeg')) return 'image/jpeg';
  if (lowerName.endsWith('.webp')) return 'image/webp';
  if (lowerName.endsWith('.gif')) return 'image/gif';
  if (lowerName.endsWith('.avif')) return 'image/avif';
  if (lowerName.endsWith('.pdf')) return 'application/pdf';
  if (lowerName.endsWith('.txt') || lowerName.endsWith('.md')) return 'text/plain';
  return 'application/octet-stream';
}

export function formatUploadErrorMessage(err: unknown): string {
  if (err instanceof Error) {
    return err.message;
  }
  if (err && typeof err === 'object') {
    const p = err as Problem;
    const msg = problemMessage(p);
    if (msg) return msg;
    const raw = err as Record<string, unknown>;
    if (typeof raw.detail === 'string' && raw.detail.trim()) return raw.detail.trim();
    if (typeof raw.title === 'string' && raw.title.trim()) return raw.title.trim();
    if (typeof raw.message === 'string' && raw.message.trim()) return raw.message.trim();
  }
  return String(err || '上传服务未就绪');
}

export async function uploadEditorAttachment(
  file: File,
  fetchFn: typeof fetch = fetch
): Promise<UploadResult> {
  const mediaType = resolveMediaType(file);
  const isImage = mediaType.startsWith('image/');
  const filename = file.name || (isImage ? 'image.png' : 'attachment');

  const created = await createAttachment(fetchFn, {
    filename,
    size: file.size,
    declared_media_type: mediaType
  });

  const attachmentId = created.id;
  if (!attachmentId) {
    throw new Error('创建附件记录失败，服务端未返回有效附件 ID');
  }

  const upload = created.upload ?? null;
  if (upload && upload.url) {
    const targetUrl = upload.url;
    await new Promise<void>((resolve, reject) => {
      const xhr = new XMLHttpRequest();
      xhr.open('PUT', targetUrl);
      // S3 预签名通常绑定了 Content-Type，必须与声明的 media_type 严格一致
      xhr.setRequestHeader('Content-Type', mediaType);
      if (upload.headers) {
        for (const [k, v] of Object.entries(upload.headers)) {
          xhr.setRequestHeader(k, v);
        }
      }
      xhr.onload = () => {
        if (xhr.status >= 200 && xhr.status < 300) {
          resolve();
        } else if (xhr.status === 403) {
          reject(
            new Error(
              `S3 权限校验拒绝 (HTTP 403)。若使用 S3/MinIO，请检查存储桶策略、PUT 权限与签名配置。`
            )
          );
        } else {
          reject(new Error(`S3 上传请求返回异常状态码: HTTP ${xhr.status}`));
        }
      };
      xhr.onerror = () => {
        reject(
          new Error(
            'S3 上传网络错误或被跨域 (CORS) 拦截。请检查 S3/MinIO 存储桶是否已配置 CORS 允许当前域名的 PUT 请求。'
          )
        );
      };
      xhr.send(file);
    });
  }

  const completed = await completeAttachment(fetchFn, attachmentId, newClientRequestId());
  const finalId = completed.id || attachmentId;
  const url = attachmentContentUrl(finalId);

  return {
    id: finalId,
    url,
    filename,
    isImage,
    size: file.size
  };
}
