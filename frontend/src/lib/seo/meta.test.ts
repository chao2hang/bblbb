// M14-SEO-01：统一 SEO 安全生成器测试。
//
// 覆盖：title/description 截断、canonical 绝对 URL 校验（javascript:/data:/
// 相对路径拒绝）、noindex 策略、OG/Twitter 安全、JSON-LD `</script` 转义、
// 隐藏内容统一 noindex（M14-SEO-03）。
import { describe, expect, it } from 'vitest';
import { buildSeo, hiddenSeo, safeHttpUrl } from './meta';

describe('buildSeo 安全生成器', () => {
  it('输出带站点后缀的 title 与 description', () => {
    const meta = buildSeo({ title: 'Rust 入门', description: '从所有权开始理解 Rust。' });
    expect(meta.title).toBe('Rust 入门 — BBLBB');
    expect(meta.description).toBe('从所有权开始理解 Rust。');
    expect(meta.robots).toBeNull(); // 默认可索引
  });

  it('长 title/description 被截断（title≤60+站点后缀，description≤160）', () => {
    const meta = buildSeo({ title: 'x'.repeat(200), description: 'y'.repeat(400) });
    expect(meta.title.length).toBeLessThanOrEqual(60 + ' — BBLBB'.length);
    expect(meta.description!.length).toBeLessThanOrEqual(160);
  });

  it('canonical 只接受绝对 http(s) URL', () => {
    expect(buildSeo({ title: 't', canonical: 'https://bblbb.example/posts/p1' }).canonical).toBe(
      'https://bblbb.example/posts/p1'
    );
    expect(safeHttpUrl('javascript:alert(1)')).toBeNull();
    expect(safeHttpUrl('data:text/html,<script>1</script>')).toBeNull();
    expect(safeHttpUrl('/relative/path')).toBeNull();
    expect(safeHttpUrl('//protocol-relative.example/x')).toBeNull();
    expect(safeHttpUrl('ftp://example.com/x')).toBeNull();
    expect(buildSeo({ title: 't', canonical: 'javascript:alert(1)' }).canonical).toBeNull();
  });

  it('noindex 输出统一 robots 指令（M14-SEO-03）', () => {
    const meta = buildSeo(hiddenSeo('草稿内容'));
    expect(meta.robots).toBe('noindex, noarchive, nofollow');
  });

  it('OG/Twitter 图片只接受 http(s)，非法图片整体丢弃', () => {
    const meta = buildSeo({
      title: 't',
      og: { image: 'javascript:alert(1)', type: 'article', siteName: 'BBLBB' },
      twitter: { card: 'summary_large_image', image: 'data:x' }
    });
    expect(meta.ogType).toBe('article');
    expect(meta.ogSiteName).toBe('BBLBB');
    expect(meta.ogImage).toBeNull();
    expect(meta.twitterImage).toBeNull();
    // 合法图片 URL 保留。
    const ok = buildSeo({ title: 't', og: { image: 'https://cdn.example/a.jpg' }, twitter: { image: 'https://cdn.example/a.jpg' } });
    expect(ok.ogImage).toBe('https://cdn.example/a.jpg');
    expect(ok.twitterImage).toBe('https://cdn.example/a.jpg');
    expect(ok.twitterCard).toBe('summary_large_image');
  });

  it('JSON-LD 输出经 `</script` 转义且可 JSON.parse', () => {
    const meta = buildSeo({
      title: 't',
      jsonLd: { '@type': 'Article', headline: '</script><script>alert(1)</script>' }
    });
    expect(meta.jsonLd).not.toContain('</script>');
    expect(meta.jsonLd).toContain('<\\/script');
    const parsed = JSON.parse(meta.jsonLd!) as { headline: string };
    expect(parsed.headline).toContain('</script>');
  });

  it('循环引用 JSON-LD 安全丢弃而非抛错', () => {
    const circular: Record<string, unknown> = { a: 1 };
    circular.self = circular;
    const meta = buildSeo({ title: 't', jsonLd: circular });
    expect(meta.jsonLd).toBeNull();
  });

  it('控制字符被清理，多行折叠为单行', () => {
    const meta = buildSeo({ title: '标题\u0000注入', description: '第一行\n\n  第二行' });
    expect(meta.title).not.toContain('\u0000');
    expect(meta.description).toBe('第一行 第二行');
  });
});
