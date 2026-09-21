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
  presentation_tokens?: {
    nickname_color?: string;
    avatar_frame?: string;
    avatar_frame_attachment_id?: string;
    avatar_attachment?: string;
    avatar_attachment_attachment_id?: string;
    profile_effect?: string;
    post_effect?: string;
    profile_badges?: Array<string>;
  } | null;
  equipped_achievements?: Array<{
    code: string;
    name: string;
  }>;
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
  remember?: boolean;
}
export interface LoginMfaChallenge {
  mfa_required: boolean;
  challenge_token: string;
  passkey_available: boolean;
}
export interface LoginMfaRequest {
  challenge_token: string;
  totp_code?: string;
  recovery_code?: string;
  passkey?: PasskeyAssertion;
}
export interface PasskeyAssertion {
  id: string;
  rawId: string;
  type: "public-key";
  response: {
    authenticatorData: string;
    clientDataJSON: string;
    signature: string;
    userHandle?: string;
  };
  clientExtensionResults?: Record<string, unknown>;
  authenticatorAttachment?: string;
}
export interface PasskeyLoginOptionsRequest {
  challenge_token: string;
}
export interface PasskeyRegistrationOptions {
  publicKey: {
    rp: {
      id?: string;
      name?: string;
    };
    user: {
      id?: string;
      name?: string;
      displayName?: string;
    };
    challenge: string;
    pubKeyCredParams: Array<{
      type?: string;
      alg?: number;
    }>;
    timeout?: number;
    excludeCredentials?: Array<{
      type?: string;
      id?: string;
      transports?: Array<string>;
    }>;
    authenticatorSelection?: {
      residentKey?: string;
      requireResidentKey?: boolean;
      userVerification?: string;
    };
    attestation?: string;
  };
}
export interface PasskeyConfirmRequest {
  name?: string;
  credential: PasskeyRegistration;
}
export interface PasskeyRegistration {
  id: string;
  rawId: string;
  type: "public-key";
  response: {
    clientDataJSON: string;
    attestationObject: string;
    transports?: Array<string>;
  };
  clientExtensionResults?: Record<string, unknown>;
  authenticatorAttachment?: string;
}
export interface PasskeyRequestOptions {
  publicKey: {
    rpId?: string;
    challenge: string;
    timeout?: number;
    allowCredentials?: Array<{
      type?: string;
      id?: string;
      transports?: Array<string>;
    }>;
    userVerification?: string;
  };
}
export interface PasskeyCredentialInfo {
  id: string;
  name: string;
  aaguid?: string;
  backup_eligible?: boolean;
  backed_up?: boolean;
  created_at: number;
  last_used_at?: number;
}
export interface PasskeyListResponse {
  passkeys: Array<PasskeyCredentialInfo>;
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
  avatar_attachment_id?: string | null;
}
export interface PostCreate {
  type: "article" | "discussion";
  title: string;
  markdown: string;
  board_id: string;
  visibility_level: number;
  access_policy: "public" | "logged_in" | "after_reply" | "level" | "paid";
  scheduled_at?: string | null;
  summary?: string | null;
  price_coin?: number | null;
  tags?: Array<string> | null;
  client_request_id: string;
}
export interface PostPatch {
  title?: string;
  markdown?: string;
  visibility_level?: number;
  access_policy?: "public" | "logged_in" | "after_reply" | "level" | "paid";
  tags?: Array<string> | null;
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
  presentation_tokens?: unknown /* unresolvable: #/components/schemas/PublicUser/properties/presentation_tokens */;
  avatar_attachment_id?: string | null;
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
  presentation_tokens?: unknown /* unresolvable: #/components/schemas/PublicUser/properties/presentation_tokens */;
  avatar_attachment_id?: string | null;
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
  icon?: string;
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
  tags?: Array<string>;
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
  code: "invalid_request" | "visibility_level_exceeds_author" | "invalid_url" | "idempotency_conflict" | "version_conflict" | "unauthorized" | "invalid_credentials" | "mfa_code_invalid" | "mfa_challenge_invalid" | "mfa_confirm_invalid" | "mfa_enrollment_invalid" | "passkey_not_configured" | "passkey_unavailable" | "passkey_challenge_invalid" | "passkey_registration_invalid" | "passkey_limit_reached" | "reauth_password_invalid" | "verification_token_invalid" | "reset_token_invalid" | "validation_failed" | "invalid_current_password" | "forbidden" | "step_up_required" | "not_found" | "csrf_failed" | "origin_not_allowed" | "host_not_allowed" | "rate_limited" | "crawler_denied" | "challenge_required" | "temporarily_banned" | "feature_disabled" | "insufficient_funds" | "daily_limit_exceeded" | "checkout_interaction_invalid" | "checkout_user_mismatch" | "checkout_intent_expired" | "checkout_intent_consumed" | "offer_version_changed" | "refund_not_allowed" | "product_unavailable" | "shop_purchase_limit_exceeded" | "shop_stock_exhausted" | "entitlement_not_usable" | "presentation_slot_conflict" | "activity_already_claimed" | "activity_not_eligible" | "invalid_price_coin" | "post_not_paid" | "price_not_configured" | "self_reaction" | "cannot_follow_self" | "cannot_message_self" | "download_url_unavailable" | "ai_consent_required" | "ai_budget_exceeded" | "ai_suggestion_stale" | "invalid_storage_request" | "storage_partial_upload" | "storage_forbidden" | "storage_auth_failed" | "storage_rate_limited" | "quota_exceeded" | "storage_conflict" | "storage_state_error" | "storage_verification_failed" | "storage_network_error" | "storage_upstream_error" | "theme_invalid" | "theme_incompatible" | "theme_not_found" | "theme_conflict" | "plugin_invalid" | "plugin_incompatible" | "plugin_not_found" | "plugin_conflict" | "marketplace_disabled" | "marketplace_invalid_client" | "refund_exceeds_purchase" | "merchant_balance_insufficient" | "webhook_invalid_signature" | "bad_request" | "conflict" | "video_insecure_scheme" | "video_invalid_url" | "video_host_invalid" | "video_port_not_allowed" | "video_private_ip" | "video_signed_url" | "video_userinfo_not_allowed" | "video_fragment_not_allowed" | "video_unsupported_type" | "video_not_video_page" | "video_invalid" | "video_mime_mismatch" | "video_no_embed_permission" | "video_takedown" | "video_provider_disabled" | "video_provider_host_not_allowed" | "video_provider_ratelimited" | "video_provider_unavailable" | "video_policy_changed" | "video_policy_version_conflict" | "video_poster_attachment_invalid" | "video_resolution_expired" | "video_embed_not_found" | "video_embed_referenced" | "video_target_conflict" | "video_target_forbidden" | "video_target_not_found" | "video_version_conflict" | "video_egress_http_error" | "video_egress_private_ip" | "video_egress_timeout" | "video_egress_too_large" | "video_egress_too_many_redirects" | "video_egress_unavailable" | "video_hls_invalid" | "video_hls_depth_exceeded" | "video_hls_segment_count_exceeded" | "video_hls_duration_exceeded" | "video_hls_cross_origin_segment" | "video_hls_key_not_allowed" | "video_hls_map_not_allowed" | "video_hls_signed_uri" | "internal_error";
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
  tags?: Array<string> | null;
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
  tags?: Array<string>;
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
export interface AdminStudioPublishRequest {
  cosmetic: {
    id?: string;
    kind: "nickname_color" | "avatar_frame" | "cosmetic_badge" | "profile_effect" | "post_effect" | "reaction_pack" | "utility" | "title_prefix";
    name?: string;
    style?: Record<string, unknown>;
  };
  product: {
    title: string;
    slug?: string;
    unit_price: number;
    stock_remaining?: number;
    required_level?: number;
    quantity_limit?: number;
    validity_seconds?: number;
    sale_start_at?: number;
    sale_end_at?: number;
    refund_policy?: "non_refundable" | "compensation_only" | "full_refund";
    status?: "draft" | "published";
    description_safe?: string;
    asset_attachment_id?: string;
    currency_id?: string;
  };
  reason?: string;
  client_request_id?: string;
}
export interface AdminStudioPublishResponse {
  cosmetic?: {
    id?: string;
    kind?: string;
    name?: string;
    style?: Record<string, unknown>;
    status?: "active" | "archived";
    updatedAt?: number;
  };
  product?: {
    id?: string;
    kind?: string;
    slug?: string;
    title?: string;
    status?: string;
    unit_price?: number;
    validity_seconds?: number;
  };
  replayed?: boolean;
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
export interface RiskThresholds {
  new_user_max_posts: number;
  new_user_grace_secs: number;
  max_links: number;
  sensitive_words: Array<string>;
  max_frequency_posts: number;
  frequency_window_secs: number;
  duplicate_window_secs: number;
}
export interface RiskPolicyView {
  version: number;
  thresholds: RiskThresholds;
}
export interface BoardRoleAssignment {
  id: string;
  board_id: string;
  user_id: string;
  username?: string;
  role_id: string;
  role_name: string;
  granted_by?: string | null;
  granted_at: number;
  expires_at?: number | null;
}
export interface InteractionDecision {
  decision: "allow" | "deny";
}
export interface ThemePreference {
  theme: string;
}
export interface PasswordChangeRequest {
  current_password: string;
  new_password: string;
}
export interface UnlockRequest {
  client_request_id: string;
}
export interface CreateApiKeyRequest {
  name: string;
  scopes: Array<string>;
  client_request_id: string;
}
export interface CreateConversationRequest {
  username: string;
  client_request_id: string;
}
export interface SendMessageRequest {
  body: string;
  client_request_id: string;
}

