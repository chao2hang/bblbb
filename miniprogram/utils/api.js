/**
 * BBLBB 领域 API 层（对 request.js 的薄封装）。
 *
 * 每个函数对应一个后端端点，字段命名与 openapi/openapi.yaml 及后端
 * 实际响应一致（已对真实后端逐条验证）。
 */

const { get, post, patch, del } = require('./request');
const store = require('./store');
const { uuidv4 } = require('./uuid');

// ── 认证 ─────────────────────────────────────────────────────────────────────

/** 登录（identifier 用户名或邮箱）。可能进入 MFA 第二步。 */
function login(identifier, password, remember) {
  return post('/api/v1/auth/login', {
    identifier,
    password,
    remember: !!remember,
  });
}

/** MFA 第二步登录（totp_code 与 recovery_code 二选一） */
function loginMfa(challengeToken, { totpCode, recoveryCode }) {
  return post('/api/v1/auth/login/mfa', {
    challenge_token: challengeToken,
    totp_code: totpCode || null,
    recovery_code: recoveryCode || null,
  });
}

/** 注册（成功 201 {ok:true}；邮箱是否已存在不泄漏） */
function register(username, password, email) {
  return post('/api/v1/auth/register', { username, password, email });
}

/** 请求找回密码（统一 202，不泄漏邮箱是否注册） */
function requestPasswordReset(email) {
  return post('/api/v1/auth/password-reset', { email });
}

/** 确认找回密码（邮件 token + 新密码） */
function confirmPasswordReset(token, password) {
  return post('/api/v1/auth/password-reset/confirm', { token, password });
}

/** 登出（204 + 清除会话 cookie） */
function logout() {
  return del('/api/v1/auth/session');
}

/** 设备（会话）列表 */
function listSessions() {
  return get('/api/v1/auth/sessions');
}

/** 逐设备撤销 */
function revokeSession(sessionId) {
  return del(`/api/v1/auth/sessions/${sessionId}`);
}

/** 全部登出（撤销所有设备会话） */
function logoutAll() {
  return del('/api/v1/auth/sessions');
}

// ── 用户 ─────────────────────────────────────────────────────────────────────

/** 当前用户（Me 投影；version 用于乐观并发 If-Match） */
function me() {
  return get('/api/v1/me');
}

/**
 * 更新资料。
 * @param {object} fields display_name / bio / signature / timezone
 * @param {number} version 当前 me.version（If-Match）
 */
function updateMe(fields, version) {
  return patch('/api/v1/me', fields, { header: { 'If-Match': String(version) } });
}

/** 公开用户资料 */
function publicUser(username) {
  return get(`/api/v1/users/${encodeURIComponent(username)}`);
}

/** 关注 / 取关 */
function followUser(username) {
  return post(`/api/v1/users/${encodeURIComponent(username)}/follow`);
}

function unfollowUser(username) {
  return del(`/api/v1/users/${encodeURIComponent(username)}/follow`);
}

// ── 板块 / 标签 ──────────────────────────────────────────────────────────────

/** 板块列表（{items, page{next_cursor, has_more}}） */
function listBoards(opts = {}) {
  return get('/api/v1/boards', opts);
}

/** 板块详情 */
function getBoard(slug) {
  return get(`/api/v1/boards/${encodeURIComponent(slug)}`);
}

/** 板块帖子（sort: latest | hot | featured | unanswered） */
function listBoardPosts(slug, opts = {}) {
  return get(`/api/v1/boards/${encodeURIComponent(slug)}/posts`, opts);
}

/** 标签列表 */
function listTags() {
  return get('/api/v1/tags');
}

// ── 帖子 ─────────────────────────────────────────────────────────────────────

/**
 * 帖子列表。
 * @param {object} opts board_id / author_id / author_username / tag /
 *   type(article|topic) / sort(latest|popular|featured|unanswered) /
 *   after / limit
 */
function listPosts(opts = {}) {
  return get('/api/v1/posts', opts);
}

/** 帖子详情（含 body_html / access_summary / capabilities） */
function getPost(id) {
  return get(`/api/v1/posts/${id}`);
}

/**
 * 创建帖子。
 * @param {object} p {type: 'article'|'discussion', title, markdown,
 *   board_id, access_policy: 'public'|'logged_in'|'after_reply'|'level'|'paid',
 *   price_coin?, summary?, tags?}
 * client_request_id 由本模块自动生成（幂等）。
 */
function createPost(p) {
  return post('/api/v1/posts', {
    type: p.type,
    title: p.title,
    markdown: p.markdown,
    board_id: p.board_id,
    access_policy: p.access_policy || 'public',
    price_coin: p.price_coin,
    summary: p.summary,
    tags: p.tags,
    client_request_id: uuidv4(),
  });
}

/** 修改帖子（若-Match 乐观并发；作者本人） */
function updatePost(id, { title, markdown }, version) {
  const body = {};
  if (title !== undefined) body.title = title;
  if (markdown !== undefined) body.markdown = markdown;
  return patch(`/api/v1/posts/${id}`, body, { header: { 'If-Match': String(version) } });
}

/** 收藏 / 取消收藏 */
function favoritePost(id) {
  return post(`/api/v1/posts/${id}/favorite`);
}

function unfavoritePost(id) {
  return del(`/api/v1/posts/${id}/favorite`);
}

