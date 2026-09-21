// M07-SHOP-STUDIO：发布面板的草稿模型与预检。
// 字段与 OpenAPI AdminStudioPublishRequest.product 一一对应；
// 边界镜像后端 studio.rs parse_product（title 1..=120、unit_price ≥ 0、
// refund_policy/status 枚举、售卖开始 ≤ 结束）。

import { DAY_SECONDS, choiceToSeconds, type ValidityChoice } from './duration';
import { validSlug } from './slugify';

export type RefundPolicy = 'non_refundable' | 'compensation_only' | 'full_refund';
export type PublishStatus = 'draft' | 'published';

export const REFUND_POLICY_OPTIONS: { value: RefundPolicy; label: string }[] = [
  { value: 'non_refundable', label: '不可退款' },
  { value: 'compensation_only', label: '仅补偿' },
  { value: 'full_refund', label: '可退款' }
];

export const REFUND_POLICY_LABELS: Record<string, string> = {
  non_refundable: '不可退款',
  compensation_only: '仅补偿',
  full_refund: '可退款'
};

export interface PublishDraft {
  title: string;
  /** 空串 = 服务端按标题派生。 */
  slug: string;
  /** slug 是否被手动改过（未改过则随标题自动派生）。 */
  slugTouched: boolean;
  unitPrice: number;
  /** null = 不限库存。 */
  stockRemaining: number | null;
  requiredLevel: number;
  quantityLimit: number;
  validityChoice: ValidityChoice;
  customDays: number;
  /** datetime-local 值（'' = 未设置）。 */
  saleStartAt: string;
  saleEndAt: string;
  refundPolicy: RefundPolicy;
  status: PublishStatus;
  descriptionSafe: string;
  /** PNG/APNG 头像框模式的附件 id（'' = 无）。 */
  assetAttachmentId: string;
  /** 货币 code 或 id（默认 coin）。 */
  currencyId: string;
  /** 操作原因（写审计）。 */
  reason: string;
}

export function createPublishDraft(): PublishDraft {
  return {
    title: '',
    slug: '',
    slugTouched: false,
    unitPrice: 0,
    stockRemaining: null,
    requiredLevel: 1,
    quantityLimit: 1,
    validityChoice: 'permanent',
    customDays: 30,
    saleStartAt: '',
    saleEndAt: '',
    refundPolicy: 'non_refundable',
    status: 'published',
    descriptionSafe: '',
    assetAttachmentId: '',
    currencyId: 'coin',
    reason: ''
  };
}

/** 从原生表单数据还原发布草稿（无 JS 提交路径；缺省字段回退默认值）。 */
export function publishDraftFromForm(form: FormData): PublishDraft {
  const draft = createPublishDraft();
  draft.title = String(form.get('title') ?? '').trim();
  draft.slug = String(form.get('slug') ?? '').trim();
  draft.slugTouched = draft.slug !== '';
  const num = (key: string, fallback: number): number => {
    const raw = String(form.get(key) ?? '');
    const n = Number(raw);
    return raw !== '' && Number.isFinite(n) ? n : fallback;
  };
  draft.unitPrice = num('unit_price', 0);
  const stock = String(form.get('stock_remaining') ?? '').trim();
  draft.stockRemaining = stock === '' ? null : Math.max(0, Math.round(Number(stock)));
  draft.requiredLevel = Math.max(1, Math.round(num('required_level', 1)));
  draft.quantityLimit = Math.max(1, Math.round(num('quantity_limit', 1)));
  const choice = String(form.get('validity_choice') ?? '');
  if (choice === '7d' || choice === '30d' || choice === '90d' || choice === 'custom') draft.validityChoice = choice;
  draft.customDays = Math.max(1, Math.round(num('custom_days', 30)));
  draft.saleStartAt = String(form.get('sale_start_at') ?? '');
  draft.saleEndAt = String(form.get('sale_end_at') ?? '');
  const refund = String(form.get('refund_policy') ?? '');
  if (refund === 'compensation_only' || refund === 'full_refund') draft.refundPolicy = refund;
  draft.status = String(form.get('status') ?? '') === 'draft' ? 'draft' : 'published';
  draft.descriptionSafe = String(form.get('description_safe') ?? '').trim();
  draft.assetAttachmentId = String(form.get('asset_attachment_id') ?? '').trim();
  draft.reason = String(form.get('reason') ?? '').trim();
  return draft;
}

