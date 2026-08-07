//! M04-VISIBILITY-03：`visibility_level ≤ 作者等级` 纯校验。
//!
//! 纯函数（无 IO），供 create/draft/edit/publish/scheduled 写路径复用：
//! 请求的可见等级缺省按 1；`<1` → [`VisibilityError::Invalid`]；
//! `> author_level` → 稳定 [`VisibilityError::ExceedsAuthorLevel`]
//! （路由层映射为 422 `visibility_level_exceeds_author`）。

use std::fmt;

/// 可见等级校验错误（稳定 Display；不含原始输入回显）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisibilityError {
    /// 值 < 1。
    Invalid,
    /// requested > author_level（ERROR-CODES.md：422 `visibility_level_exceeds_author`）。
    ExceedsAuthorLevel { requested: u32, author_level: u32 },
}

impl fmt::Display for VisibilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid => write!(f, "visibility_level must be >= 1"),
            Self::ExceedsAuthorLevel {
                requested,
                author_level,
            } => write!(
                f,
                "visibility_level {requested} exceeds author level {author_level}"
            ),
        }
    }
}

impl std::error::Error for VisibilityError {}

/// 纯校验：缺省按 1；`<1` → [`VisibilityError::Invalid`]；
/// `> author_level` → [`VisibilityError::ExceedsAuthorLevel`]。
pub fn validate_visibility_level(
    visibility_level: Option<u32>,
    author_level: u32,
) -> Result<u32, VisibilityError> {
    let requested = visibility_level.unwrap_or(1);
    if requested < 1 {
        return Err(VisibilityError::Invalid);
    }
    if requested > author_level {
        return Err(VisibilityError::ExceedsAuthorLevel {
            requested,
            author_level,
        });
    }
    Ok(requested)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;

    /// M04-VISIBILITY-03 边界：0 / 1 / author_level / author_level+1。
    #[test]
    fn boundaries_with_author_level_5() {
        assert_eq!(
            validate_visibility_level(Some(0), 5),
            Err(VisibilityError::Invalid)
        );
        assert_eq!(validate_visibility_level(Some(1), 5), Ok(1));
        assert_eq!(validate_visibility_level(Some(5), 5), Ok(5));
        assert_eq!(
            validate_visibility_level(Some(6), 5),
            Err(VisibilityError::ExceedsAuthorLevel {
                requested: 6,
                author_level: 5
            })
        );
        // 缺省按 1
        assert_eq!(validate_visibility_level(None, 5), Ok(1));
    }

    #[test]
    fn boundaries_with_author_level_1() {
        assert_eq!(validate_visibility_level(Some(1), 1), Ok(1));
        assert_eq!(
            validate_visibility_level(Some(2), 1),
            Err(VisibilityError::ExceedsAuthorLevel {
                requested: 2,
                author_level: 1
            })
        );
        assert_eq!(
            validate_visibility_level(Some(0), 1),
            Err(VisibilityError::Invalid)
        );
    }

    #[test]
    fn error_is_stable_and_typed() {
        let err = VisibilityError::ExceedsAuthorLevel {
            requested: 9,
            author_level: 3,
        };
        assert_eq!(err.to_string(), "visibility_level 9 exceeds author level 3");
        assert_eq!(
            VisibilityError::Invalid.to_string(),
            "visibility_level must be >= 1"
        );
        // 与 domain 侧既有错误语义一致（command.rs 同文案）
        assert_eq!(
            err,
            VisibilityError::ExceedsAuthorLevel {
                requested: 9,
                author_level: 3
            }
        );
    }

    /// AppError::visibility_level_exceeds_author（422）序列化为稳定 problem code。
    #[tokio::test]
    async fn visibility_exceeds_author_error_serializes_422() {
        use axum::http::StatusCode;
        use axum::response::IntoResponse;
        use http_body_util::BodyExt;

        let resp = AppError::visibility_level_exceeds_author(
            "visibility_level 4 exceeds author level 3",
            "req-vis",
        )
        .into_response();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY, "必须 422");

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["code"], "visibility_level_exceeds_author");
        assert_eq!(v["status"], 422);
        assert_eq!(v["type"], "about:blank");
        assert!(v["detail"]
            .as_str()
            .unwrap()
            .contains("exceeds author level"));
    }
}
