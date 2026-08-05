//! M03-PROFILE-07：账户注销匿名化服务。
//!
//! 语义（RETENTION-PRIVACY.md §矩阵）：
//! - 帖子/评论等公开讨论**保留**（内容按策略处理），作者标识保留 `author_id`
//!   指向已匿名化的 users 行；公开投影对 `deleted` 用户返回 404（PROFILE-06），
//!   前端以"已注销用户"降级展示，等同替换作者标识；
//! - users 行就地匿名化：username/email 替换为不可识别且唯一的派生值，
//!   display_name/bio/signature/头像/Cover/last_login_at/delete_requested_at
//!   清空，status → `deleted`（终止态），version +1；
//! - 断开可识别资料关系：删除 user_preferences/user_privacy；
//! - 立即撤销全部 Session（revoked_at + revoke_reason='account_deleted'）；
//! - 审计/账务记录不删除（不可删除审计，MODERATION.md §11）；profile_revisions
//!   保留。
//!
//! 注销请求/冷却/取消/执行 Job/法律保留属 M03-PROFILE-08；本模块是执行器。

use sqlx::Either;

use crate::db::DatabasePool;
use crate::outbox::now_millis;

/// 匿名化用户的派生用户名前缀（不可识别、全局唯一）。
const DELETED_USERNAME_PREFIX: &str = "deleted_user_";
/// 匿名化邮箱域名（不可路由，RFC 2606 `.invalid`）。
const DELETED_EMAIL_DOMAIN: &str = "@deleted.invalid";

/// 执行注销匿名化（单事务，幂等：已 deleted 的行再次调用无副作用）。
pub async fn anonymize_user(pool: &DatabasePool, user_id: &str) -> Result<(), String> {
    let now = now_millis();
    let short_id = &user_id[..user_id.len().min(12)];
    let anonymous_username = format!("{DELETED_USERNAME_PREFIX}{short_id}");
    let anonymous_email = format!("{short_id}{DELETED_EMAIL_DOMAIN}");

    let mut tx = match pool {
        Either::Left(p) => Either::Left(p.begin().await.map_err(|e| e.to_string())?),
        Either::Right(p) => Either::Right(p.begin().await.map_err(|e| e.to_string())?),
    };

    // 1. users 就地匿名化
    let affected = match &mut tx {
        Either::Left(t) => sqlx::query(
            "UPDATE users
             SET username_normalized = ?,
                 email_normalized = ?,
                 display_name = NULL,
                 bio = NULL,
                 signature = NULL,
                 avatar_attachment_id = NULL,
                 cover_attachment_id = NULL,
                 last_login_at = NULL,
                 delete_requested_at = NULL,
                 deleted_at = ?,
                 status = 'deleted',
                 level = 1,
                 version = version + 1
             WHERE id = ? AND status != 'deleted'",
        )
        .bind(&anonymous_username)
        .bind(&anonymous_email)
        .bind(now)
        .bind(user_id)
        .execute(&mut **t)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected(),
        Either::Right(t) => sqlx::query(
            "UPDATE users
             SET username_normalized = ?,
                 email_normalized = ?,
                 display_name = NULL,
                 bio = NULL,
                 signature = NULL,
                 avatar_attachment_id = NULL,
                 cover_attachment_id = NULL,
                 last_login_at = NULL,
                 delete_requested_at = NULL,
                 deleted_at = ?,
                 status = 'deleted',
                 level = 1,
                 version = version + 1
             WHERE id = ? AND status != 'deleted'",
        )
        .bind(&anonymous_username)
        .bind(&anonymous_email)
        .bind(now)
        .bind(user_id)
        .execute(&mut **t)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected(),
    };
    if affected == 0 {
        // 用户不存在或已匿名化（幂等）
        return match tx {
            Either::Left(t) => t.commit().await.map_err(|e| e.to_string()),
            Either::Right(t) => t.commit().await.map_err(|e| e.to_string()),
        };
    }

    // 2. 断开可识别资料关系：删除私有偏好/隐私行
    match &mut tx {
        Either::Left(t) => {
            sqlx::query("DELETE FROM user_preferences WHERE user_id = ?")
                .bind(user_id)
                .execute(&mut **t)
                .await
                .map_err(|e| e.to_string())?;
            sqlx::query("DELETE FROM user_privacy WHERE user_id = ?")
                .bind(user_id)
                .execute(&mut **t)
                .await
                .map_err(|e| e.to_string())?;
        }
        Either::Right(t) => {
            sqlx::query("DELETE FROM user_preferences WHERE user_id = ?")
                .bind(user_id)
                .execute(&mut **t)
                .await
                .map_err(|e| e.to_string())?;
            sqlx::query("DELETE FROM user_privacy WHERE user_id = ?")
                .bind(user_id)
                .execute(&mut **t)
                .await
                .map_err(|e| e.to_string())?;
        }
    }

    // 3. 立即撤销全部 Session（含设备）
    match &mut tx {
        Either::Left(t) => {
            sqlx::query(
                "UPDATE user_sessions SET revoked_at = ?, revoke_reason = 'account_deleted'
                 WHERE user_id = ? AND revoked_at IS NULL",
            )
            .bind(now)
            .bind(user_id)
            .execute(&mut **t)
            .await
            .map_err(|e| e.to_string())?;
        }
        Either::Right(t) => {
            sqlx::query(
                "UPDATE user_sessions SET revoked_at = ?, revoke_reason = 'account_deleted'
                 WHERE user_id = ? AND revoked_at IS NULL",
            )
            .bind(now)
            .bind(user_id)
            .execute(&mut **t)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    match tx {
        Either::Left(t) => t.commit().await.map_err(|e| e.to_string()),
        Either::Right(t) => t.commit().await.map_err(|e| e.to_string()),
    }
}
