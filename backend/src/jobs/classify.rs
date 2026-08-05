//! 外部 Provider（SMTP/S3）错误分类（M01-JOBS-10）。
//!
//! 把具体错误归一化为 [`FailureClass`]：
//! - [`FailureClass::Transient`]：临时错误，按退避重试；
//! - [`FailureClass::Permanent`]：永久错误，直接 dead-letter；
//! - [`FailureClass::Cancelled`]：操作被取消（worker 停机/租约失效/调用方
//!   取消），不重试也不死信，交由租约恢复。
//!
//! 分类决定调用 [`fail_job`](crate::jobs::retry::fail_job) 时的 `RetryClass`：
//! `Transient → RetryClass::Transient`、`Permanent → RetryClass::Permanent`；
//! `Cancelled` 不调用 `fail_job`。

use crate::jobs::retry::RetryClass;

/// 失败分类结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureClass {
    /// 临时错误（SMTP 4xx、S3 429/5xx、超时、连接中断）：按退避重试。
    Transient,
    /// 永久错误（SMTP 5xx、S3 4xx、凭据/输入错误）：直接 dead-letter。
    Permanent,
    /// 取消（停机/租约失效）：不重试不死信，交由租约恢复。
    Cancelled,
}

impl FailureClass {
    /// 映射到 `fail_job` 可用的 `RetryClass`；`Cancelled` 返回 `None`。
    pub fn retry_class(self) -> Option<RetryClass> {
        match self {
            FailureClass::Transient => Some(RetryClass::Transient),
            FailureClass::Permanent => Some(RetryClass::Permanent),
            FailureClass::Cancelled => None,
        }
    }
}

/// 归一化后的外部 Provider 错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    /// SMTP 应答码（如 450、550）。
    Smtp { code: u16 },
    /// S3/HTTP 状态码（如 403、404、408、429、503）。
    S3 { status: u16 },
    /// 超时（连接/读取/操作）。
    Timeout { operation: &'static str },
    /// 连接中断/拒绝等网络层错误。
    Connection,
    /// 操作被取消。
    Cancelled,
}

impl ProviderError {
    /// 建立明确分类（M01-JOBS-10）。
    ///
    /// - SMTP：`4xx` 临时（421 服务不可用、450/451/452 邮箱暂不可用）→ 重试；
    ///   `5xx` 永久（550/551/552/553/554 等）→ 死信。
    /// - S3/HTTP：`408`/`425`/`429` 与 `5xx` 临时 → 重试；
    ///   其余 `4xx`（400 参数、403 无权限、404 对象不存在）永久 → 死信。
    /// - 超时与连接中断：临时 → 重试（重试需幂等，见 M01-JOBS-06）。
    /// - 取消：不重试、不死信。
    pub fn classify(&self) -> FailureClass {
        use FailureClass::*;
        match self {
            ProviderError::Smtp { code } if (400..500).contains(code) => Transient,
            ProviderError::Smtp { code } if (500..600).contains(code) => Permanent,
            // SMTP 其它应答（<400 或未知）：异常路径，按永久处理
            ProviderError::Smtp { .. } => Permanent,
            ProviderError::S3 { status }
                if *status == 408
                    || *status == 425
                    || *status == 429
                    || (500..600).contains(status) =>
            {
                Transient
            }
            ProviderError::S3 { .. } => Permanent,
            ProviderError::Timeout { .. } => Transient,
            ProviderError::Connection => Transient,
            ProviderError::Cancelled => Cancelled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::retry::RetryClass;

    #[test]
    fn smtp_4xx_is_transient() {
        for code in [421, 450, 451, 452, 455, 499] {
            assert_eq!(
                ProviderError::Smtp { code }.classify(),
                FailureClass::Transient,
                "SMTP {code} 应临时"
            );
        }
    }

    #[test]
    fn smtp_5xx_is_permanent() {
        for code in [500, 501, 550, 551, 552, 553, 554, 599] {
            assert_eq!(
                ProviderError::Smtp { code }.classify(),
                FailureClass::Permanent,
                "SMTP {code} 应永久"
            );
        }
    }

    #[test]
    fn s3_transient_statuses() {
        for status in [408, 425, 429, 500, 502, 503, 504] {
            assert_eq!(
                ProviderError::S3 { status }.classify(),
                FailureClass::Transient,
                "S3 {status} 应临时"
            );
        }
    }

    #[test]
    fn s3_permanent_statuses() {
        for status in [400, 401, 403, 404, 405, 409, 422] {
            assert_eq!(
                ProviderError::S3 { status }.classify(),
                FailureClass::Permanent,
                "S3 {status} 应永久"
            );
        }
    }

    #[test]
    fn timeout_and_connection_are_transient() {
        assert_eq!(
            ProviderError::Timeout {
                operation: "connect"
            }
            .classify(),
            FailureClass::Transient
        );
        assert_eq!(
            ProviderError::Timeout {
                operation: "s3.get_object"
            }
            .classify(),
            FailureClass::Transient
        );
        assert_eq!(
            ProviderError::Connection.classify(),
            FailureClass::Transient
        );
    }

    #[test]
    fn cancellation_is_not_retryable() {
        assert_eq!(ProviderError::Cancelled.classify(), FailureClass::Cancelled);
        assert_eq!(
            ProviderError::Cancelled.classify().retry_class(),
            None,
            "取消不映射到 RetryClass"
        );
    }

    #[test]
    fn retry_class_mapping_round_trips() {
        assert_eq!(
            FailureClass::Transient.retry_class(),
            Some(RetryClass::Transient)
        );
        assert_eq!(
            FailureClass::Permanent.retry_class(),
            Some(RetryClass::Permanent)
        );
        assert_eq!(FailureClass::Cancelled.retry_class(), None);
    }
}
