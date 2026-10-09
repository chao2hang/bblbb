import { describe, expect, it } from 'vitest';
import { formatLedgerAction, formatLedgerKind } from './format';

describe('formatLedgerAction', () => {
  it('翻译商城购买操作', () => {
    expect(formatLedgerAction('shop purchase 磁带 x1')).toBe('商城购买 磁带 x1');
    expect(formatLedgerAction('shop purchase 梦幻色彩 x1')).toBe('商城购买 梦幻色彩 x1');
    expect(formatLedgerAction('shop purchase Girls Animated Miniback x1')).toBe('商城购买 Girls Animated Miniback x1');
    expect(formatLedgerAction('shop purchase')).toBe('商城购买');
  });

  it('翻译商城退款操作', () => {
    expect(formatLedgerAction('refund 商品质量问题')).toBe('退款：商品质量问题');
    expect(formatLedgerAction('refund: 重复购买')).toBe('退款：重复购买');
    expect(formatLedgerAction('refund')).toBe('退款');
  });

  it('翻译各类活动奖励操作', () => {
    expect(formatLedgerAction('check_in 奖励')).toBe('签到奖励');
    expect(formatLedgerAction('check_in')).toBe('签到奖励');
    expect(formatLedgerAction('checkin')).toBe('签到奖励');
    expect(formatLedgerAction('task 奖励')).toBe('任务奖励');
    expect(formatLedgerAction('reaction 奖励')).toBe('点赞互动奖励');
    expect(formatLedgerAction('post 奖励')).toBe('发帖奖励');
    expect(formatLedgerAction('comment 奖励')).toBe('评论奖励');
    expect(formatLedgerAction('leaderboard 奖励')).toBe('排行榜奖励');
    expect(formatLedgerAction('reward')).toBe('激励奖励');
  });

  it('翻译附件下载与付费帖子操作', () => {
    expect(formatLedgerAction('download attachment att-123')).toBe('下载附件 att-123');
    expect(formatLedgerAction('attachment download')).toBe('下载附件');
    expect(formatLedgerAction('unlock paid post post-456')).toBe('解锁付费帖子 post-456');
  });

  it('翻译应用市场交易操作', () => {
    expect(formatLedgerAction('marketplace purchase mp-1')).toBe('应用市场购买 mp-1');
    expect(formatLedgerAction('marketplace merchant credit mp-1')).toBe('应用市场商户结算 mp-1');
    expect(formatLedgerAction('marketplace platform fee mp-1')).toBe('应用市场平台手续费 mp-1');
    expect(formatLedgerAction('marketplace refund mp-1: 取消')).toBe('应用市场退款 mp-1：取消');
    expect(formatLedgerAction('marketplace merchant refund mp-1')).toBe('应用市场商户退款 mp-1');
    expect(formatLedgerAction('marketplace fee refund mp-1')).toBe('应用市场平台手续费退款 mp-1');
    expect(formatLedgerAction('merchant compensation: 补偿发放')).toBe('商户补偿：补偿发放');
  });

  it('保持已有中文或自定义备注不变', () => {
    expect(formatLedgerAction('管理员人工补发')).toBe('管理员人工补发');
    expect(formatLedgerAction('活动奖励发放')).toBe('活动奖励发放');
    expect(formatLedgerAction('新用户注册礼包')).toBe('新用户注册礼包');
  });

  it('在 memo 为空时根据 kind 及数值兜底', () => {
    expect(formatLedgerAction(null, 'award')).toBe('奖励发放');
    expect(formatLedgerAction('', 'shop_purchase')).toBe('商城购买');
    expect(formatLedgerAction(undefined, 'adjust')).toBe('人工调账');
    expect(formatLedgerAction(null, 'reversal')).toBe('冲正退款');
    expect(formatLedgerAction(null, 'credit')).toBe('系统入账');
    expect(formatLedgerAction(null, 'debit')).toBe('消费/扣减');
    expect(formatLedgerAction(null, undefined, 50)).toBe('系统入账');
    expect(formatLedgerAction(null, undefined, -50)).toBe('消费/扣减');
  });
});

describe('formatLedgerKind', () => {
  it('正确映射所有账本 kind', () => {
    expect(formatLedgerKind('award')).toBe('奖励发放');
    expect(formatLedgerKind('shop_purchase')).toBe('商城购买');
    expect(formatLedgerKind('consume')).toBe('消费扣减');
    expect(formatLedgerKind('adjust')).toBe('人工调账');
    expect(formatLedgerKind('reversal')).toBe('冲正退款');
    expect(formatLedgerKind('transfer')).toBe('积分转账');
    expect(formatLedgerKind('freeze')).toBe('积分冻结');
    expect(formatLedgerKind('unfreeze')).toBe('积分解冻');
  });
});
