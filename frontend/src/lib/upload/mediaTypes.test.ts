import { afterEach, describe, expect, it } from 'vitest';
import {
  ALL_UPLOAD_MEDIA_TYPES,
  categoryForMediaType,
  extensionMediaType,
  isAllowedMediaType,
  resolveUploadMediaType,
  resetUploadPolicyCache,
  uploadTypeHint
} from './mediaTypes';

describe('mediaTypes', () => {
  afterEach(() => resetUploadPolicyCache());

  it('extensionMediaType 按扩展名归一化（含大小写与多扩展名）', () => {
    expect(extensionMediaType('photo.JPG')).toBe('image/jpeg');
    expect(extensionMediaType('notes.md')).toBe('text/plain');
    expect(extensionMediaType('report.docx')).toBe(
      'application/vnd.openxmlformats-officedocument.wordprocessingml.document'
    );
    expect(extensionMediaType('clip.webm')).toBe('video/webm');
    expect(extensionMediaType('noext')).toBeNull();
    expect(extensionMediaType('.hidden')).toBeNull();
    expect(extensionMediaType('virus.exe')).toBeNull();
  });

  it('categoryForMediaType 覆盖全部能力白名单成员', () => {
    for (const mt of ALL_UPLOAD_MEDIA_TYPES) {
      expect(categoryForMediaType(mt), mt).not.toBeNull();
    }
    expect(categoryForMediaType('application/zip')).toBeNull();
    expect(categoryForMediaType('text/html')).toBeNull();
  });

  it('isAllowedMediaType：缺省按全量白名单，传入策略按子集', () => {
    expect(isAllowedMediaType('application/pdf')).toBe(true);
    expect(isAllowedMediaType('application/zip')).toBe(false);
    expect(isAllowedMediaType('image/png', ['image/png', 'video/mp4'])).toBe(true);
    expect(isAllowedMediaType('application/pdf', ['image/png', 'video/mp4'])).toBe(false);
  });

  it('resolveUploadMediaType：扩展名优先，策略外返回 null', () => {
    // 浏览器对 md/csv 常报空 type，扩展名归一化兜底
    expect(
      resolveUploadMediaType(new File(['a'], 'x.csv', { type: '' }))
    ).toBe('text/csv');
    // 浏览器误报 octet-stream 但扩展名明确 → 归一化
    expect(
      resolveUploadMediaType(new File(['a'], 'x.mp3', { type: 'application/octet-stream' }))
    ).toBe('audio/mpeg');
    // 策略关闭 image → 图片文件被拒
    expect(
      resolveUploadMediaType(new File(['a'], 'x.png', { type: 'image/png' }), ['application/pdf'])
    ).toBeNull();
    // 白名单外类型恒拒绝
    expect(resolveUploadMediaType(new File(['a'], 'x.zip', { type: 'application/zip' }))).toBeNull();
  });

  it('uploadTypeHint 按策略生成类目文案', () => {
    expect(uploadTypeHint(null)).toContain('图片');
    expect(uploadTypeHint(['application/pdf'])).toBe('PDF 文档');
    expect(uploadTypeHint([])).toContain('未开放');
  });
});
