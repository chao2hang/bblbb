/**
 * AUTO-GENERATED — DO NOT EDIT.
 * Source: openapi/openapi.yaml (components.schemas)
 * Generator: scripts/generate-ts-types.rb
 *
 * Frozen baseline: contract version 1.0.0 (see README.md in this directory).
 * Regenerate with: ruby scripts/generate-ts-types.rb
 * Verify no drift with: ruby scripts/generate-ts-types.rb --check
 */

export type GenericRequest = Record<string, unknown>;
export interface DeviceSession {
  id: string;
  user_agent: string | null;
  created_at: number;
  last_seen_at: number;
  absolute_expires_at: number;
  version: number;
}
export interface RegisterRequest {
  username: string;
  email: string;
  password: string;
}
export interface AttachmentCreate {
  filename: string;
  size: number;
  declared_media_type: string;
  target_type?: string | null;
  target_id?: string | null;
}
export interface PublicUser {
  id: string;
  username: string;
  display_name: string | null;
  level: number;
  bio?: string | null;
  avatar_attachment_id?: string | null;
  cover_attachment_id?: string | null;
  signature?: string | null;
  created_at: number;
}
export type GenericSuccess = Record<string, unknown>;
export interface Health {
  status: "ok";
  version: string;
}
export interface LoginRequest {
  identifier: string;
  password: string;
}
export interface LoginMfaChallenge {
  mfa_required: boolean;
  challenge_token: string;
}
export interface LoginMfaRequest {
  challenge_token: string;
  totp_code?: string;
  recovery_code?: string;
}
export type LoginResult = Me | LoginMfaChallenge;
export interface ProfilePatch {
  display_name?: string;
  bio?: string;
  signature?: string;
  timezone?: string;
  theme?: "default" | "dark" | "light";
  email_visible_to?: "everyone" | "registered" | "nobody";
  profile_visible_to?: "everyone" | "registered" | "nobody";
}
export interface PostCreate {
  type: "article" | "discussion";
  title: string;
  markdown: string;
  board_id: string;
  visibility_level: number;
  access_policy: "public" | "logged_in" | "after_reply" | "level" | "paid";
  scheduled_at?: string | null;
  client_request_id: string;
}
export interface PostPatch {
  title?: string;
  markdown?: string;
  visibility_level?: number;
  access_policy?: "public" | "logged_in" | "after_reply" | "level" | "paid";
}
export interface CommentCreate {
  markdown: string;
  parent_id?: string | null;
  client_request_id: string;
}
export interface ResourceMeta {
  id: string;
  version: number;
  created_at: string;
  updated_at: string;
}
export interface Author {
  username: string;
  display_name: string | null;
  level: number;
  profile_url: string;
}
export type Me = ResourceMeta & {
  username: string;
  email: string;
  email_verified: boolean;
  status: string;
  display_name?: string | null;
  bio?: string | null;
  signature?: string | null;
  timezone?: string;
  theme_name?: string | null;
  email_visible_to?: "everyone" | "registered" | "nobody";
  profile_visible_to?: "everyone" | "registered" | "nobody";
  level: number;
  roles: Array<string>;
  mfa_enabled?: boolean;
};
export interface AdminUser {
  id: string;
  username: string;
  email: string;
  email_verified: boolean;
  status: string;
  display_name?: string | null;
  level: number;
  roles: Array<string>;
  created_at: number;
  updated_at: number;
  last_login_at?: number | null;
  delete_requested_at?: number | null;
  deleted_at?: number | null;
}
export type Board = ResourceMeta & {
  slug: string;
  name: string;
  description?: string;
  parent_id?: string;
  visibility?: "public" | "members" | "restricted" | "hidden";
  posting_mode?: "normal" | "approval" | "readonly" | "closed";
  post_count: number;
  is_active?: number;
};
export type Post = ResourceMeta & {
  post_type: "article" | "discussion";
  title: string;
  author: Author;
  status: string;
  body_html?: string;
  access_summary: AccessSummary;
  capabilities: Array<string>;
};
export type Comment = ResourceMeta & {
  author: Author;
  status: string;
  body_html?: string;
};
export interface AccessSummary {
  policy: "public" | "logged_in" | "after_reply" | "level" | "paid";
  required_level?: number;
  unlocked: boolean;
}
export interface PageInfo {
  next_cursor: string | null;
  has_more: boolean;
}
export interface BoardPage {
  items: Array<Board>;
  page: PageInfo;
}
export interface PostPage {
  items: Array<Post>;
  page: PageInfo;
}
export interface CommentPage {
  items: Array<Comment>;
  page: PageInfo;
}
export interface ActivityVisitResult {
  checked_in_today: boolean;
  streak_days: number;
  today_earned: Array<Money>;
  point_operation_id?: string;
}
export interface Money {
  currency: string;
  amount: number;
}
export interface Problem {
  type: string;
  title: string;
  status: number;
  code: "invalid_request" | "visibility_level_exceeds_author" | "invalid_url" | "idempotency_conflict" | "version_conflict" | "authentication_required" | "invalid_token" | "forbidden" | "step_up_required" | "not_found" | "csrf_failed" | "origin_not_allowed" | "host_not_allowed" | "rate_limited" | "feature_disabled" | "policy_disabled" | "policy_version_changed" | "insufficient_funds" | "daily_limit_exceeded" | "checkout_interaction_invalid" | "checkout_user_mismatch" | "checkout_intent_expired" | "checkout_intent_consumed" | "offer_version_changed" | "refund_not_allowed" | "product_unavailable" | "product_version_changed" | "shop_purchase_limit_exceeded" | "shop_stock_exhausted" | "entitlement_not_usable" | "presentation_slot_conflict" | "activity_already_claimed" | "activity_not_eligible" | "attachment_not_ready" | "download_authorization_pending" | "download_url_unavailable" | "media_blocked" | "media_probe_failed" | "hls_policy_exceeded" | "provider_unavailable" | "ai_consent_required" | "ai_budget_exceeded" | "ai_suggestion_stale" | "job_not_retryable" | "storage_unavailable" | "internal_error";
  detail: string;
  instance?: string;
  request_id: string;
  errors?: Array<{
    field?: string;
    code: string;
    message_key: string;
  }>;
}
export type EmptyRequest = Record<string, unknown>;
export interface TokenRequest {
  token: string;
}
export interface PasswordResetRequest {
  email: string;
}
export interface PasswordResetConfirm {
  token: string;
  password: string;
}
export interface TotpEnrollResponse {
  otpauth_uri: string;
  secret_base32: string;
  issuer: string;
  account: string;
}
export interface MfaConfirmRequest {
  code: string;
}
export interface ReAuthRequest {
  password: string;
}
export interface RecoveryCodesResult {
  codes: Array<string>;
  only_shown_once: boolean;
}
export interface CsrfToken {
  token: string;
}
export interface Page {
  next_cursor: string | null;
  has_more: boolean;
}
export interface TaskAccepted {
  task_id: string;
  status: null;
  poll_url: string;
  cancel_url?: string | null;
  source_revision?: number | null;
  policy_version: number;
}
export interface SearchResult {
  id: string;
  type: "post" | "user" | "board" | "tag";
  title: string;
  url: string;
  excerpt: string;
}
export interface SearchPage {
  items: Array<SearchResult>;
  page: Page;
}
export interface DraftCreate {
  type: "article" | "discussion";
  title: string;
  markdown: string;
  board_id: string;
  visibility_level: number;
  access_policy: "public" | "logged_in" | "after_reply" | "level" | "paid";
  scheduled_at?: string | null;
  client_request_id: string;
}
export interface DraftPatch {
  title?: string;
  markdown?: string;
  board_id?: string;
  visibility_level?: number;
  access_policy?: "public" | "logged_in" | "after_reply" | "level" | "paid";
  scheduled_at?: string | null;
}
export interface DraftPreviewRequest {
  markdown: string;
  restricted_markdown?: string;
}
export interface DraftPreview {
  html: string;
  restricted_html?: string | null;
  excerpt: string;
}
export type Draft = ResourceMeta & {
  type: "article" | "discussion";
  title: string;
  markdown: string;
  board_id: string;
  visibility_level: number;
  access_policy: string;
  scheduled_at?: string | null;
};
export type Revision = ResourceMeta & {
  resource_id: string;
  editor: Author;
  reason: string;
};
export interface ReportCreate {
  target_type: "post" | "comment" | "user" | "attachment";
  target_id: string;
  reason_code: "spam" | "harassment" | "illegal_content" | "privacy" | "copyright" | "malware" | "wrong_board" | "other";
  details?: string | null;
}
export interface AppealCreate {
  sanction_id: string;
  content: string;
}
export interface ModerationDecision {
  decision: string;
  reason: string;
  expected_version: number;
}
export interface SanctionCreate {
  case_id: string;
  user_id: string;
  type: "warning" | "rate_limit" | "mute" | "board_mute" | "ban";
  starts_at: string;
  ends_at: string | null;
  reason: string;
}
export type Notification = ResourceMeta & {
  type: string;
  read_at: string | null;
  safe_summary?: string;
};
export interface NotificationPage {
  items: Array<Notification>;
  page: Page;
}
export interface AttachmentComplete {
  client_request_id: string;
}
export interface ProfileCoverSet {
  attachment_id: string;
  alt_text: string;
  position: string;
}
export interface DownloadRequest {
  target_type?: "post" | "comment" | null;
  target_id?: string | null;
  expected_policy_version?: number | null;
  client_request_id: string;
}
export interface DownloadResult {
  authorization_id: string;
  attachment_id: string;
  charged: Money;
  download_url: string;
  url_expires_at: string;
  authorization_expires_at: string;
  reused_authorization: boolean;
}
export interface ReactionCreate {
  reaction: string;
}
export interface EntitlementEquip {
  expected_presentation_version: number;
}
export interface ShopOrderCreate {
  product_id: string;
  expected_product_version: number;
  quantity: number;
  client_request_id: string;
}
export interface AiConsentCreate {
  provider_id: string;
  purpose: string;
  data_mode: null;
  disclosure_version: number;
  disclosure_hash: string;
}
export interface SuggestionAccept {
  expected_base_version: number;
  selected_fields?: Array<string>;
}
export interface VideoResolveRequest {
  source_url: string;
  target_type: "post" | "comment";
  target_id?: string | null;
}
export interface VideoEmbedCreate {
  resolution_id: string;
  target_type: "post" | "comment";
  target_id: string;
  expected_policy_version: number;
}
export interface VideoEmbedPatch {
  title_override?: string | null;
  poster_override_attachment_id?: string | null;
  version?: number;
}
export interface OfferCreate {
  external_offer_id: string;
  title: string;
  description: string;
  currency_id: string;
  unit_amount: number;
  quantity_min: number;
  quantity_max: number;
  stock_policy: "unlimited" | "finite";
  stock_remaining: number | null;
}
export interface CheckoutIntentCreate {
  offer_id: string;
  expected_offer_version: number;
  merchant_order_id: string;
  quantity: number;
}
export interface CheckoutConfirm {
  interaction_id: string;
  decision: "confirm" | "deny";
  expected_intent_version: number;
}
export interface RefundCreate {
  amount: Money | null;
  reason_code: string;
  merchant_refund_id: string;
}
export interface PolicyPatch {
  expected_version: number;
  reason: string;
  changes: Record<string, unknown>;
}
export interface InteractionDecision {
  decision: "allow" | "deny";
}
export interface ThemePreference {
  theme: string;
}