/** 我的收藏（{items, page}） */
function listMyFavorites(opts = {}) {
  return get('/api/v1/me/favorites', opts);
}

/** 点赞（reaction 目前仅 "like"；不能赞自己的内容） */
function reactPost(id, reaction = 'like') {
  return post(`/api/v1/posts/${id}/reactions`, { reaction });
}

function unreactPost(id, reaction = 'like') {
  return del(`/api/v1/posts/${id}/reactions/${reaction}`);
}

// ── 评论（楼层） ─────────────────────────────────────────────────────────────

/** 帖子评论列表（游标 after=base64url("floor:id")） */
function listComments(postId, opts = {}) {
  return get(`/api/v1/posts/${postId}/comments`, opts);
}

/** 发表评论（parent_id 可选：回复楼层） */
function createComment(postId, markdown, parentId) {
  return post(`/api/v1/posts/${postId}/comments`, {
    markdown,
    parent_id: parentId || null,
    client_request_id: uuidv4(),
  });
}

/** 编辑评论（作者本人；If-Match 版本） */
function updateComment(id, markdown, version) {
  return patch(`/api/v1/comments/${id}`, { markdown }, { header: { 'If-Match': String(version) } });
}

/** 删除评论（作者本人） */
function deleteComment(id) {
  return del(`/api/v1/comments/${id}`);
}

// ── 搜索 ─────────────────────────────────────────────────────────────────────

/** 全站搜索（GET /api/v1/search?q=） */
function search(q, opts = {}) {
  return get('/api/v1/search', { q, ...opts });
}

// ── 活跃 / 等级 / 签到 ──────────────────────────────────────────────────────

/** 今日签到状态 / 连续天数 / 等级 / 经验 */
function activitySummary() {
  return get('/api/v1/activity/summary');
}

/**
 * 记录有效业务页面访问并自动签到（每日首次）。
 * path 必须是业务页面路径（/boards、/posts、/me 等）。
 * 注意：请求需携带正常浏览器 UA（微信 wx.request 运行时 UA 满足要求）。
 */
function recordVisit(path = '/me') {
  return post('/api/v1/activity/visit', { path });
}

/** 我的积分流水（{items, next_cursor}） */
function listMyPointTransactions(opts = {}) {
  return get('/api/v1/me/point-transactions', opts);
}

// ── 成就 ─────────────────────────────────────────────────────────────────────

/** 成就目录 */
function listAchievements() {
  return get('/api/v1/achievements');
}

/** 我的成就 */
function listMyAchievements() {
  return get('/api/v1/me/achievements');
}

// ── 积分商城 ─────────────────────────────────────────────────────────────────

/** 商品列表（{products: []}） */
function listShopProducts() {
  return get('/api/v1/shop/products');
}

/** 购买商品（原子扣款；幂等键自动生成） */
function buyProduct(productId, quantity = 1) {
  return post('/api/v1/shop/orders', {
    product_id: productId,
    quantity,
    idempotency_key: uuidv4(),
  });
}

/** 我的权益 */
function listMyEntitlements() {
  return get('/api/v1/me/entitlements');
}

/** 装备 / 卸下装扮权益 */
function equipEntitlement(id) {
  return post(`/api/v1/me/entitlements/${id}/equip`);
}

function unequipEntitlement(id) {
  return post(`/api/v1/me/entitlements/${id}/unequip`);
}

/** 我的装扮投影（头像框/昵称色等） */
function getPresentation() {
  return get('/api/v1/me/presentation');
}

// ── 付费内容 ─────────────────────────────────────────────────────────────────

/** 解锁付费帖子（扣金币；client_request_id 1-200 字符） */
function unlockPost(id) {
  return post(`/api/v1/posts/${id}/unlock`, { client_request_id: uuidv4() });
}

// ── 通用：会话校验 ──────────────────────────────────────────────────────────

/**
 * 应用启动时静默校验登录态：拉取最新 Me 并写回缓存；
 * 401 时 request 层已清理会话。
 * @returns {Promise<object|null>} 最新 Me；未登录返回 null
 */
async function refreshMe() {
  if (!store.isLoggedIn()) return null;
  try {
    const data = await me();
    store.setMe(data);
    return data;
  } catch (e) {
    return store.getMe();
  }
}

module.exports = {
  // 认证
  login,
  loginMfa,
  register,
  requestPasswordReset,
  confirmPasswordReset,
  logout,
  listSessions,
  revokeSession,
  logoutAll,
  // 用户
  me,
  updateMe,
  publicUser,
  followUser,
  unfollowUser,
  // 板块
  listBoards,
  getBoard,
  listBoardPosts,
  listTags,
  // 帖子
  listPosts,
  getPost,
  createPost,
  updatePost,
  favoritePost,
  unfavoritePost,
  listMyFavorites,
  reactPost,
  unreactPost,
  // 评论
  listComments,
  createComment,
  updateComment,
  deleteComment,
  // 搜索
  search,
  // 活跃
  activitySummary,
  recordVisit,
  listMyPointTransactions,
  // 成就
  listAchievements,
  listMyAchievements,
  // 商城
  listShopProducts,
  buyProduct,
  listMyEntitlements,
  equipEntitlement,
  unequipEntitlement,
  getPresentation,
  // 付费
  unlockPost,
  // 会话
  refreshMe,
};
