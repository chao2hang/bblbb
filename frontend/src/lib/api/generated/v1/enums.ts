/**
 * AUTO-GENERATED — DO NOT EDIT.
 * Source: openapi/openapi.yaml (components.schemas)
 * Generator: scripts/generate-ts-types.rb
 *
 * Frozen baseline: contract version 1.0.0 (see README.md in this directory).
 * Regenerate with: ruby scripts/generate-ts-types.rb
 * Verify no drift with: ruby scripts/generate-ts-types.rb --check
 */

export type HealthStatus = "ok";
export type ProfilePatchTheme = "default" | "dark" | "light";
export type ProfilePatchEmailVisibleTo = "everyone" | "registered" | "nobody";
export type ProfilePatchProfileVisibleTo = "everyone" | "registered" | "nobody";
export type PostCreateType = "article" | "discussion";
export type PostCreateAccessPolicy = "public" | "logged_in" | "after_reply" | "level" | "paid";
export type PostPatchAccessPolicy = "public" | "logged_in" | "after_reply" | "level" | "paid";
export type MeEmailVisibleTo = "everyone" | "registered" | "nobody";
export type MeProfileVisibleTo = "everyone" | "registered" | "nobody";
export type BoardVisibility = "public" | "members" | "restricted" | "hidden";
export type BoardPostingMode = "normal" | "approval" | "readonly" | "closed";
export type PostPostType = "article" | "discussion";
export type AccessSummaryPolicy = "public" | "logged_in" | "after_reply" | "level" | "paid";
export type ProblemCode = "invalid_request" | "visibility_level_exceeds_author" | "invalid_url" | "idempotency_conflict" | "version_conflict" | "authentication_required" | "invalid_token" | "forbidden" | "step_up_required" | "not_found" | "csrf_failed" | "origin_not_allowed" | "host_not_allowed" | "rate_limited" | "feature_disabled" | "policy_disabled" | "policy_version_changed" | "insufficient_funds" | "daily_limit_exceeded" | "checkout_interaction_invalid" | "checkout_user_mismatch" | "checkout_intent_expired" | "checkout_intent_consumed" | "offer_version_changed" | "refund_not_allowed" | "product_unavailable" | "product_version_changed" | "shop_purchase_limit_exceeded" | "shop_stock_exhausted" | "entitlement_not_usable" | "presentation_slot_conflict" | "activity_already_claimed" | "activity_not_eligible" | "attachment_not_ready" | "download_authorization_pending" | "download_url_unavailable" | "media_blocked" | "media_probe_failed" | "hls_policy_exceeded" | "provider_unavailable" | "ai_consent_required" | "ai_budget_exceeded" | "ai_suggestion_stale" | "job_not_retryable" | "storage_unavailable" | "internal_error";
export type SearchResultType = "post" | "user" | "board" | "tag";
export type DraftCreateType = "article" | "discussion";
export type DraftCreateAccessPolicy = "public" | "logged_in" | "after_reply" | "level" | "paid";
export type DraftPatchAccessPolicy = "public" | "logged_in" | "after_reply" | "level" | "paid";
export type DraftType = "article" | "discussion";
export type ReportCreateTargetType = "post" | "comment" | "user" | "attachment";
export type ReportCreateReasonCode = "spam" | "harassment" | "illegal_content" | "privacy" | "copyright" | "malware" | "wrong_board" | "other";
export type SanctionCreateType = "warning" | "rate_limit" | "mute" | "board_mute" | "ban";
export type DownloadRequestTargetType = "post" | "comment";
export type VideoResolveRequestTargetType = "post" | "comment";
export type VideoEmbedCreateTargetType = "post" | "comment";
export type OfferCreateStockPolicy = "unlimited" | "finite";
export type CheckoutConfirmDecision = "confirm" | "deny";
export type InteractionDecisionDecision = "allow" | "deny";
