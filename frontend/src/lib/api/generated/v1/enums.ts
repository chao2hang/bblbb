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
export type ProblemCode = "invalid_request" | "visibility_level_exceeds_author" | "invalid_url" | "idempotency_conflict" | "version_conflict" | "unauthorized" | "forbidden" | "step_up_required" | "not_found" | "csrf_failed" | "origin_not_allowed" | "host_not_allowed" | "rate_limited" | "crawler_denied" | "challenge_required" | "temporarily_banned" | "feature_disabled" | "insufficient_funds" | "daily_limit_exceeded" | "checkout_interaction_invalid" | "checkout_user_mismatch" | "checkout_intent_expired" | "checkout_intent_consumed" | "offer_version_changed" | "refund_not_allowed" | "product_unavailable" | "shop_purchase_limit_exceeded" | "shop_stock_exhausted" | "entitlement_not_usable" | "presentation_slot_conflict" | "activity_already_claimed" | "activity_not_eligible" | "download_url_unavailable" | "ai_consent_required" | "ai_budget_exceeded" | "ai_suggestion_stale" | "invalid_storage_request" | "storage_partial_upload" | "storage_forbidden" | "storage_auth_failed" | "storage_rate_limited" | "quota_exceeded" | "storage_conflict" | "storage_state_error" | "storage_verification_failed" | "storage_network_error" | "storage_upstream_error" | "theme_invalid" | "theme_incompatible" | "theme_not_found" | "theme_conflict" | "plugin_invalid" | "plugin_incompatible" | "plugin_not_found" | "plugin_conflict" | "marketplace_disabled" | "marketplace_invalid_client" | "refund_exceeds_purchase" | "merchant_balance_insufficient" | "webhook_invalid_signature" | "bad_request" | "conflict" | "not_implemented" | "video_insecure_scheme" | "video_invalid_url" | "video_host_invalid" | "video_port_not_allowed" | "video_private_ip" | "video_signed_url" | "video_userinfo_not_allowed" | "video_fragment_not_allowed" | "video_unsupported_type" | "video_not_video_page" | "video_invalid" | "video_mime_mismatch" | "video_no_embed_permission" | "video_takedown" | "video_provider_disabled" | "video_provider_host_not_allowed" | "video_provider_ratelimited" | "video_provider_unavailable" | "video_policy_changed" | "video_policy_version_conflict" | "video_poster_attachment_invalid" | "video_resolution_expired" | "video_embed_not_found" | "video_embed_referenced" | "video_target_conflict" | "video_target_forbidden" | "video_target_not_found" | "video_version_conflict" | "video_egress_http_error" | "video_egress_private_ip" | "video_egress_timeout" | "video_egress_too_large" | "video_egress_too_many_redirects" | "video_egress_unavailable" | "video_hls_invalid" | "video_hls_depth_exceeded" | "video_hls_segment_count_exceeded" | "video_hls_duration_exceeded" | "video_hls_cross_origin_segment" | "video_hls_key_not_allowed" | "video_hls_map_not_allowed" | "video_hls_signed_uri" | "internal_error";
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
