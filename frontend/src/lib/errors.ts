// BBLBB Problem 统一映射（M00-FRONTEND-04）
//
// 服务端所有错误统一返回 application/problem+json（RFC 9457），
// schema 见 openapi/openapi.yaml components.schemas.Problem，
// 稳定机器码注册表见 docs/ERROR-CODES.md。
//
// 页面错误提示一律走本模块：
//   - problemMessage(problem)  按 code → message_key → detail → title → status 降级取文案
//   - fieldError(problem, f)   从 problem.errors[] 取字段级错误（field/code/message_key）
//   - requestIdOf(problem)     透传后端 request_id，供用户联系/排查
//   - retryAfterOf(problem)    429 的 Retry-After（秒，由 client.ts 从响应头附加）
//
// 注意：detail 可能为后端原始英文文案，UI 不应把它当作最终展示语；本模块
// 优先用稳定 code/message_key 命中中文文案，仅作为兜底使用 detail。

export interface ProblemFieldError {
  field?: string;
  code?: string;
  message_key?: string;
}

export interface Problem {
  type?: string;
  title?: string;
  status?: number;
  code?: string;
  detail?: string;
  instance?: string;
  request_id?: string;
  /** 429 时由 client.ts 从 Retry-After 响应头附加（秒）；服务端不返回则为 null。 */
  retry_after?: number | null;
  errors?: ProblemFieldError[];
}

// ─── code → 中文文案（与 docs/ERROR-CODES.md 逐项对齐） ───────────────────────

const MESSAGE_BY_CODE: Record<string, string> = {
  // 通用
  invalid_request: '请求参数有误，请检查后重试',
  bad_request: '请求参数有误，请检查后重试',
  authentication_required: '请先登录后再继续操作',
  unauthorized: '请先登录后再继续操作',
  invalid_token: '登录状态已失效，请重新登录',
  forbidden: '你没有权限执行此操作',
  not_found: '内容不存在或已被移除',
  conflict: '操作冲突，请刷新页面后重试',
  idempotency_conflict: '重复提交了同一操作，请使用新内容重试',
  version_conflict: '内容已发生变化，请刷新后重新编辑',
  csrf_failed: '安全校验失败，请刷新页面后重试',
  rate_limited: '操作过于频繁，请稍后再试',
  too_many_requests: '操作过于频繁，请稍后再试',
  feature_disabled: '该功能当前未开放',
  policy_disabled: '当前策略已关闭此操作',
  policy_version_changed: '策略已更新，请刷新后重试',
  internal_error: '服务器开小差了，请稍后重试',
  validation_failed: '提交的内容未通过校验',

  // 校验（422）
  visibility_level_exceeds_author: '内容的最低可见等级高于你的当前等级',
  invalid_url: '链接地址无效，请修改后重试',
  media_probe_failed: '媒体探测失败，请更换来源或改用外链',
  hls_policy_exceeded: '视频流超出限制，请使用更小流或外链',

  // 账号与内容
  insufficient_funds: '余额不足',
  daily_limit_exceeded: '已达今日上限，请稍后再试',
  activity_already_claimed: '今日任务已领取过',
  activity_not_eligible: '暂未满足该任务条件',
  job_not_retryable: '该任务已结束，无法重试',

  // 商城 / 下载 / 媒体 / AI（长尾，仅给稳定中文兜底）
  checkout_interaction_invalid: '结算确认已过期，请重新发起',
  checkout_user_mismatch: '结算账号与登录账号不一致',
  checkout_intent_expired: '结算意图已过期，请重新发起',
  checkout_intent_consumed: '该结算单已被处理',
  offer_version_changed: '报价已更新，请重新确认',
  refund_not_allowed: '该订单当前不支持退款',
  product_unavailable: '商品已下架或停售',
  product_version_changed: '商品信息已更新，请重新确认',
  shop_purchase_limit_exceeded: '超出该商品购买限制',
  shop_stock_exhausted: '商品库存不足',
  entitlement_not_usable: '持有的物品已失效或数量不足',
  presentation_slot_conflict: '展示位冲突，请刷新衣柜',
  attachment_not_ready: '附件仍在处理中，请稍后查询',
  download_authorization_pending: '下载授权处理中，请稍后查询',
  download_url_unavailable: '暂时无法生成下载链接，请稍后重试',
  media_blocked: '该媒体来源被阻止',
  provider_unavailable: '服务商暂不可用，请稍后重试',
  ai_consent_required: '发送数据前需要你同意 AI 处理条款',
  ai_budget_exceeded: 'AI 用量已超限，请稍后再试',
  ai_suggestion_stale: '内容已更新，建议已过期',
  storage_unavailable: '存储服务暂不可用，请稍后重试'
};

// ─── status → 通用中文文案（code 未命中时的兜底） ─────────────────────────────

const MESSAGE_BY_STATUS: Record<number, string> = {
  400: '请求参数有误，请检查后重试',
  401: '请先登录后再继续操作',
  403: '你没有权限执行此操作',
  404: '内容不存在或已被移除',
  409: '操作冲突，请刷新页面后重试',
  422: '提交的内容未通过校验',
  429: '操作过于频繁，请稍后再试',
  500: '服务器开小差了，请稍后重试',
  502: '服务暂时不可用，请稍后重试',
  503: '服务暂时不可用，请稍后重试'
};

// ─── field → 通用中文文案（字段级错误无 code/message_key 时的兜底） ────────────

const MESSAGE_BY_FIELD: Record<string, string> = {
  username: '用户名不符合要求',
  email: '邮箱格式不正确',
  password: '密码不符合要求',
  title: '标题不符合要求',
  content: '内容不符合要求',
  board_slug: '请选择正确的板块',
  visibility: '可见性设置无效'
};

export function problemMessage(problem: Problem | null | undefined): string {
  if (!problem) return '操作失败，请稍后重试';
  if (problem.code && MESSAGE_BY_CODE[problem.code]) return MESSAGE_BY_CODE[problem.code];
  if (problem.detail) return problem.detail;
  if (problem.title) return problem.title;
  if (problem.status && MESSAGE_BY_STATUS[problem.status]) return MESSAGE_BY_STATUS[problem.status];
  return '操作失败，请稍后重试';
}

/** 字段级错误：在 problem.errors[] 中按 field 查找并映射为中文文案；无则返回 null。 */
export function fieldError(problem: Problem | null | undefined, field: string): string | null {
  const item = problem?.errors?.find((e) => e.field === field);
  if (!item) return null;
  const key = item.message_key ?? item.code;
  if (key && MESSAGE_BY_CODE[key]) return MESSAGE_BY_CODE[key];
  if (item.code && MESSAGE_BY_CODE[item.code]) return MESSAGE_BY_CODE[item.code];
  if (MESSAGE_BY_FIELD[field]) return MESSAGE_BY_FIELD[field];
  return '该字段输入有误';
}

/** 后端透传的 request_id（用于定位/联系客服），无则返回 null。 */
export function requestIdOf(problem: Problem | null | undefined): string | null {
  return problem?.request_id ?? null;
}

/** 429 的 Retry-After 秒数（client.ts 附加），无则返回 null。 */
export function retryAfterOf(problem: Problem | null | undefined): number | null {
  return typeof problem?.retry_after === 'number' ? problem.retry_after : null;
}

/** 组装带 request_id 的完整错误文案，供表单顶部/Toast 使用。 */
export function problemText(problem: Problem | null | undefined): string {
  const message = problemMessage(problem);
  const rid = requestIdOf(problem);
  return rid ? `${message}（请求号 ${rid}）` : message;
}
