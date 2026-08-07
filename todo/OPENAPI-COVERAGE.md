# OpenAPI operation 实现覆盖登记

> 由 `ruby scripts/sync-operation-coverage.rb` 从 `openapi/openapi.yaml` 同步。
> 本表不是重复 TODO；每一行关联唯一工作包，完成状态以 JSON 中的实现、测试和证据字段为准。

## 汇总

- 契约操作：**184**
- 唯一 operationId：**184**
- 实现状态：`baseline_only` 3；`implemented` 58；`in_progress` 7；`not_started` 116
- 里程碑分配：`M0` 1；`M2` 18；`M3` 25；`M4` 17；`M5` 13；`M6` 19；`M7` 28；`M8` 3；`M9` 16；`M10` 10；`M11` 14；`M12` 12；`M13` 8

## 状态规则

- `not_started`：只有冻结契约，尚无实现证据。
- `baseline_only`：已有骨架实现/测试，但尚未通过对应 v1 工作包全部门槛。
- `in_progress`：已指定 owner，handler 或测试尚不完整。
- `implemented`：handler 与领域测试完成，但跨库/E2E/安全证据尚未完整。
- `verified`：契约、权限、三数据库、客户端和专项测试证据全部完整。
- `blocked`：必须填写 blocker、owner、复查日期和解除条件（写在 evidence 字段或关联任务中）。

## 逐项登记

