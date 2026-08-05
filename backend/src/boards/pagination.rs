//! M03-BOARDS-04：板块列表 cursor 分页——不透明游标 + 稳定排序。
//!
//! 稳定排序键 = `(sort_order ASC, created_at ASC, id ASC)`（id 兜底保证确定性，
//! 同级排序号 + 同毫秒创建也能稳定排序）。游标是最后一条已返回项的排序键，
//! base64url（no-pad）编码，对客户端不透明；服务端只按排序键字典序跳过，
//! 分页不重不漏。

use base64::Engine as _;
use serde::{Deserialize, Serialize};

/// 排序键（游标负载）：`(sort_order, created_at, id)`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardCursor {
    pub sort_order: i64,
    pub created_at: i64,
    pub id: String,
}

impl BoardCursor {
    /// 字典序比较：本键严格大于 `other`（游标跳过语义）。
    pub fn gt(&self, other: &BoardCursor) -> bool {
        (self.sort_order, self.created_at, self.id.as_str())
            > (other.sort_order, other.created_at, other.id.as_str())
    }
}

/// 游标编码（base64url no-pad；不透明）。
pub fn encode_cursor(cursor: &BoardCursor) -> String {
    let json = serde_json::to_string(cursor).expect("BoardCursor 可序列化");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json.as_bytes())
}

/// 游标解码错误（路由层映射为 400 invalid_request）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorDecodeError;

/// 游标解码（base64 + JSON 双重校验）。
pub fn decode_cursor(encoded: &str) -> Result<BoardCursor, CursorDecodeError> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| CursorDecodeError)?;
    serde_json::from_slice(&bytes).map_err(|_| CursorDecodeError)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cursor(sort_order: i64, created_at: i64, id: &str) -> BoardCursor {
        BoardCursor {
            sort_order,
            created_at,
            id: id.to_string(),
        }
    }

    #[test]
    fn cursor_round_trips_through_base64url() {
        let c = cursor(3, 1_722_816_000_000, "01911fd5-f000-7561-a2a5-3dd6434157f0");
        let encoded = encode_cursor(&c);
        assert!(
            !encoded.contains('+') && !encoded.contains('/') && !encoded.contains('='),
            "必须 base64url no-pad"
        );
        assert_eq!(decode_cursor(&encoded), Ok(c));
    }

    #[test]
    fn malformed_cursor_is_rejected() {
        assert_eq!(decode_cursor("!!!not-base64!!!"), Err(CursorDecodeError));
        assert_eq!(
            decode_cursor(&encode_cursor(&cursor(1, 1, "x"))[..3]),
            Err(CursorDecodeError),
            "截断游标必须拒绝"
        );
        assert_eq!(decode_cursor(""), Err(CursorDecodeError));
    }

    #[test]
    fn gt_follows_tuple_lexicographic_order() {
        let base = cursor(1, 100, "a");
        assert!(cursor(2, 0, "z").gt(&base), "sort_order 优先");
        assert!(!cursor(0, 999, "z").gt(&base));
        assert!(cursor(1, 101, "z").gt(&base), "同 sort_order 比 created_at");
        assert!(!cursor(1, 100, "a").gt(&base), "同键不严格大于");
        assert!(cursor(1, 100, "b").gt(&base), "同键同 created_at 比 id");
        assert!(
            !cursor(1, 100, "a").gt(&cursor(1, 100, "b")),
            "id 较小不 gt"
        );
        assert!(cursor(1, 100, "c").gt(&cursor(1, 100, "b")), "id 较大 gt");
    }
}
