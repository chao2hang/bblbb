# BBLBB — 状态机合法/非法迁移测试矩阵（M16-HARNESS-03）

> 每个状态机至少有一个合法迁移行为测试和一个非法迁移测试（拒绝且不改变状态），
> 并对稳定错误码做断言。证据引用 `backend/tests/` 与 `backend/src/` 中真实存在的
> 测试函数（`file.rs#function`），由 `ruby scripts/check-state-machine-matrix.rb`
> 机械校验引用存在性。状态定义见 `docs/STATE-MACHINES.md`。

| 状态机 | 合法迁移测试（证据） | 非法迁移测试（证据） | 错误码断言 |
|---|---|---|---|
| User（pending_verification→active→restricted→banned→pending_delete→anonymized） | `backend/tests/deletion_lifecycle.rs#due_execution_anonymizes_and_preserves_audit` · `backend/tests/deletion_lifecycle.rs#cancel_restores_active_cancels_job_and_audits` | 封禁会话实时撤销、注销不绕过 sanction：`backend/tests/moderation/sanctions.rs#ban_revokes_sessions_and_marks_banned` | `forbidden` / `not_found` |
| Post（draft→pending_review→published→hidden→deleted；closed_at 锁帖） | `backend/tests/posts_publish.rs#immediate_publish_writes_all_artifacts` · `backend/tests/posts_publish.rs#scheduled_publish_via_job` | 未知板块/越权/非法载荷拒绝：`backend/tests/posts_publish.rs#immediate_publish_rejects_unknown_board` · `backend/tests/posts_publish.rs#handle_publish_job_rejects_invalid_payload` | `not_found` / `forbidden` |
| Post 版本（乐观锁） | `backend/tests/posts_edit.rs#owner_edit_creates_immutable_revision` | 版本冲突 409：`backend/tests/posts_edit.rs#edit_version_conflict_returns_409` | `version_conflict` |
| Comment（published→hidden→deleted） | `backend/tests/moderation/cases.rs#content_action_hide_removes_from_public_and_audits` · `backend/tests/moderation/cases.rs#content_action_delete_and_restore` | 隐藏帖不可回复/越权删除：`backend/tests/comments_routes.rs#create_comment_on_hidden_post_is_allowed` · `backend/tests/posts_govern.rs#move_post_validates_target_board` | `not_found` / `forbidden` |
| Board（visibility/posting_mode 稳定枚举；is_active/deleted_at） | `backend/tests/boards_visibility.rs#list_filter_respects_visibility` · `backend/tests/boards_admin.rs#deactivated_board_leaves_public_list` | readonly/closed 禁发帖、停用板离开公开列表：`backend/tests/boards_visibility.rs#hidden_board_requires_management` | `forbidden` / `not_found` |
| Authorization（assignment 生效/过期） | `backend/tests/authz_roles.rs#expired_assignment_is_excluded` | 未来授权未生效：`backend/tests/authz_roles.rs#future_grant_is_not_effective_and_rows_are_retained` · `backend/tests/authz_object.rs#expired_assignment_combined_with_object_rules` | `forbidden` |
| Report/Case（open→triaged→investigating→resolved/rejected→reopened；withdrawn 终态） | `backend/tests/moderation/cases.rs#case_state_machine_transitions`（合法边） | 非法边拒绝：`backend/tests/moderation/cases.rs#case_state_machine_transitions`（含 `!` 非法边断言） | `not_found` / `forbidden` / `version_conflict` |
| Appeal（submitted→reviewing→upheld/partially_upheld/rejected；withdrawn 终态） | `backend/tests/moderation/appeals.rs#decide_reject_and_partial_are_append_only` · `backend/tests/moderation/appeals.rs#decide_uphold_revokes_sanction_and_restores_ban` | 并发决策版本冲突：`backend/tests/moderation/appeals.rs#concurrent_decision_stale_version` | `version_conflict` / `forbidden` |
| Sanction（scheduled→active→expired；revoked 只追加） | `backend/tests/moderation/sanctions.rs#effective_sanctions_realtime_boundaries`（半开边界 starts_at<=now<ends_at） | 撤销只追加 reversal 不改历史：`backend/tests/moderation/sanctions.rs#revoke_appends_reversal_without_mutating_original` | `conflict` / `forbidden` |
| Attachment（pending→processing→ready；quarantined→deleted） | `backend/tests/storage/upload.rs#complete_happy_path_readies_attachment_with_hash_audit_outbox` · `backend/tests/storage/upload.rs#complete_quarantines_on_head_size_mismatch_and_rolls_back_reserved` | 未 ready 附件不泄漏（统一 404）：`backend/tests/download/billing.rs#not_ready_attachment_never_leaks` | `not_found` / `storage_state_error` / `quota_exceeded` |
| Download Authorization（active→expired/revoked） | `backend/tests/download/billing.rs#sign_url_after_authorization_does_not_recharge` · `backend/tests/download/billing.rs#free_download_creates_authorization_without_charge` | 停用策略拒绝新下载：`backend/tests/download/billing.rs#disabled_policy_rejects_download` | `download_url_unavailable` / `not_found` |
| Checkout Intent（created→awaiting_confirmation→consumed；expired/cancelled 终态） | `backend/tests/marketplace/checkout.rs#intent_binds_user_client_offer_and_is_one_shot_with_ttl` · `backend/tests/marketplace/checkout.rs#concurrent_confirms_exactly_one_succeeds` | 过期/消费后拒绝：`backend/tests/marketplace/checkout.rs#intent_expires_after_ttl` · `backend/tests/marketplace/checkout.rs#deny_decision_cancels_intent_without_charging` | `checkout_intent_expired` / `checkout_intent_consumed` / `idempotency_conflict` |
| Purchase（succeeded→partially_refunded→refunded；disputed 回流） | `backend/tests/marketplace/refund.rs#settlement_and_refund_preserve_double_sided_identity` | 不提供回滚成功购买、累计超额拒绝：`backend/tests/marketplace/refund.rs#refund_is_reversal_only_and_respects_cumulative_cap` · `backend/tests/marketplace/refund.rs#refund_identity_sum_is_zero_and_immutable_history_preserved` | `refund_exceeds_purchase` / `refund_not_allowed` |
| Refund（requested→succeeded；requested→rejected/failed；failed→requested） | `backend/tests/marketplace/refund.rs#admin_refund_works_without_client_scope` | 跨 Client 退款拒绝：`backend/tests/marketplace/refund.rs#client_cannot_refund_another_clients_purchase` | `refund_not_allowed` / `forbidden` |
| AI Task（queued→running→succeeded；cancelled/failed/dead） | `backend/tests/ai/tasks.rs#execute_success_marks_succeeded` · `backend/tests/ai/tasks.rs#execute_retries_5xx_then_dead_after_max_attempts` · `backend/tests/ai/tasks.rs#cancel_moves_queued_to_cancelled_and_is_idempotent` | 4xx 直接 dead、consent 撤销阻断：`backend/tests/ai/tasks.rs#execute_4xx_marks_dead_immediately` · `backend/tests/ai/tasks.rs#execute_rechecks_consent_and_blocks_revoked` | `ai_suggestion_stale` / `ai_budget_exceeded` / `ai_consent_required` |
| AI Suggestion（pending→accepted/rejected/stale） | `backend/tests/ai/tasks.rs#task_state_is_scoped_to_owner` | revision 变化→stale 不覆盖新版本：`backend/tests/ai/tasks.rs#execute_rechecks_consent_and_blocks_revoked` | `ai_suggestion_stale` |
| Video Embed（pending→ready；blocked/error→pending/removed） | `backend/tests/video.rs#resolve_create_get_delete_lifecycle` · `backend/tests/video.rs#refresh_mime_spoof_sets_error_and_keeps_external_link` | 策略收紧后降级：`backend/tests/video.rs#policy_change_rechecks_references_and_degrades` | `video_mime_mismatch` / `video_takedown` / `video_policy_changed` |
| Shop Product（draft→pending_review→published→disabled→retired） | `backend/tests/shop/core.rs#publish_disable_and_admin_list` | 等级门槛/销售窗口拒绝：`backend/tests/shop/core.rs#level_gate_and_sale_window_are_checked` | `product_unavailable` / `forbidden` |
| Shop Order（created→succeeded→partially_refunded→refunded） | `backend/tests/shop/core.rs#buy_product_charges_and_grants_entitlement` · `backend/tests/shop/core.rs#concurrent_buys_do_not_oversell` | 退款只撤销权益不删历史：`backend/tests/shop/core.rs#refund_respects_policy_and_revokes_entitlement` | `refund_not_allowed` / `insufficient_funds` |
| Entitlement（owned→equipped→owned；expired/revoked/consumed） | `backend/tests/shop/core.rs#entitlement_expiry_and_slot_exclusivity` | 槽位互斥/过期：`backend/tests/shop/core.rs#entitlement_expiry_and_slot_exclusivity`（非法路径断言） | `presentation_slot_conflict` / `entitlement_not_usable` |
| Activity Claim（eligible→claimed；rejected/expired；reversed） | `backend/tests/economy/activity.rs#checkin_first_claim_grants_then_replay_dedupes` · `backend/tests/economy/activity.rs#concurrent_visits_claim_once` · `backend/tests/economy/activity.rs#checkin_activity_day_follows_user_timezone_boundary` | 封禁/未验证用户拒绝、每日上限：`backend/tests/economy/activity.rs#banned_and_unverified_users_cannot_claim` · `backend/tests/economy/activity.rs#content_reward_respects_daily_limit` | `activity_already_claimed` / `activity_not_eligible` |
| Job（queued→running→retry_wait→succeeded；cancelled/dead；dead→queued 人工重放） | `backend/src/jobs/mod.rs#allowed_transition`（合法边表）· `backend/tests/jobs_retry.rs#complete_job_marks_succeeded_and_clears_lock` | 非法迁移拒绝且不改变状态：`backend/src/jobs/mod.rs#illegal_transitions_are_rejected_and_do_not_mutate` | `conflict` / `not_found` |
| Outbox Event（pending→processing→sent/failed） | `backend/tests/outbox.rs#outbox_event_persists_on_commit` · `backend/tests/outbox_consumer.rs#consume_in_tx_wins_once_per_consumer` | 崩溃回滚重投恰一次：`backend/tests/outbox_consumer.rs#crash_rollback_replays_side_effect_exactly_once` | `internal_error`（事务回滚无响应泄漏） |
| Webhook Delivery（pending→delivering→delivered；retry_wait→dead） | `backend/tests/marketplace/refund.rs#webhook_hmac_time_window_replay_and_delivery_records` | 非 2xx 退避到 dead-letter：`backend/tests/marketplace/refund.rs#webhook_non_2xx_backs_off_and_dead_letters` | `webhook_invalid_signature` |

## 错误码断言覆盖

合法/非法迁移的错误码由路由层稳定 Problem code 断言（M16-HARNESS-04 四方一致：
docs/ERROR-CODES.md ↔ OpenAPI Problem.code enum ↔ backend 实现 ↔ frontend 映射）。
`ruby scripts/check-code-fixtures.rb` 对全部稳定码逐一校验 Fixture 与前端映射。

## 验证命令

```sh
ruby scripts/check-state-machine-matrix.rb   # 矩阵引用的测试文件/函数真实存在
ruby scripts/check-code-fixtures.rb          # 每个稳定码有 Fixture + 前端映射
ruby scripts/check-state-enums.rb            # 枚举与 OpenAPI/前端一致
ruby scripts/check-roadmap.rb                # 路线图一致性
```
