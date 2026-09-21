// slug 工具单元测试：与 backend/src/shop/studio.rs 的 slugify/valid_slug
// 保持一致（同样的标题在两端派生出同样的 slug）。
import { describe, expect, it } from 'vitest';
import { slugify, validSlug } from './slugify';

describe('slugify（标题 → slug 派生）', () => {
  it('英文标题转小写、空格折叠为单连字符', () => {
    expect(slugify('Neon Nick')).toBe('neon-nick');
    expect(slugify('  Multiple   Spaces  ')).toBe('multiple-spaces');
  });

  it('连续非字母数字折叠为单个连字符', () => {
    expect(slugify('a--b__c!!d')).toBe('a-b-c-d');
  });

  it('中文等非 ASCII 被剥离；全中文回退 cosmetic', () => {
    expect(slugify('樱花粉渐变')).toBe('cosmetic');
    expect(slugify('Sakura 樱花 Pink')).toBe('sakura-pink');
  });

  it('截断 48 字符且不带尾部连字符', () => {
    const slug = slugify(`x${'-a'.repeat(40)}`);
    expect(slug.length).toBeLessThanOrEqual(48);
    expect(slug.endsWith('-')).toBe(false);
  });

  it('空输入回退 cosmetic', () => {
    expect(slugify('')).toBe('cosmetic');
    expect(slugify('!!!')).toBe('cosmetic');
  });
});

describe('validSlug（显式 slug 预检）', () => {
  it('合法 slug 通过', () => {
    expect(validSlug('neon-nick')).toBe(true);
    expect(validSlug('a')).toBe(true);
    expect(validSlug('abc-123-def')).toBe(true);
  });

  it('非法 slug 拒绝', () => {
    expect(validSlug('')).toBe(false);
    expect(validSlug('-lead')).toBe(false);
    expect(validSlug('trail-')).toBe(false);
    expect(validSlug('double--dash')).toBe(false);
    expect(validSlug('Upper')).toBe(false);
    expect(validSlug('has space')).toBe(false);
    expect(validSlug('x'.repeat(65))).toBe(false);
  });
});
