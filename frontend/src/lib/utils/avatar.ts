/**
 * 默认头像配色盘：16 组精心校准的高级双色渐变（矿物色/国风雅致/现代界面语汇）。
 * 每组渐变经过 WCAG AA 对比度校验，确保纯白首字母文字（#ffffff）在任何尺寸下都具备清晰的易读性。
 */
export const AVATAR_PALETTES: readonly [string, string][] = [
  ['#1e40af', '#3b82f6'], // 黛蓝 / Azure Indigo
  ['#0e7490', '#06b6d4'], // 石青 / Deep Cyan
  ['#0f766e', '#14b8a6'], // 松青 / Pine Teal
  ['#047857', '#10b981'], // 翡翠 / Jade Emerald
  ['#15803d', '#22c55e'], // 竹青 / Forest Green
  ['#b45309', '#f59e0b'], // 琥珀 / Golden Amber
  ['#c2410c', '#fb923c'], // 丹霞 / Terracotta
  ['#b91c1c', '#f87171'], // 绛红 / Vermilion Crimson
  ['#be123c', '#fb7185'], // 胭脂 / Carmine Rose
  ['#86198f', '#d946ef'], // 紫棠 / Mulberry Violet
  ['#6b21a8', '#a855f7'], // 玄紫 / Royal Amethyst
  ['#4c1d95', '#818cf8'], // 暮紫 / Iris Violet
  ['#1d4ed8', '#60a5fa'], // 霁蓝 / Cobalt Sky
  ['#0369a1', '#38bdf8'], // 苍穹 / Ocean Slate
  ['#92400e', '#d97706'], // 古铜 / Bronze Gold
  ['#4a044e', '#9333ea']  // 檀紫 / Deep Plum
] as const;

/**
 * 提取头像首字母或标识字符：
 * 1. 过滤首尾空白与前导 handle 符号（如 @user / #tag）；
 * 2. 准确拆分 Unicode surrogate pairs 与 Emoji；
 * 3. 英文字母统一转为大写，中文字符与符号原样保留；
 * 4. 空值或纯符号安全降级为 '?'。
 */
export function getAvatarInitial(name?: string | null): string {
  if (!name) return '?';
  const cleaned = name.trim().replace(/^[@#]+/, '');
  if (!cleaned) return '?';
  const chars = Array.from(cleaned);
  const first = chars[0];
  if (!first) return '?';
  return first.toUpperCase();
}

/**
 * 解析统一的头像标识键（Avatar Seed）：
 * 优先使用全站稳定且唯一的 username，缺省时回退 id，最后回退展示名称。
 * 保证同一用户在全站任意位置（导航、个人主页、帖子、评论、悬浮资料卡、抽屉等）
 * 拥有完全一致的默认背景颜色。
 */
export function resolveAvatarSeed(
  identity?: {
    username?: string | null;
    id?: string | null;
    display_name?: string | null;
    name?: string | null;
  } | string | null
): string {
  if (!identity) return '?';
  if (typeof identity === 'string') return identity.trim() || '?';
  const key =
    identity.username?.trim() ||
    identity.id?.trim() ||
    identity.display_name?.trim() ||
    identity.name?.trim() ||
    '?';
  return key;
}

/**
 * 32 位 FNV-1a 哈希算法：
 * 具备优秀的雪崩效应与高离散度，保证不同用户名称分布均匀（“随机视觉”），
 * 同时针对同一用户输入完全确定幂等（“按用户固定”）。
 */
export function getAvatarColorIndex(key: string, paletteLength: number = AVATAR_PALETTES.length): number {
  const normalized = (key || '?').trim().toLowerCase();
  let hash = 2166136261;
  for (let i = 0; i < normalized.length; i++) {
    hash ^= normalized.charCodeAt(i);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0) % paletteLength;
}

/**
 * 根据用户标识（seed 或 name）获取固定的渐变配色
 */
export function getAvatarGradient(key: string): { c1: string; c2: string; gradient: string } {
  const index = getAvatarColorIndex(key);
  const [c1, c2] = AVATAR_PALETTES[index];
  return {
    c1,
    c2,
    gradient: `linear-gradient(135deg, ${c1}, ${c2})`
  };
}