/** 草稿 → 时效秒（null = 永久）。 */
export function publishValiditySeconds(draft: PublishDraft): number | null {
  return choiceToSeconds(draft.validityChoice, draft.customDays);
}

export type PublishDraftErrors = Partial<
  Record<
    | 'title'
    | 'slug'
    | 'unit_price'
    | 'stock_remaining'
    | 'required_level'
    | 'quantity_limit'
    | 'validity_seconds'
    | 'sale_window'
    | 'refund_policy'
    | 'description_safe'
    | 'asset_attachment_id'
    | 'reason',
    string
  >
>;

/** 前端预检（与后端 parse_product 同边界）：返回字段级错误，空对象 = 通过。 */
export function validatePublishDraft(draft: PublishDraft): PublishDraftErrors {
  const errors: PublishDraftErrors = {};
  const title = draft.title.trim();
  if (!title || [...title].length > 120) errors.title = '商品名称需为 1–120 个字符';
  if (draft.slug && !validSlug(draft.slug)) errors.slug = 'slug 只能包含小写字母、数字与单个连字符';
  if (!Number.isFinite(draft.unitPrice) || draft.unitPrice < 0) errors.unit_price = '价格不能为负数';
  if (draft.stockRemaining !== null && (!Number.isFinite(draft.stockRemaining) || draft.stockRemaining < 0)) {
    errors.stock_remaining = '库存不能为负数';
  }
  if (!Number.isInteger(draft.requiredLevel) || draft.requiredLevel < 1) errors.required_level = '等级门槛需 ≥ 1';
  if (!Number.isInteger(draft.quantityLimit) || draft.quantityLimit < 1) errors.quantity_limit = '限购需 ≥ 1';
  if (draft.validityChoice === 'custom' && (!Number.isFinite(draft.customDays) || draft.customDays < 1)) {
    errors.validity_seconds = '自定义时效至少 1 天';
  }
  if (draft.saleStartAt && draft.saleEndAt && draft.saleStartAt > draft.saleEndAt) {
    errors.sale_window = '售卖开始时间不能晚于结束时间';
  }
  if ([...draft.descriptionSafe].length > 2000) errors.description_safe = '说明最长 2000 字';
  if (!draft.reason.trim()) errors.reason = '操作原因必填（写审计）';
  return errors;
}

/** 草稿 → 提交给复合端点的 product 对象（日期转毫秒时间戳）。 */
export function publishDraftToProduct(draft: PublishDraft): Record<string, unknown> {
  const toMs = (v: string): number | null => {
    if (!v) return null;
    const ms = Date.parse(v);
    return Number.isFinite(ms) ? ms : null;
  };
  return {
    title: draft.title.trim(),
    slug: draft.slug || undefined,
    unit_price: Math.max(0, Math.round(draft.unitPrice)),
    stock_remaining: draft.stockRemaining,
    required_level: draft.requiredLevel,
    quantity_limit: draft.quantityLimit,
    validity_seconds: publishValiditySeconds(draft),
    sale_start_at: toMs(draft.saleStartAt),
    sale_end_at: toMs(draft.saleEndAt),
    refund_policy: draft.refundPolicy,
    status: draft.status,
    description_safe: draft.descriptionSafe.trim() || undefined,
    asset_attachment_id: draft.assetAttachmentId || null,
    currency_id: draft.currencyId || 'coin'
  };
}

export { DAY_SECONDS };
