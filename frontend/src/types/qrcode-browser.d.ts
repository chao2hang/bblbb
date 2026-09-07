// `qrcode` 包（npm）最小类型声明——该包不附带官方 TypeScript 类型。
//
// 本项目仅使用 `qrcode/lib/browser` 的 `toString`：基于 svg-tag 渲染器的
// 纯字符串 SVG 输出（无 canvas 依赖，Node/浏览器均可运行），用于 MFA 注册时
// 在服务端（+page.server.ts，Vite SSR 外部化依赖）生成可扫码的二维码。
declare module 'qrcode/lib/browser' {
  export interface QRCodeColor {
    dark: string;
    light: string;
  }
  export interface QRCodeToStringOptions {
    /** 输出格式（本项目仅用 'svg'）。 */
    type?: 'svg' | 'utf8' | 'terminal';
    /** 纠错等级；'M'（≈15% 冗余）适合身份验证器扫码场景。 */
    errorCorrectionLevel?: 'L' | 'M' | 'Q' | 'H';
    /** SVG width/height 属性（viewBox 按模块矩阵自适应，图像可无损缩放）。 */
    width?: number;
    /** 静区宽度（模块数）。 */
    margin?: number;
    /** 模块/背景颜色。 */
    color?: QRCodeColor;
    /** 固定 QR 版本号（默认按内容自动选择）。 */
    version?: number;
  }
  const QRCodeBrowser: {
    toString(text: string, cb: (err: Error | null, svg: string) => void): void;
    toString(text: string, options?: QRCodeToStringOptions): Promise<string>;
    create(text: string, options?: QRCodeToStringOptions): unknown;
  };
  export default QRCodeBrowser;
}
