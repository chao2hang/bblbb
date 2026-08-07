// M08-FEEDS-05 / M04-MARKDOWN-08：JsonLd 白名单组件（静态 JSON-LD 注入）。
//
// 安全属性：`data` 中的 `</script` 必须被转义为 `<\/script`，任何字符串字段
// 都不能提前闭合 script 标签形成注入。
import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/svelte';
import JsonLd from './JsonLd.svelte';

describe('JsonLd（静态 JSON-LD 注入组件）', () => {
  it('渲染 application/ld+json script 标签', () => {
    const { container } = render(JsonLd, {
      data: JSON.stringify({ '@context': 'https://schema.org', '@type': 'WebSite', name: 'BBLBB' })
    });
    const script = container.querySelector('script[type="application/ld+json"]');
    expect(script).toBeTruthy();
    expect(script!.textContent).toContain('"@type":"WebSite"');
    expect(script!.textContent).toContain('"name":"BBLBB"');
  });

  it('转义 `</script` 防止闭合注入', () => {
    // 恶意字符串字段：若未转义，会提前闭合 script。
    const malicious = JSON.stringify({ name: '</script><script>alert(1)</script>' });
    const { container } = render(JsonLd, { data: malicious });
    const script = container.querySelector('script[type="application/ld+json"]');
    expect(script).toBeTruthy();
    // 输出不得包含未转义的 `</script>`。
    expect(script!.textContent).not.toContain('</script>');
    expect(script!.textContent).toContain('<\\/script');
    // 仍是合法 JSON（\/ 转义语义不变）。
    expect(() => JSON.parse(script!.textContent!)).not.toThrow();
    // 语义不变：解析后仍是原恶意字符串字段（未丢失内容）。
    const parsed = JSON.parse(script!.textContent!) as { name: string };
    expect(parsed.name).toContain('</script>');
  });

  it('渲染后可被 JSON.parse（完整 JSON-LD）', () => {
    const data = JSON.stringify({ '@context': 'https://schema.org', '@type': 'WebSite', name: 'BBLBB' });
    const { container } = render(JsonLd, { data });
    const text = container.querySelector('script')!.textContent!;
    const parsed = JSON.parse(text);
    expect(parsed['@type']).toBe('WebSite');
  });
});
