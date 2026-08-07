//! M06-DOWNLOAD：下载授权与积分抵扣。
//!
//! - 策略优先级：附件 → 板块 → 站点（DOWNLOAD-BILLING.md）。
//! - 价格解析：附件覆盖 → 板块覆盖 → 站点默认；等级/角色免费规则与管理员
//!   强制策略（forced_free/forced_paid）由服务层解释并审计。
//! - 首次授权创建 `download_authorizations`（免费也写授权，不绕过流程）；
//!   扣款、point operation、授权、审计、Outbox 同一事务。
//! - 有效授权重签 URL 不重复扣款；URL TTL 与授权有效期独立。
//! - 失败不泄漏对象/授权信息；未 ready、无权限、余额不足、封禁、策略停用
//!   返回不区分原因的通用错误。

pub mod service;

pub use service::{download, get_authorization, sign_url, DownloadError, DownloadPolicy};
