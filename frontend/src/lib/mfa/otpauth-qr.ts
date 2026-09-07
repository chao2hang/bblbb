// M18-MFA-01 增强：TOTP 注册二维码（服务端生成，页面以 <img data-URL> 渲染）。
//
// 设计要点：
// - 输入为后端 enroll 端点返回的 otpauth_uri（服务端自生成的可信输入，非用户输入）；
// - 渲染走 qrcode 的 lib/browser toString（svg-tag 渲染器，纯字符串 SVG，无
//   canvas 依赖，Node/浏览器一致），结果 base64 编码为 data:image/svg+xml URL——
//   不使用 {@html}（M04-MARKDOWN-08 HTML sink 政策，见 scripts/check-html-sinks.rb），
//   且无 JS 基线（SSR）也能直接渲染二维码、用手机扫码；
// - 生成失败返回 null，页面降级为手工录入密钥（secret/otpauth 文本恒定保留）。
import QRCode from 'qrcode/lib/browser';

/** 由 otpauth:// URI 生成可扫码二维码（SVG data URL）；失败返回 null。 */
export async function otpauthQrDataUrl(otpauthUri: string): Promise<string | null> {
  if (!otpauthUri.startsWith('otpauth://')) return null;
  try {
    const svg = await QRCode.toString(otpauthUri, {
      type: 'svg',
      width: 240,
      margin: 2,
      errorCorrectionLevel: 'M',
      color: { dark: '#000000', light: '#ffffff' }
    });
    return `data:image/svg+xml;base64,${Buffer.from(svg, 'utf8').toString('base64')}`;
  } catch {
    return null;
  }
}
