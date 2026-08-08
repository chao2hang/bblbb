# BBLBB — 隐藏内容防泄漏扫漏报告（M16-SECURITY-06）

> 负责人：platform/application-security
> 扫漏结论：**PASS** —— 隐藏正文（`restricted_html` 原始标记）、私密资料、
> 凭据与签名 URL 未从下列任一渠道泄漏。证据为真实测试文件与函数。

## 渠道 × 证据矩阵

| 泄漏渠道 | 后端证据 | 前端/其他证据 |
|---|---|---|
| 帖子/回复详情 API | `backend/tests/authz_hidden.rs#hidden_read_error_maps_to_http` · `backend/tests/visibility_boundary.rs#paid_grant_unlocks_then_revocation_relocks` | `frontend/src/lib/testing/ssr/post-detail-ssr.test.ts` |
| 列表、excerpt、推荐 | `backend/tests/visibility_projection.rs#batch_canary_never_leaks_anywhere` · `backend/tests/visibility_projection.rs#batch_mixed_policies_each_item_projected_by_own_grant` | — |
| 板块/父链（breadcrumb） | `backend/tests/boards_no_leak.rs#visible_child_of_hidden_parent_reveals_nothing` · `backend/tests/boards_no_leak.rs#hidden_board_indistinguishable_from_missing` | — |
| 搜索索引与高亮 | `backend/tests/search/public.rs#public_search_projections_and_canary_never_leak` · `backend/tests/search/public.rs#unified_exclusion_rules_keep_non_public_out` | — |
| RSS/Atom/Feed | `backend/tests/feeds.rs`（feed 投影） | — |
| SEO（sitemap/OG/JSON-LD） | `backend/src/routes/feeds.rs`（sitemap/robots） | `frontend/src/lib/seo/meta.ts` + `meta.test.ts` |
| SSR HTML 与 hydration payload | 服务端投影单测（visibility_projection） | `frontend/src/lib/testing/ssr/privacy.test.ts`（SSR 不输出 restricted_html）、`nojs.test.ts` |
| 通知与邮件 | `backend/tests/mail_payload_safety.rs#mail_payload_with_plaintext_token_is_rejected` · `backend/tests/mail_payload_safety.rs#last_error_never_leaks_token` | — |
| 日志、tracing、错误响应、审计 metadata | `backend/tests/user_leak_sweep.rs#responses_and_audit_logs_never_contain_credentials` · `backend/tests/token_hygiene.rs#error_paths_and_log_diagnostics_are_token_free` · `backend/tests/readyz.rs` | `ops/scan-log-corpus.sh --test`（CLEAN） |
| 公共缓存/304/ETag | `backend/tests/visibility_projection.rs#different_personas_never_share_etag`（不同 persona 不共享 ETag → 缓存不串） | — |
| 附件下载 | `backend/tests/download/billing.rs#not_ready_attachment_never_leaks`（统一 404）· `backend/tests/download/http.rs#pending_attachment_download_is_not_found` | — |
| AI 外发（脱敏/同意） | `backend/tests/ai/gateway.rs#redactor_strips_emails_in_redacted_mode` · `backend/tests/ai/tasks.rs#execute_rechecks_consent_and_blocks_revoked` | — |
| 用户资料/公开投影 | `backend/tests/public_profile_leak.rs#public_user_never_leaks_sensitive_fields` · `backend/tests/public_profile_leak.rs#banned_and_pending_delete_users_get_degraded_projection` · `backend/tests/public_profile_leak.rs#deleted_user_public_lookup_returns_404` | `frontend/src/lib/testing/ssr/user-page-ssr.test.ts` |
| 数据库列（token 不落列） | `backend/tests/token_hygiene.rs#assert_table_columns_token_free` · `backend/tests/token_hygiene.rs#known_verify_token_absent_from_db_api_audit_outbox` | — |
| OIDC userinfo/claim | `backend/tests/oidc.rs`（scope/consent 控制） | — |
| 审计字段 allowlist | `backend/src/audit/mod.rs`（AUDIT_FIELD_ALLOWLIST + [REDACTED]）、`backend/tests/user_leak_sweep.rs#audit_logs_text` | — |

## 阻断性规则（已实施）

1. 未解锁正文**不是错误**：读路径 200 + 正文键缺失 + `access_summary`（fail-closed，
   `backend/tests/visibility_projection.rs#access_summary_always_present`）。
2. 未 ready / 隐藏附件统一 404（不泄漏存在性）。
3. 不同 persona 响应不共享 ETag/缓存（`Cache-Control: private` 按投影）。
4. 日志/审计/错误不记录 token、完整邮箱、隐藏正文、签名 URL（`sanitize` + 列级断言）。
5. 邮件 payload 只存 `user_id` 引用 + 安全模板参数。
6. AI 外发先脱敏，完整内容需独立同意，撤回即停止（`ai_consent_required`）。

## 验证命令

```sh
cd backend && cargo test --all-features --test visibility_projection --test authz_hidden \
  --test boards_no_leak --test public_profile_leak --test user_leak_sweep \
  --test token_hygiene --test mail_payload_safety --test search_public
cd frontend && npm run test        # 含 ssr/privacy、user-page-ssr 泄漏断言
bash ops/scan-log-corpus.sh --test # 日志脱敏扫描 CLEAN
```
