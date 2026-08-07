use axum::Router;

use crate::app::AppState;

/// 反应路由。
///
/// 主端点（POST/DELETE `/api/v1/posts/{id}/reactions`、`/api/v1/comments/{id}/reactions`）
/// 注册在 posts.rs/comments.rs，由主代理接线到 `crate::reactions::service`；
/// 本模块无独立路径（避免与既有端点冲突）。
pub fn router() -> Router<AppState> {
    Router::new()
}