| operationId | Method | Path | Tag | Milestone / work package | Priority | Status | Owner |
|---|---:|---|---|---|---:|---|---|
| `getHealth` | `GET` | `/healthz` | Health | `M0` / `M00-BACKEND` | `P0` | `baseline_only` | `platform` |
| `getCsrfToken` | `GET` | `/api/v1/auth/csrf` | Auth | `M2` / `M02-SESSION` | `P0` | `implemented` | `backend-auth` |
| `login` | `POST` | `/api/v1/auth/login` | Auth | `M2` / `M02-SESSION` | `P0` | `implemented` | `backend-auth` |
| `loginMfa` | `POST` | `/api/v1/auth/login/mfa` | Auth | `M2` / `M02-IDENTITY` | `P0` | `implemented` | `backend-auth` |
| `mfaDisable` | `DELETE` | `/api/v1/auth/mfa` | Auth | `M2` / `M02-IDENTITY` | `P0` | `implemented` | `backend-auth` |
| `mfaConfirm` | `POST` | `/api/v1/auth/mfa/confirm` | Auth | `M2` / `M02-IDENTITY` | `P0` | `implemented` | `backend-auth` |
| `mfaEnroll` | `POST` | `/api/v1/auth/mfa/enroll` | Auth | `M2` / `M02-IDENTITY` | `P0` | `implemented` | `backend-auth` |
| `mfaCancel` | `DELETE` | `/api/v1/auth/mfa/enrollment` | Auth | `M2` / `M02-IDENTITY` | `P0` | `implemented` | `backend-auth` |
| `mfaRecoveryCodes` | `POST` | `/api/v1/auth/mfa/recovery-codes` | Auth | `M2` / `M02-IDENTITY` | `P0` | `implemented` | `backend-auth` |
| `requestPasswordReset` | `POST` | `/api/v1/auth/password-reset` | Auth | `M2` / `M02-IDENTITY` | `P0` | `implemented` | `backend-auth` |
| `confirmPasswordReset` | `POST` | `/api/v1/auth/password-reset/confirm` | Auth | `M2` / `M02-IDENTITY` | `P0` | `implemented` | `backend-auth` |
| `reAuth` | `POST` | `/api/v1/auth/re-auth` | Auth | `M2` / `M02-IDENTITY` | `P0` | `implemented` | `backend-auth` |
| `register` | `POST` | `/api/v1/auth/register` | Auth | `M2` / `M02-IDENTITY` | `P0` | `implemented` | `backend-auth` |
| `resendVerification` | `POST` | `/api/v1/auth/resend-verification` | Auth | `M2` / `M02-IDENTITY` | `P0` | `implemented` | `backend-auth` |
| `logout` | `DELETE` | `/api/v1/auth/session` | Auth | `M2` / `M02-SESSION` | `P0` | `implemented` | `backend-auth` |
| `logoutAll` | `DELETE` | `/api/v1/auth/sessions` | Auth | `M2` / `M02-IDENTITY` | `P0` | `implemented` | `backend-auth` |
| `listSessions` | `GET` | `/api/v1/auth/sessions` | Auth | `M2` / `M02-IDENTITY` | `P0` | `implemented` | `backend-auth` |
| `revokeSession` | `DELETE` | `/api/v1/auth/sessions/{id}` | Auth | `M2` / `M02-IDENTITY` | `P0` | `implemented` | `backend-auth` |
| `verifyEmail` | `POST` | `/api/v1/auth/verify-email` | Auth | `M2` / `M02-IDENTITY` | `P0` | `implemented` | `backend-auth` |
| `listAdminBoards` | `GET` | `/api/v1/admin/boards` | Boards | `M3` / `M03-BOARDS` | `P1` | `in_progress` | `backend-content` |
| `createAdminBoard` | `POST` | `/api/v1/admin/boards` | Boards | `M3` / `M03-BOARDS` | `P1` | `implemented` | `backend-content` |
| `getAdminBoard` | `GET` | `/api/v1/admin/boards/{id}` | Boards | `M3` / `M03-BOARDS` | `P1` | `in_progress` | `backend-content` |
| `updateAdminBoard` | `PATCH` | `/api/v1/admin/boards/{id}` | Boards | `M3` / `M03-BOARDS` | `P1` | `implemented` | `backend-content` |
| `listBoards` | `GET` | `/api/v1/boards` | Boards | `M3` / `M03-BOARDS` | `P1` | `implemented` | `backend-content` |
| `getBoard` | `GET` | `/api/v1/boards/{slug}` | Boards | `M3` / `M03-BOARDS` | `P1` | `implemented` | `backend-content` |
| `listTags` | `GET` | `/api/v1/tags` | Boards | `M3` / `M03-BOARDS` | `P1` | `implemented` | `backend-content` |
| `listAdminRoles` | `GET` | `/api/v1/admin/roles` | Roles | `M3` / `M03-AUTHZ` | `P0` | `in_progress` | `security-backend` |
| `createAdminRole` | `POST` | `/api/v1/admin/roles` | Roles | `M3` / `M03-AUTHZ` | `P0` | `in_progress` | `security-backend` |
| `getAdminRole` | `GET` | `/api/v1/admin/roles/{id}` | Roles | `M3` / `M03-AUTHZ` | `P0` | `in_progress` | `security-backend` |
| `updateAdminRole` | `PATCH` | `/api/v1/admin/roles/{id}` | Roles | `M3` / `M03-AUTHZ` | `P0` | `in_progress` | `security-backend` |
| `listAdminTags` | `GET` | `/api/v1/admin/tags` | Tags | `M3` / `M03-BOARDS` | `P1` | `implemented` | `backend-content` |
| `createAdminTag` | `POST` | `/api/v1/admin/tags` | Tags | `M3` / `M03-BOARDS` | `P1` | `implemented` | `backend-content` |
| `getAdminTag` | `GET` | `/api/v1/admin/tags/{id}` | Tags | `M3` / `M03-BOARDS` | `P1` | `in_progress` | `backend-content` |
| `updateAdminTag` | `PATCH` | `/api/v1/admin/tags/{id}` | Tags | `M3` / `M03-BOARDS` | `P1` | `implemented` | `backend-content` |
| `listAdminUsers` | `GET` | `/api/v1/admin/users` | Users | `M3` / `M03-PROFILE` | `P1` | `not_started` | `unassigned` |
| `createAdminUser` | `POST` | `/api/v1/admin/users` | Users | `M3` / `M03-PROFILE` | `P1` | `not_started` | `unassigned` |
| `getAdminUser` | `GET` | `/api/v1/admin/users/{id}` | Users | `M3` / `M03-PROFILE` | `P1` | `not_started` | `unassigned` |
| `updateAdminUser` | `PATCH` | `/api/v1/admin/users/{id}` | Users | `M3` / `M03-PROFILE` | `P1` | `not_started` | `unassigned` |
| `getMe` | `GET` | `/api/v1/me` | Users | `M3` / `M03-PROFILE` | `P1` | `implemented` | `security-backend` |
| `updateMe` | `PATCH` | `/api/v1/me` | Users | `M3` / `M03-PROFILE` | `P1` | `implemented` | `security-backend` |
| `delete_me_profile_cover` | `DELETE` | `/api/v1/me/profile-cover` | Users | `M3` / `M03-PROFILE` | `P1` | `not_started` | `unassigned` |
| `post_me_profile_cover` | `POST` | `/api/v1/me/profile-cover` | Users | `M3` / `M03-PROFILE` | `P1` | `not_started` | `unassigned` |
| `get_users_user_id_profile_cover` | `GET` | `/api/v1/users/{user_id}/profile-cover` | Users | `M3` / `M03-PROFILE` | `P1` | `not_started` | `unassigned` |
| `getPublicUser` | `GET` | `/api/v1/users/{username}` | Users | `M3` / `M03-PROFILE` | `P1` | `implemented` | `security-backend` |
| `delete_comments_id_` | `DELETE` | `/api/v1/comments/{id}` | Comments | `M4` / `M04-COMMENTS` | `P1` | `implemented` | `backend-content` |
| `patch_comments_id_` | `PATCH` | `/api/v1/comments/{id}` | Comments | `M4` / `M04-COMMENTS` | `P1` | `implemented` | `backend-content` |
| `listComments` | `GET` | `/api/v1/posts/{postId}/comments` | Comments | `M4` / `M04-COMMENTS` | `P1` | `implemented` | `backend-content` |
| `createComment` | `POST` | `/api/v1/posts/{postId}/comments` | Comments | `M4` / `M04-COMMENTS` | `P1` | `implemented` | `backend-content` |
| `listDrafts` | `GET` | `/api/v1/drafts` | Drafts | `M4` / `M04-POSTS` | `P1` | `implemented` | `backend-content` |
| `createDraft` | `POST` | `/api/v1/drafts` | Drafts | `M4` / `M04-POSTS` | `P1` | `implemented` | `backend-content` |
| `deleteDraft` | `DELETE` | `/api/v1/drafts/{id}` | Drafts | `M4` / `M04-POSTS` | `P1` | `implemented` | `backend-content` |
| `getDraft` | `GET` | `/api/v1/drafts/{id}` | Drafts | `M4` / `M04-POSTS` | `P1` | `implemented` | `backend-content` |
| `updateDraft` | `PATCH` | `/api/v1/drafts/{id}` | Drafts | `M4` / `M04-POSTS` | `P1` | `implemented` | `backend-content` |
| `previewDraft` | `POST` | `/api/v1/drafts/{id}/preview` | Drafts | `M4` / `M04-POSTS` | `P1` | `implemented` | `backend-content` |
| `listBoardPosts` | `GET` | `/api/v1/boards/{slug}/posts` | Posts | `M4` / `M04-POSTS` | `P1` | `implemented` | `backend-content` |
| `listPosts` | `GET` | `/api/v1/posts` | Posts | `M4` / `M04-POSTS` | `P1` | `implemented` | `backend-content` |
| `createPost` | `POST` | `/api/v1/posts` | Posts | `M4` / `M04-POSTS` | `P1` | `implemented` | `backend-content` |
| `getPost` | `GET` | `/api/v1/posts/{postId}` | Posts | `M4` / `M04-POSTS` | `P1` | `implemented` | `backend-content` |
| `updatePost` | `PATCH` | `/api/v1/posts/{postId}` | Posts | `M4` / `M04-POSTS` | `P1` | `implemented` | `backend-content` |
| `listPostRevisions` | `GET` | `/api/v1/posts/{id}/revisions` | Revisions | `M4` / `M04-POSTS` | `P1` | `implemented` | `backend-content` |
| `getPostRevision` | `GET` | `/api/v1/posts/{id}/revisions/{revisionId}` | Revisions | `M4` / `M04-POSTS` | `P1` | `implemented` | `backend-content` |
| `post_admin_moderation_sanctions` | `POST` | `/api/v1/admin/moderation/sanctions` | Admin | `M5` / `M05-SANCTIONS` | `P0` | `not_started` | `unassigned` |
| `listModerationAppeals` | `GET` | `/api/v1/admin/moderation/appeals` | Moderation | `M5` / `M05-APPEALS` | `P1` | `implemented` | `backend-moderation` |
| `getModerationAppeal` | `GET` | `/api/v1/admin/moderation/appeals/{id}` | Moderation | `M5` / `M05-APPEALS` | `P1` | `implemented` | `backend-moderation` |
| `decideModerationAppeal` | `PATCH` | `/api/v1/admin/moderation/appeals/{id}` | Moderation | `M5` / `M05-APPEALS` | `P1` | `implemented` | `backend-moderation` |
| `listModerationCases` | `GET` | `/api/v1/admin/moderation/cases` | Moderation | `M5` / `M05-CASES` | `P0` | `implemented` | `backend-moderation` |
| `getModerationCase` | `GET` | `/api/v1/admin/moderation/cases/{id}` | Moderation | `M5` / `M05-CASES` | `P0` | `implemented` | `backend-moderation` |
| `updateModerationCase` | `PATCH` | `/api/v1/admin/moderation/cases/{id}` | Moderation | `M5` / `M05-CASES` | `P0` | `implemented` | `backend-moderation` |
| `listOwnAppeals` | `GET` | `/api/v1/appeals` | Moderation | `M5` / `M05-APPEALS` | `P1` | `implemented` | `backend-moderation` |
| `createAppeal` | `POST` | `/api/v1/appeals` | Moderation | `M5` / `M05-APPEALS` | `P1` | `implemented` | `backend-moderation` |
| `getOwnAppeal` | `GET` | `/api/v1/appeals/{id}` | Moderation | `M5` / `M05-APPEALS` | `P1` | `implemented` | `backend-moderation` |
| `post_reports` | `POST` | `/api/v1/reports` | Moderation | `M5` / `M05-CASES` | `P0` | `implemented` | `backend-moderation` |
| `get_notifications` | `GET` | `/api/v1/notifications` | Notifications | `M5` / `M05-NOTIFY` | `P1` | `implemented` | `backend-notifications` |
| `post_notifications_id_read` | `POST` | `/api/v1/notifications/{id}/read` | Notifications | `M5` / `M05-NOTIFY` | `P1` | `implemented` | `backend-notifications` |
| `get_admin_levels_id_attachment_quota` | `GET` | `/api/v1/admin/levels/{id}/attachment-quota` | Admin | `M6` / `M06-QUOTA` | `P0` | `not_started` | `unassigned` |
| `patch_admin_levels_id_attachment_quota` | `PATCH` | `/api/v1/admin/levels/{id}/attachment-quota` | Admin | `M6` / `M06-QUOTA` | `P0` | `not_started` | `unassigned` |
| `get_admin_storage_config` | `GET` | `/api/v1/admin/storage/config` | Admin | `M6` / `M06-QUOTA` | `P0` | `not_started` | `unassigned` |
| `patch_admin_storage_config` | `PATCH` | `/api/v1/admin/storage/config` | Admin | `M6` / `M06-QUOTA` | `P0` | `not_started` | `unassigned` |
| `post_admin_storage_test` | `POST` | `/api/v1/admin/storage/test` | Admin | `M6` / `M06-QUOTA` | `P0` | `not_started` | `unassigned` |
| `createAttachment` | `POST` | `/api/v1/attachments` | Attachments | `M6` / `M06-UPLOAD` | `P0` | `not_started` | `unassigned` |
| `delete_attachments_id_` | `DELETE` | `/api/v1/attachments/{id}` | Attachments | `M6` / `M06-UPLOAD` | `P0` | `not_started` | `unassigned` |
| `get_attachments_id_` | `GET` | `/api/v1/attachments/{id}` | Attachments | `M6` / `M06-UPLOAD` | `P0` | `not_started` | `unassigned` |
| `post_attachments_id_complete` | `POST` | `/api/v1/attachments/{id}/complete` | Attachments | `M6` / `M06-UPLOAD` | `P0` | `not_started` | `unassigned` |
| `get_attachments_id_content` | `GET` | `/api/v1/attachments/{id}/content` | Attachments | `M6` / `M06-UPLOAD` | `P0` | `not_started` | `unassigned` |
| `post_attachments_id_download` | `POST` | `/api/v1/attachments/{id}/download` | Attachments | `M6` / `M06-DOWNLOAD` | `P0` | `not_started` | `unassigned` |
| `get_attachments_id_download_policy` | `GET` | `/api/v1/attachments/{id}/download-policy` | Attachments | `M6` / `M06-DOWNLOAD` | `P0` | `not_started` | `unassigned` |
| `get_download_authorizations_id_` | `GET` | `/api/v1/download-authorizations/{id}` | Attachments | `M6` / `M06-DOWNLOAD` | `P0` | `not_started` | `unassigned` |
| `post_download_authorizations_id_sign_url` | `POST` | `/api/v1/download-authorizations/{id}/sign-url` | Attachments | `M6` / `M06-DOWNLOAD` | `P0` | `not_started` | `unassigned` |
| `get_me_download_transactions` | `GET` | `/api/v1/me/download-transactions` | Attachments | `M6` / `M06-DOWNLOAD` | `P0` | `not_started` | `unassigned` |
| `getAttachmentDownloadPolicyAdmin` | `GET` | `/api/v1/admin/attachments/{id}/download-policy` | Download Billing | `M6` / `M06-DOWNLOAD` | `P0` | `not_started` | `unassigned` |
| `updateAttachmentDownloadPolicyAdmin` | `PATCH` | `/api/v1/admin/attachments/{id}/download-policy` | Download Billing | `M6` / `M06-DOWNLOAD` | `P0` | `not_started` | `unassigned` |
| `getDownloadBillingConfig` | `GET` | `/api/v1/admin/download-billing/config` | Download Billing | `M6` / `M06-DOWNLOAD` | `P0` | `not_started` | `unassigned` |
| `updateDownloadBillingConfig` | `PATCH` | `/api/v1/admin/download-billing/config` | Download Billing | `M6` / `M06-DOWNLOAD` | `P0` | `not_started` | `unassigned` |
| `get_activity_summary` | `GET` | `/api/v1/activity/summary` | Activity | `M7` / `M07-LEVELS` | `P1` | `not_started` | `unassigned` |
| `recordAuthenticatedVisit` | `POST` | `/api/v1/activity/visit` | Activity | `M7` / `M07-LEVELS` | `P1` | `not_started` | `unassigned` |
| `getAdminActivityConfig` | `GET` | `/api/v1/admin/activity/config` | Activity | `M7` / `M07-LEVELS` | `P1` | `not_started` | `unassigned` |
| `updateAdminActivityConfig` | `PATCH` | `/api/v1/admin/activity/config` | Activity | `M7` / `M07-LEVELS` | `P1` | `not_started` | `unassigned` |
| `listAdminActivityTasks` | `GET` | `/api/v1/admin/activity/tasks` | Activity | `M7` / `M07-LEVELS` | `P1` | `not_started` | `unassigned` |
| `createAdminActivityTask` | `POST` | `/api/v1/admin/activity/tasks` | Activity | `M7` / `M07-LEVELS` | `P1` | `not_started` | `unassigned` |
| `updateAdminActivityTask` | `PATCH` | `/api/v1/admin/activity/tasks/{id}` | Activity | `M7` / `M07-LEVELS` | `P1` | `not_started` | `unassigned` |
| `post_comments_id_reactions` | `POST` | `/api/v1/comments/{id}/reactions` | Activity | `M7` / `M07-LEVELS` | `P1` | `not_started` | `unassigned` |
| `delete_comments_id_reactions_reaction_` | `DELETE` | `/api/v1/comments/{id}/reactions/{reaction}` | Activity | `M7` / `M07-LEVELS` | `P1` | `not_started` | `unassigned` |
| `post_posts_id_reactions` | `POST` | `/api/v1/posts/{id}/reactions` | Activity | `M7` / `M07-LEVELS` | `P1` | `not_started` | `unassigned` |
| `delete_posts_id_reactions_reaction_` | `DELETE` | `/api/v1/posts/{id}/reactions/{reaction}` | Activity | `M7` / `M07-LEVELS` | `P1` | `not_started` | `unassigned` |
| `getAdminShopConfig` | `GET` | `/api/v1/admin/shop/config` | Shop | `M7` / `M07-SHOP` | `P1` | `not_started` | `unassigned` |
| `updateAdminShopConfig` | `PATCH` | `/api/v1/admin/shop/config` | Shop | `M7` / `M07-SHOP` | `P1` | `not_started` | `unassigned` |
| `listAdminShopOrders` | `GET` | `/api/v1/admin/shop/orders` | Shop | `M7` / `M07-SHOP` | `P1` | `not_started` | `unassigned` |
| `refundAdminShopOrder` | `POST` | `/api/v1/admin/shop/orders/{id}/refund` | Shop | `M7` / `M07-SHOP` | `P1` | `not_started` | `unassigned` |
| `listAdminShopProducts` | `GET` | `/api/v1/admin/shop/products` | Shop | `M7` / `M07-SHOP` | `P1` | `not_started` | `unassigned` |
| `createAdminShopProduct` | `POST` | `/api/v1/admin/shop/products` | Shop | `M7` / `M07-SHOP` | `P1` | `not_started` | `unassigned` |
| `updateAdminShopProduct` | `PATCH` | `/api/v1/admin/shop/products/{id}` | Shop | `M7` / `M07-SHOP` | `P1` | `not_started` | `unassigned` |
| `disableAdminShopProduct` | `POST` | `/api/v1/admin/shop/products/{id}/disable` | Shop | `M7` / `M07-SHOP` | `P1` | `not_started` | `unassigned` |
| `publishAdminShopProduct` | `POST` | `/api/v1/admin/shop/products/{id}/publish` | Shop | `M7` / `M07-SHOP` | `P1` | `not_started` | `unassigned` |
| `get_me_entitlements` | `GET` | `/api/v1/me/entitlements` | Shop | `M7` / `M07-SHOP` | `P1` | `not_started` | `unassigned` |
| `post_me_entitlements_id_equip` | `POST` | `/api/v1/me/entitlements/{id}/equip` | Shop | `M7` / `M07-SHOP` | `P1` | `not_started` | `unassigned` |
| `post_me_entitlements_id_unequip` | `POST` | `/api/v1/me/entitlements/{id}/unequip` | Shop | `M7` / `M07-SHOP` | `P1` | `not_started` | `unassigned` |
| `get_me_presentation` | `GET` | `/api/v1/me/presentation` | Shop | `M7` / `M07-SHOP` | `P1` | `not_started` | `unassigned` |
| `post_shop_orders` | `POST` | `/api/v1/shop/orders` | Shop | `M7` / `M07-SHOP` | `P1` | `not_started` | `unassigned` |
| `get_shop_orders_id_` | `GET` | `/api/v1/shop/orders/{id}` | Shop | `M7` / `M07-SHOP` | `P1` | `not_started` | `unassigned` |
| `get_shop_products` | `GET` | `/api/v1/shop/products` | Shop | `M7` / `M07-SHOP` | `P1` | `not_started` | `unassigned` |
| `get_shop_products_id_` | `GET` | `/api/v1/shop/products/{id}` | Shop | `M7` / `M07-SHOP` | `P1` | `not_started` | `unassigned` |
| `getPublicAtomFeed` | `GET` | `/api/v1/atom` | Feeds | `M8` / `M08-FEEDS` | `P1` | `not_started` | `unassigned` |
| `getPublicRssFeed` | `GET` | `/api/v1/rss` | Feeds | `M8` / `M08-FEEDS` | `P1` | `not_started` | `unassigned` |
| `searchPublicContent` | `GET` | `/api/v1/search` | Search | `M8` / `M08-INDEX` | `P1` | `not_started` | `unassigned` |
| `get_ai_capabilities` | `GET` | `/api/v1/ai/capabilities` | AI | `M9` / `M09-SUGGESTIONS` | `P1` | `not_started` | `unassigned` |
| `delete_ai_consent` | `DELETE` | `/api/v1/ai/consent` | AI | `M9` / `M09-SUGGESTIONS` | `P1` | `not_started` | `unassigned` |
| `post_ai_consent` | `POST` | `/api/v1/ai/consent` | AI | `M9` / `M09-SUGGESTIONS` | `P1` | `not_started` | `unassigned` |
| `post_ai_drafts_draft_id_format` | `POST` | `/api/v1/ai/drafts/{draft_id}/format` | AI | `M9` / `M09-SUGGESTIONS` | `P1` | `not_started` | `unassigned` |
| `post_ai_posts_post_id_moderation_suggestion` | `POST` | `/api/v1/ai/posts/{post_id}/moderation-suggestion` | AI | `M9` / `M09-SUGGESTIONS` | `P1` | `not_started` | `unassigned` |
| `post_ai_posts_post_id_seo_suggestion` | `POST` | `/api/v1/ai/posts/{post_id}/seo-suggestion` | AI | `M9` / `M09-SUGGESTIONS` | `P1` | `not_started` | `unassigned` |
| `get_ai_suggestions_id_` | `GET` | `/api/v1/ai/suggestions/{id}` | AI | `M9` / `M09-SUGGESTIONS` | `P1` | `not_started` | `unassigned` |
| `post_ai_suggestions_id_accept` | `POST` | `/api/v1/ai/suggestions/{id}/accept` | AI | `M9` / `M09-SUGGESTIONS` | `P1` | `not_started` | `unassigned` |
| `get_ai_tasks_id_` | `GET` | `/api/v1/ai/tasks/{id}` | AI | `M9` / `M09-SUGGESTIONS` | `P1` | `not_started` | `unassigned` |
| `post_ai_tasks_id_cancel` | `POST` | `/api/v1/ai/tasks/{id}/cancel` | AI | `M9` / `M09-SUGGESTIONS` | `P1` | `not_started` | `unassigned` |
| `get_admin_ai_config` | `GET` | `/api/v1/admin/ai/config` | Admin | `M9` / `M09-GATEWAY` | `P0` | `not_started` | `unassigned` |
| `patch_admin_ai_config` | `PATCH` | `/api/v1/admin/ai/config` | Admin | `M9` / `M09-GATEWAY` | `P0` | `not_started` | `unassigned` |
| `post_admin_ai_providers_test` | `POST` | `/api/v1/admin/ai/providers/test` | Admin | `M9` / `M09-GATEWAY` | `P0` | `not_started` | `unassigned` |
| `get_admin_ai_tasks` | `GET` | `/api/v1/admin/ai/tasks` | Admin | `M9` / `M09-GATEWAY` | `P0` | `not_started` | `unassigned` |
| `post_admin_ai_tasks_id_cancel` | `POST` | `/api/v1/admin/ai/tasks/{id}/cancel` | Admin | `M9` / `M09-GATEWAY` | `P0` | `not_started` | `unassigned` |
| `post_admin_ai_tasks_id_retry` | `POST` | `/api/v1/admin/ai/tasks/{id}/retry` | Admin | `M9` / `M09-GATEWAY` | `P0` | `not_started` | `unassigned` |
| `get_admin_video_policies` | `GET` | `/api/v1/admin/video/policies` | Admin | `M10` / `M10-VIDEO` | `P0` | `not_started` | `unassigned` |
| `post_admin_video_policies_test` | `POST` | `/api/v1/admin/video/policies/test` | Admin | `M10` / `M10-VIDEO` | `P0` | `not_started` | `unassigned` |
| `get_admin_video_policies_provider_` | `GET` | `/api/v1/admin/video/policies/{provider}` | Admin | `M10` / `M10-VIDEO` | `P0` | `not_started` | `unassigned` |
| `patch_admin_video_policies_provider_` | `PATCH` | `/api/v1/admin/video/policies/{provider}` | Admin | `M10` / `M10-VIDEO` | `P0` | `not_started` | `unassigned` |
| `post_video_embeds` | `POST` | `/api/v1/video-embeds` | Video | `M10` / `M10-VIDEO` | `P0` | `not_started` | `unassigned` |
| `post_video_embeds_resolve` | `POST` | `/api/v1/video-embeds/resolve` | Video | `M10` / `M10-VIDEO` | `P0` | `not_started` | `unassigned` |
| `delete_video_embeds_id_` | `DELETE` | `/api/v1/video-embeds/{id}` | Video | `M10` / `M10-VIDEO` | `P0` | `not_started` | `unassigned` |
| `get_video_embeds_id_` | `GET` | `/api/v1/video-embeds/{id}` | Video | `M10` / `M10-VIDEO` | `P0` | `not_started` | `unassigned` |
| `patch_video_embeds_id_` | `PATCH` | `/api/v1/video-embeds/{id}` | Video | `M10` / `M10-VIDEO` | `P0` | `not_started` | `unassigned` |
| `post_video_embeds_id_refresh` | `POST` | `/api/v1/video-embeds/{id}/refresh` | Video | `M10` / `M10-VIDEO` | `P0` | `not_started` | `unassigned` |
| `get_well_known_openid_configuration` | `GET` | `/.well-known/openid-configuration` | OAuth | `M11` / `M11-PROTOCOL` | `P0` | `not_started` | `unassigned` |
| `get_oauth_interactions_id_` | `GET` | `/api/v1/oauth/interactions/{id}` | OAuth | `M11` / `M11-PROTOCOL` | `P0` | `not_started` | `unassigned` |
| `post_oauth_interactions_id_decision` | `POST` | `/api/v1/oauth/interactions/{id}/decision` | OAuth | `M11` / `M11-PROTOCOL` | `P0` | `not_started` | `unassigned` |
| `get_oauth_authorize` | `GET` | `/oauth/authorize` | OAuth | `M11` / `M11-PROTOCOL` | `P0` | `not_started` | `unassigned` |
| `get_oauth_jwks_json` | `GET` | `/oauth/jwks.json` | OAuth | `M11` / `M11-PROTOCOL` | `P0` | `not_started` | `unassigned` |
| `get_oauth_logout` | `GET` | `/oauth/logout` | OAuth | `M11` / `M11-PROTOCOL` | `P0` | `not_started` | `unassigned` |
| `post_oauth_logout` | `POST` | `/oauth/logout` | OAuth | `M11` / `M11-PROTOCOL` | `P0` | `not_started` | `unassigned` |
| `post_oauth_revoke` | `POST` | `/oauth/revoke` | OAuth | `M11` / `M11-PROTOCOL` | `P0` | `not_started` | `unassigned` |
| `post_oauth_token` | `POST` | `/oauth/token` | OAuth | `M11` / `M11-PROTOCOL` | `P0` | `not_started` | `unassigned` |
| `get_oauth_userinfo` | `GET` | `/oauth/userinfo` | OAuth | `M11` / `M11-PROTOCOL` | `P0` | `not_started` | `unassigned` |
| `listAdminOAuthClients` | `GET` | `/api/v1/admin/oauth-clients` | OAuth Clients | `M11` / `M11-CONSENT` | `P0` | `not_started` | `unassigned` |
| `createAdminOAuthClient` | `POST` | `/api/v1/admin/oauth-clients` | OAuth Clients | `M11` / `M11-CONSENT` | `P0` | `not_started` | `unassigned` |
| `getAdminOAuthClient` | `GET` | `/api/v1/admin/oauth-clients/{id}` | OAuth Clients | `M11` / `M11-CONSENT` | `P0` | `not_started` | `unassigned` |
| `updateAdminOAuthClient` | `PATCH` | `/api/v1/admin/oauth-clients/{id}` | OAuth Clients | `M11` / `M11-CONSENT` | `P0` | `not_started` | `unassigned` |
| `get_admin_marketplace_clients` | `GET` | `/api/v1/admin/marketplace/clients` | Admin | `M12` / `M12-CLIENTS` | `P0` | `not_started` | `unassigned` |
| `patch_admin_marketplace_clients_id_` | `PATCH` | `/api/v1/admin/marketplace/clients/{id}` | Admin | `M12` / `M12-CLIENTS` | `P0` | `not_started` | `unassigned` |
| `post_admin_marketplace_clients_id_rotate_webhook_secret` | `POST` | `/api/v1/admin/marketplace/clients/{id}/rotate-webhook-secret` | Admin | `M12` / `M12-CLIENTS` | `P0` | `not_started` | `unassigned` |
| `get_admin_marketplace_transactions` | `GET` | `/api/v1/admin/marketplace/transactions` | Admin | `M12` / `M12-CLIENTS` | `P0` | `not_started` | `unassigned` |
| `post_marketplace_checkout_intents` | `POST` | `/api/v1/marketplace/checkout-intents` | Marketplace | `M12` / `M12-CHECKOUT` | `P0` | `not_started` | `unassigned` |
| `post_marketplace_checkout_intents_id_confirm` | `POST` | `/api/v1/marketplace/checkout-intents/{id}/confirm` | Marketplace | `M12` / `M12-CHECKOUT` | `P0` | `not_started` | `unassigned` |
| `post_marketplace_offers` | `POST` | `/api/v1/marketplace/offers` | Marketplace | `M12` / `M12-CHECKOUT` | `P0` | `not_started` | `unassigned` |
| `get_marketplace_offers_id_` | `GET` | `/api/v1/marketplace/offers/{id}` | Marketplace | `M12` / `M12-CHECKOUT` | `P0` | `not_started` | `unassigned` |
| `patch_marketplace_offers_id_` | `PATCH` | `/api/v1/marketplace/offers/{id}` | Marketplace | `M12` / `M12-CHECKOUT` | `P0` | `not_started` | `unassigned` |
| `get_marketplace_purchases` | `GET` | `/api/v1/marketplace/purchases` | Marketplace | `M12` / `M12-CHECKOUT` | `P0` | `not_started` | `unassigned` |
| `get_marketplace_purchases_id_` | `GET` | `/api/v1/marketplace/purchases/{id}` | Marketplace | `M12` / `M12-CHECKOUT` | `P0` | `not_started` | `unassigned` |
| `post_marketplace_purchases_id_refund` | `POST` | `/api/v1/marketplace/purchases/{id}/refund` | Marketplace | `M12` / `M12-CHECKOUT` | `P0` | `not_started` | `unassigned` |
| `get_admin_themes` | `GET` | `/api/v1/admin/themes` | Admin | `M13` / `M13-THEME` | `P1` | `not_started` | `unassigned` |
| `post_admin_themes_data_packages` | `POST` | `/api/v1/admin/themes/data-packages` | Admin | `M13` / `M13-THEME` | `P1` | `not_started` | `unassigned` |
| `put_admin_themes_default` | `PUT` | `/api/v1/admin/themes/default` | Admin | `M13` / `M13-THEME` | `P1` | `not_started` | `unassigned` |
| `delete_admin_themes_name_` | `DELETE` | `/api/v1/admin/themes/{name}` | Admin | `M13` / `M13-THEME` | `P1` | `not_started` | `unassigned` |
| `patch_admin_themes_name_settings` | `PATCH` | `/api/v1/admin/themes/{name}/settings` | Admin | `M13` / `M13-THEME` | `P1` | `not_started` | `unassigned` |
| `get_me_preferences_theme` | `GET` | `/api/v1/me/preferences/theme` | Themes | `M13` / `M13-THEME` | `P1` | `baseline_only` | `backend-auth` |
| `put_me_preferences_theme` | `PUT` | `/api/v1/me/preferences/theme` | Themes | `M13` / `M13-THEME` | `P1` | `baseline_only` | `backend-auth` |
| `get_themes_active` | `GET` | `/api/v1/themes/active` | Themes | `M13` / `M13-THEME` | `P1` | `not_started` | `unassigned` |
