// M18-MFA-01：otpauthQrDataUrl 单测（纯字符串 SVG 渲染，无需 canvas/DOM）。
import { describe, expect, it } from 'vitest';
import { otpauthQrDataUrl } from './otpauth-qr';

const URI = 'otpauth://totp/BBLBB:alice@example.com?secret=JBSWY3DPEHPK3PXP&issuer=BBLBB';

describe('otpauthQrDataUrl（TOTP 注册二维码）', () => {
  it('生成 SVG data URL（base64 解码后为合法 <svg>）', async () => {
    const url = await otpauthQrDataUrl(URI);
    expect(url).toMatch(/^data:image\/svg\+xml;base64,/);
    const b64 = url!.slice('data:image/svg+xml;base64,'.length);
    const svg = Buffer.from(b64, 'base64').toString('utf8');
    expect(svg).toContain('<svg');
    expect(svg).toContain('viewBox');
    expect(svg).toContain('</svg>');
  });

  it('同一 URI 输出确定性（便于测试断言与缓存）', async () => {
    const [a, b] = await Promise.all([otpauthQrDataUrl(URI), otpauthQrDataUrl(URI)]);
    expect(a).toBe(b);
  });

  it('含中文 issuer 的 URI 也可生成（percent-encode 后仍可读）', async () => {
    const url = await otpauthQrDataUrl(
      'otpauth://totp/BBLBB:alice@example.com?secret=JBSWY3DPEHPK3PXP&issuer=%E5%B0%8F%E7%BA%A2%E4%B9%A6'
    );
    expect(url).toMatch(/^data:image\/svg\+xml;base64,/);
  });

  it('非 otpauth:// 或空 URI → null（页面走手工录入降级）', async () => {
    expect(await otpauthQrDataUrl('https://example.com/x')).toBeNull();
    expect(await otpauthQrDataUrl('')).toBeNull();
  });
});
