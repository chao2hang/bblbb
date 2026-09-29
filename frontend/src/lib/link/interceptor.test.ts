import { describe, expect, it, vi } from 'vitest';
import {
  isExternalUrl,
  buildJumpUrl,
  validateJumpTarget,
  setupExternalLinkInterceptor
} from './interceptor';

describe('isExternalUrl (外链跳转安全判定)', () => {
  const origin = 'http://localhost:5173';

  it('空值或纯空白应判定为非外链', () => {
    expect(isExternalUrl('', origin)).toBe(false);
    expect(isExternalUrl('   ', origin)).toBe(false);
    expect(isExternalUrl(null, origin)).toBe(false);
    expect(isExternalUrl(undefined, origin)).toBe(false);
  });

  it('站内相对路径判定为非外链', () => {
    expect(isExternalUrl('/', origin)).toBe(false);
    expect(isExternalUrl('/boards', origin)).toBe(false);
    expect(isExternalUrl('/posts/p1?sort=hot', origin)).toBe(false);
    expect(isExternalUrl('./relative', origin)).toBe(false);
    expect(isExternalUrl('../parent', origin)).toBe(false);
  });

  it('特殊协议与锚点判定为非外链', () => {
    expect(isExternalUrl('#main-content', origin)).toBe(false);
    expect(isExternalUrl('javascript:alert(1)', origin)).toBe(false);
    expect(isExternalUrl('mailto:test@example.com', origin)).toBe(false);
    expect(isExternalUrl('tel:10086', origin)).toBe(false);
    expect(isExternalUrl('sms:10086', origin)).toBe(false);
  });

  it('同源绝对 URL 判定为非外链', () => {
    expect(isExternalUrl('http://localhost:5173/', origin)).toBe(false);
    expect(isExternalUrl('http://localhost:5173/boards/tech', origin)).toBe(false);
    expect(isExternalUrl('http://localhost:5173/link?target=https%3A%2F%2Fexample.com', origin)).toBe(false);
  });

  it('跨源 HTTP / HTTPS URL 判定为外链', () => {
    expect(isExternalUrl('https://www.arsrna.cn', origin)).toBe(true);
    expect(isExternalUrl('https://github.com/tencent', origin)).toBe(true);
    expect(isExternalUrl('http://example.com/test?foo=bar', origin)).toBe(true);
  });

  it('协议相对 URL 判定为外链', () => {
    expect(isExternalUrl('//evil.example.com/steal', origin)).toBe(true);
    expect(isExternalUrl('/\\\\evil.example.com/steal', origin)).toBe(true);
  });
});

describe('buildJumpUrl (跳转确认链接构造)', () => {
  it('正确编码目标地址到 /link?target=', () => {
    expect(buildJumpUrl('https://www.arsrna.cn')).toBe(
      '/link?target=https%3A%2F%2Fwww.arsrna.cn'
    );
    expect(buildJumpUrl('https://example.com/path?a=1&b=2')).toBe(
      '/link?target=https%3A%2F%2Fexample.com%2Fpath%3Fa%3D1%26b%3D2'
    );
  });
});

describe('validateJumpTarget (跳转目标安全性校验)', () => {
  it('合法的 HTTP/HTTPS 目标返回 valid=true', () => {
    const res = validateJumpTarget('https://www.arsrna.cn');
    expect(res.valid).toBe(true);
    expect(res.url).toBe('https://www.arsrna.cn/');
  });

  it('协议相对目标解析为 HTTPS 外链并可继续访问', () => {
    expect(validateJumpTarget('//example.com/path')).toEqual({
      valid: true,
      url: 'https://example.com/path'
    });
  });

  it('缺少目标参数返回 missing', () => {
    expect(validateJumpTarget('')).toEqual({ valid: false, url: '', reason: 'missing' });
    expect(validateJumpTarget(null)).toEqual({ valid: false, url: '', reason: 'missing' });
  });

  it('非 HTTP/HTTPS 协议判定为 unsafe_scheme', () => {
    expect(validateJumpTarget('javascript:alert(1)')).toEqual({
      valid: false,
      url: 'javascript:alert(1)',
      reason: 'unsafe_scheme'
    });
    expect(validateJumpTarget('data:text/html,<h1>bad</h1>')).toEqual({
      valid: false,
      url: 'data:text/html,<h1>bad</h1>',
      reason: 'unsafe_scheme'
    });
  });

  it('非法格式返回 malformed', () => {
    expect(validateJumpTarget('not a valid url')).toEqual({
      valid: false,
      url: 'not a valid url',
      reason: 'malformed'
    });
  });
});

describe('setupExternalLinkInterceptor (全局外链点击拦截)', () => {
  it('拦截外链点击并重定向到 /link?target=', () => {
    const fakeWindow = {
      location: { origin: 'http://localhost:5173', href: '' },
      open: vi.fn()
    } as unknown as Window;

    const fakeDoc = document.implementation.createHTMLDocument();
    const cleanup = setupExternalLinkInterceptor(fakeDoc, fakeWindow);

    // 1. 普通外部新标签页链接
    const extBlankLink = fakeDoc.createElement('a');
    extBlankLink.href = 'https://www.arsrna.cn';
    extBlankLink.setAttribute('target', '_blank');
    fakeDoc.body.appendChild(extBlankLink);

    const clickEvent1 = new MouseEvent('click', { bubbles: true, cancelable: true });
    extBlankLink.dispatchEvent(clickEvent1);

    expect(clickEvent1.defaultPrevented).toBe(true);
    expect(fakeWindow.open).toHaveBeenCalledWith(
      '/link?target=' + encodeURIComponent('https://www.arsrna.cn'),
      '_blank',
      'noopener,noreferrer'
    );

    // 2. 同源站内链接不应被拦截
    const internalLink = fakeDoc.createElement('a');
    internalLink.href = '/boards';
    fakeDoc.body.appendChild(internalLink);

    const clickEvent2 = new MouseEvent('click', { bubbles: true, cancelable: true });
    internalLink.dispatchEvent(clickEvent2);
    expect(clickEvent2.defaultPrevented).toBe(false);

    // 3. 带有 data-direct-jump="true" 的放行链接不应被拦截
    const directLink = fakeDoc.createElement('a');
    directLink.href = 'https://www.arsrna.cn';
    directLink.setAttribute('data-direct-jump', 'true');
    fakeDoc.body.appendChild(directLink);

    const clickEvent3 = new MouseEvent('click', { bubbles: true, cancelable: true });
    directLink.dispatchEvent(clickEvent3);
    expect(clickEvent3.defaultPrevented).toBe(false);

    // 4. 标准 auxclick 中键事件也必须经过确认页
    const middleLink = fakeDoc.createElement('a');
    middleLink.href = 'https://middle.example/path';
    fakeDoc.body.appendChild(middleLink);
    const auxEvent = new MouseEvent('auxclick', { bubbles: true, cancelable: true, button: 1 });
    middleLink.dispatchEvent(auxEvent);
    expect(auxEvent.defaultPrevented).toBe(true);
    expect(fakeWindow.open).toHaveBeenCalledWith(
      '/link?target=' + encodeURIComponent('https://middle.example/path'),
      '_blank',
      'noopener,noreferrer'
    );

    // 5. 清理拦截器后不再拦截
    cleanup();
    const clickEvent4 = new MouseEvent('click', { bubbles: true, cancelable: true });
    extBlankLink.dispatchEvent(clickEvent4);
    expect(clickEvent4.defaultPrevented).toBe(false);
  });
});
