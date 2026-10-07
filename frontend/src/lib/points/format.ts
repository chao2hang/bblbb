/**
 * 积分流水行为与备注说明的本地化 (i18n) 格式化工具。
 */

/**
 * 将积分流水中的英文操作描述/备注转换为清晰的中文说明。
 * 支持历史既有数据（如 "shop purchase 磁带 x1"、"check_in 奖励"）与各业务域流水的中文翻译。
 *
 * @param memo 账本记录中存储的原始 memo
 * @param kind 账本操作类型（如 award, shop_purchase, adjust, reversal 等）
 * @param amount 变动数值（正数为增加，负数为扣减）
 */
export function formatLedgerAction(
  memo: string | null | undefined,
  kind?: string | null,
  amount?: number
): string {
  const trimmed = memo?.trim();

  if (trimmed) {
    // 1. 商城购买流水，如: "shop purchase 磁带 x1"
    const shopMatch = trimmed.match(/^shop\s+purchase\s*(.*)$/i);
    if (shopMatch) {
      const details = shopMatch[1].trim();
      return details ? `商城购买 ${details}` : '商城购买';
    }

    // 2. 商城退款，如: "refund 原因"
    const refundMatch = trimmed.match(/^refund(?::\s*|\s+|$)(.*)$/i);
    if (refundMatch) {
      const reason = refundMatch[1]?.trim();
      return reason ? `退款：${reason}` : '退款';
    }

    // 3. 奖励相关（check_in / task / reaction / post / comment / leaderboard）
    if (/^check[_-]?in(?:\s*奖励)?$/i.test(trimmed)) {
      return '签到奖励';
    }
    if (/^task(?:\s*奖励)?$/i.test(trimmed)) {
      return '任务奖励';
    }
    if (/^reaction(?:\s*奖励)?$/i.test(trimmed)) {
      return '点赞互动奖励';
    }
    if (/^post(?:\s*奖励)?$/i.test(trimmed)) {
      return '发帖奖励';
    }
    if (/^comment(?:\s*奖励)?$/i.test(trimmed)) {
      return '评论奖励';
    }
    if (/^leaderboard(?:\s*奖励)?$/i.test(trimmed)) {
      return '排行榜奖励';
    }
    if (/^reward$/i.test(trimmed)) {
      return '激励奖励';
    }

    // 4. 附件下载，如: "download attachment <id>", "attachment download"
    const downloadMatch = trimmed.match(/^download\s+attachment\s*(.*)$/i);
    if (downloadMatch) {
      const id = downloadMatch[1].trim();
      return id ? `下载附件 ${id}` : '下载附件';
    }
    if (/^attachment\s+download$/i.test(trimmed)) {
      return '下载附件';
    }

    // 5. 付费帖子解锁，如: "unlock paid post <id>"
    const postUnlockMatch = trimmed.match(/^unlock\s+paid\s+post\s*(.*)$/i);
    if (postUnlockMatch) {
      const id = postUnlockMatch[1].trim();
      return id ? `解锁付费帖子 ${id}` : '解锁付费帖子';
    }

    // 6. 应用市场交易
    const mpPurchaseMatch = trimmed.match(/^marketplace\s+purchase\s*(.*)$/i);
    if (mpPurchaseMatch) {
      const id = mpPurchaseMatch[1].trim();
      return id ? `应用市场购买 ${id}` : '应用市场购买';
    }
    const mpCreditMatch = trimmed.match(/^marketplace\s+merchant\s+credit\s*(.*)$/i);
    if (mpCreditMatch) {
      const id = mpCreditMatch[1].trim();
      return id ? `应用市场商户结算 ${id}` : '应用市场商户结算';
    }
    const mpFeeMatch = trimmed.match(/^marketplace\s+platform\s+fee\s*(.*)$/i);
    if (mpFeeMatch) {
      const id = mpFeeMatch[1].trim();
      return id ? `应用市场平台手续费 ${id}` : '应用市场平台手续费';
    }
    const mpRefundDetailMatch = trimmed.match(/^marketplace\s+refund\s+([^:]+):\s*(.*)$/i);
    if (mpRefundDetailMatch) {
      return `应用市场退款 ${mpRefundDetailMatch[1].trim()}：${mpRefundDetailMatch[2].trim()}`;
    }
    const mpRefundMatch = trimmed.match(/^marketplace\s+refund\s*(.*)$/i);
    if (mpRefundMatch) {
      const id = mpRefundMatch[1].trim();
      return id ? `应用市场退款 ${id}` : '应用市场退款';
    }
    const mpMerchantRefundMatch = trimmed.match(/^marketplace\s+merchant\s+refund\s*(.*)$/i);
    if (mpMerchantRefundMatch) {
      const id = mpMerchantRefundMatch[1].trim();
      return id ? `应用市场商户退款 ${id}` : '应用市场商户退款';
    }
    const mpFeeRefundMatch = trimmed.match(/^marketplace\s+fee\s+refund\s*(.*)$/i);
    if (mpFeeRefundMatch) {
      const id = mpFeeRefundMatch[1].trim();
      return id ? `应用市场平台手续费退款 ${id}` : '应用市场平台手续费退款';
    }
    const merchantCompMatch = trimmed.match(/^merchant\s+compensation:\s*(.*)$/i);
    if (merchantCompMatch) {
      return `商户补偿：${merchantCompMatch[1].trim()}`;
    }

    // 7. 管理操作英文备注
    if (/^admin_grant$/i.test(trimmed)) {
      return '管理员发放';
    }
    if (/^admin_adjust$/i.test(trimmed)) {
      return '管理员调账';
    }

    // 已是中文或自定义备注，直接返回
    return trimmed;
  }

  // memo 为空时的兜底
  return formatLedgerKind(kind, amount);
}

/**
 * 格式化账本操作类型（kind）。
 */
export function formatLedgerKind(kind?: string | null, amount?: number): string {
  switch (kind?.toLowerCase()) {
    case 'award':
      return '奖励发放';
    case 'shop_purchase':
      return '商城购买';
    case 'consume':
      return '消费扣减';
    case 'adjust':
      return '人工调账';
    case 'reversal':
      return '冲正退款';
    case 'transfer':
      return '积分转账';
    case 'freeze':
      return '积分冻结';
    case 'unfreeze':
      return '积分解冻';
    case 'credit':
      return '系统入账';
    case 'debit':
      return '消费/扣减';
    case 'checkin':
    case 'check_in':
      return '每日签到';
    case 'admin_adjust':
      return '管理员调整';
    case 'admin_grant':
      return '管理员发放';
    case 'content_unlock':
      return '付费内容解锁';
    case 'attachment_download':
      return '附件资源下载';
    case 'marketplace_order':
      return '应用市场消费';
    case 'reward':
      return '激励奖励';
    default:
      if (amount !== undefined) {
        return amount >= 0 ? '系统入账' : '消费/扣减';
      }
      return '积分变动';
  }
}
