/** Flag 显示名与说明（capability 中文名；注册表以后端为准）。 */
export const FLAG_LABELS: Record<string, { label: string; hint: string }> = {
  ai: { label: '大模型 Gateway', hint: '格式化、内容审计建议、SEO 辅助与逐次同意' },
  video: { label: '视频嵌入', hint: 'Direct/HLS/西瓜视频安全解析与渲染' },
  download_billing: { label: '下载计费', hint: '附件下载抵扣积分与授权' },
  oidc: { label: 'OIDC Provider', hint: '对外统一登录（需专项门槛）' },
  marketplace: { label: '第三方 Marketplace', hint: '外部应用市场接入与账务' }
};

export function flagLabel(name: string): string {
  return FLAG_LABELS[name]?.label ?? name;
}
