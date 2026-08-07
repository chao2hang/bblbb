// M09-UI-02/03：AI 同意披露文案与稳定 hash。
//
// 每次正文外发（full_with_consent）前必须展示完整披露并取得明确确认
// （docs/AI.md §5）。本模块生成披露文案（Provider/用途/留存/训练/区域/数据
// 模式）并计算稳定 hash——前端把「展示给用户的文案」及其版本交给后端记录，
// 后端保存 text hash 作为同意证据。任何文案变更必须 bump AI_DISCLOSURE_VERSION。

/** 当前同意披露文案版本（站点策略；变更文案即 +1）。 */
export const AI_DISCLOSURE_VERSION = 1;

export interface DisclosureBuildInput {
  providerName: string | null | undefined;
  purpose: string;
  dataMode: string | null | undefined;
  retention?: string | null;
  training?: string | null;
  region?: string | null;
}

/** 完整披露文案（展示给用户并参与 hash 计算，作为同意证据）。 */
export function buildDisclosureText(input: DisclosureBuildInput): string {
  const purposeLabel = (p: string): string => {
    const map: Record<string, string> = {
      formatting: '内容格式化',
      seo: 'SEO 优化',
      tagging: '标签建议',
      moderation: '内容审核辅助'
    };
    return map[p] ?? p;
  };
  const lines: string[] = [];
  lines.push(`你的正文将发送给 AI 提供商「${input.providerName ?? '已配置提供商'}」用于「${purposeLabel(input.purpose)}」。`);
  lines.push(`数据模式：${input.dataMode ?? 'full_with_consent'}（本次为完整内容发送）。`);
  if (input.retention) lines.push(`留存：${input.retention}。`);
  if (input.training) lines.push(`训练使用：${input.training}。`);
  if (input.region) lines.push(`数据处理区域：${input.region}。`);
  lines.push('发送内容仅用于本次建议生成，不会用于自动处罚或修改权限；你可以在同意记录中随时撤回。');
  return lines.join('\n');
}

/** FNV-1a 32-bit 稳定 hash（十六进制）。纯文本 → 固定串，供同意证据记录。 */
export function disclosureHash(text: string): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return (h >>> 0).toString(16).padStart(8, '0');
}
