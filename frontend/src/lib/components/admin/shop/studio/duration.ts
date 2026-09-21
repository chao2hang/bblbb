// M07-SHOP-STUDIO：商品时效（validity_seconds）友好选择与展示工具。

export const DAY_SECONDS = 86_400;

export type ValidityChoice = 'permanent' | '7d' | '30d' | '90d' | 'custom';

export const VALIDITY_OPTIONS: { value: Exclude<ValidityChoice, 'custom'>; label: string; seconds: number | null }[] = [
  { value: 'permanent', label: '永久', seconds: null },
  { value: '7d', label: '7 天', seconds: 7 * DAY_SECONDS },
  { value: '30d', label: '30 天', seconds: 30 * DAY_SECONDS },
  { value: '90d', label: '90 天', seconds: 90 * DAY_SECONDS }
];

/** 秒 → 友好选项（非预设值归入 custom）。 */
export function validityToChoice(seconds: number | null | undefined): ValidityChoice {
  if (seconds === null || seconds === undefined) return 'permanent';
  for (const opt of VALIDITY_OPTIONS) {
    if (opt.seconds === seconds) return opt.value;
  }
  return 'custom';
}

/** 友好选项 → 秒；custom 由调用方自行读取自定义天数。 */
export function choiceToSeconds(choice: ValidityChoice, customDays = 30): number | null {
  if (choice === 'custom') return Math.max(1, Math.round(customDays)) * DAY_SECONDS;
  return VALIDITY_OPTIONS.find((o) => o.value === choice)?.seconds ?? null;
}

/** 秒 → 中文展示（永久 / N 天 / N 小时 / N 秒）。 */
export function formatValidity(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined) return '永久';
  if (!Number.isFinite(seconds) || seconds <= 0) return '永久';
  if (seconds % DAY_SECONDS === 0) return `${seconds / DAY_SECONDS} 天`;
  if (seconds % 3600 === 0) return `${seconds / 3600} 小时`;
  return `${seconds} 秒`;
}
