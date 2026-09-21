// 发布面板草稿预检与序列化的单元测试（边界镜像后端 studio.rs parse_product）。
import { describe, expect, it } from 'vitest';
import { DAY_SECONDS } from './duration';
import {
  createPublishDraft,
  publishDraftToProduct,
  publishValiditySeconds,
  validatePublishDraft
} from './publish-draft';

describe('validatePublishDraft（发布字段预检）', () => {
  it('默认草稿 + 名称 + 原因通过', () => {
    const draft = createPublishDraft();
    draft.title = '樱花粉渐变昵称';
    draft.reason = '首次发布';
    expect(validatePublishDraft(draft)).toEqual({});
  });

  it('名称必填且 ≤120 字', () => {
    const draft = createPublishDraft();
    draft.reason = 'r';
    expect(validatePublishDraft(draft).title).toBeTruthy();
    draft.title = 'x'.repeat(121);
    expect(validatePublishDraft(draft).title).toBeTruthy();
  });

  it('价格/库存不能为负', () => {
    const draft = createPublishDraft();
    draft.title = 't';
    draft.reason = 'r';
    draft.unitPrice = -1;
    expect(validatePublishDraft(draft).unit_price).toBeTruthy();
    draft.unitPrice = 0;
    draft.stockRemaining = -5;
    expect(validatePublishDraft(draft).stock_remaining).toBeTruthy();
  });

  it('售卖窗口开始不能晚于结束', () => {
    const draft = createPublishDraft();
    draft.title = 't';
    draft.reason = 'r';
    draft.saleStartAt = '2026-10-02T00:00';
    draft.saleEndAt = '2026-10-01T00:00';
    expect(validatePublishDraft(draft).sale_window).toBeTruthy();
    draft.saleEndAt = '2026-10-03T00:00';
    expect(validatePublishDraft(draft).sale_window).toBeUndefined();
  });

  it('操作原因必填', () => {
    const draft = createPublishDraft();
    draft.title = 't';
    expect(validatePublishDraft(draft).reason).toBeTruthy();
  });

  it('非法显式 slug 被拒', () => {
    const draft = createPublishDraft();
    draft.title = 't';
    draft.reason = 'r';
    draft.slug = 'Bad Slug';
    expect(validatePublishDraft(draft).slug).toBeTruthy();
    draft.slug = 'good-slug';
    expect(validatePublishDraft(draft).slug).toBeUndefined();
  });
});

describe('publishValiditySeconds / publishDraftToProduct', () => {
  it('永久 → null；30 天 → 秒', () => {
    const draft = createPublishDraft();
    expect(publishValiditySeconds(draft)).toBeNull();
    draft.validityChoice = '30d';
    expect(publishValiditySeconds(draft)).toBe(30 * DAY_SECONDS);
    draft.validityChoice = 'custom';
    draft.customDays = 14;
    expect(publishValiditySeconds(draft)).toBe(14 * DAY_SECONDS);
  });

  it('序列化：空 slug/说明/附件省略，日期转毫秒', () => {
    const draft = createPublishDraft();
    draft.title = '  樱花粉渐变昵称  ';
    draft.unitPrice = 99.6;
    draft.saleStartAt = '2026-10-01T00:00';
    const product = publishDraftToProduct(draft);
    expect(product.title).toBe('樱花粉渐变昵称');
    expect(product.unit_price).toBe(100);
    expect(product.slug).toBeUndefined();
    expect(product.description_safe).toBeUndefined();
    expect(product.asset_attachment_id).toBeNull();
    expect(product.currency_id).toBe('coin');
    expect(product.refund_policy).toBe('non_refundable');
    expect(product.status).toBe('published');
    expect(typeof product.sale_start_at).toBe('number');
    expect(product.sale_end_at).toBeNull();
  });
});
