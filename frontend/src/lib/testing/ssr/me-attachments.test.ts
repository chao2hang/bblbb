import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import MeAttachmentsPage from '../../../routes/me/attachments/+page.svelte';
import type { Attachment, AttachmentQuota } from '$lib/api/client';

const quota: AttachmentQuota = {
  max_file_bytes: 5 * 1024 * 1024,
  total_bytes: 250 * 1024 * 1024,
  used_bytes: 100 * 1024 * 1024,
  remaining_bytes: 150 * 1024 * 1024,
  reserved_bytes: 0,
  charged_bytes: 100 * 1024 * 1024,
  daily_upload_bytes: 50 * 1024 * 1024,
  daily_used_bytes: 1024,
  retention_days: 30
};

const imageAttachment: Attachment = {
  id: 'att-img-1',
  owner_id: 'u-1',
  original_name: 'screenshot.png',
  media_type: 'image/png',
  size_bytes: 204800,
  status: 'ready',
  is_public: false,
  ref_count: 2,
  created_at: 1700000000000
};

const fileAttachment: Attachment = {
  id: 'att-file-1',
  owner_id: 'u-1',
  original_name: 'report.zip',
  media_type: 'application/zip',
  size_bytes: 4096,
  status: 'quarantined',
  is_public: false,
  ref_count: 0,
  created_at: 1700000001000
};

describe('GAP-FIX: /me/attachments 我的附件页', () => {
  it('容量摘要与附件列表正常渲染（含图片缩略图与删除表单）', () => {
    const { body } = render(MeAttachmentsPage, {
      props: {
        data: {
          items: [imageAttachment, fileAttachment],
          quota,
          error: null
        }
      }
    });
    // 容量摘要（QuotaDisplay）
    expect(body).toContain('存储额度');
    expect(body).toContain('250 MB');
    // 列表：文件名 / 状态徽章 / 引用数 / 删除表单
    expect(body).toContain('screenshot.png');
    expect(body).toContain('report.zip');
    expect(body).toContain('已就绪');
    expect(body).toContain('已隔离');
    expect(body).toContain('action="?/remove"');
    expect(body).toContain('name="id" value="att-img-1"');
    // 内容端点引用（预览与下载链接同源）
    expect(body).toContain('/api/v1/attachments/att-img-1/content');
  });

  it('空列表与容量缺失时渲染空态与占位（不抛 TypeError）', () => {
    expect(() => {
      const { body } = render(MeAttachmentsPage, {
        props: {
          data: { items: [], quota: null, error: null }
        }
      });
      expect(body).toContain('还没有附件');
    }).not.toThrow();
  });

  it('加载失败时展示错误提示（列表与容量降级）', () => {
    const { body } = render(MeAttachmentsPage, {
      props: {
        data: { items: [], quota: null, error: '后端不可用' }
      }
    });
    expect(body).toContain('附件信息加载失败');
    expect(body).toContain('容量信息暂不可用');
  });
});
