//! M6 存储域：附件、对象存储适配器、上传/配额、下载授权与迁移。
//!
//! 文件所有权（Wave M6-M7，主代理独占本模块骨架）：
//! - `model.rs` / `error.rs` / `adapter.rs`：主代理（M06-SCHEMA/ADAPTER 契约）
//! - `upload.rs` / `quota.rs`：上传+配额域 agent
//! - `migration.rs`：迁移+回滚域 agent
//! - `download` 相关逻辑在 `backend/src/download/`（下载域 agent）

pub mod adapter;
pub mod error;
pub mod migration;
pub mod model;
pub mod quota;
pub mod upload;

pub use adapter::{
    LocalAdapter, S3Adapter, S3Config, StorageAdapter, StorageConfig, StorageService,
};
pub use error::StorageError;
pub use model::{
    generate_object_key, AttachmentRecord, AttachmentStatus, NewAttachment, ObjectHead,
    PresignedUrl, QuotaCounters, QuotaPolicy, StorageBackend,
};

/// 当前毫秒时间戳（与 outbox/ledger 同源，避免多时钟漂移）。
pub fn now_millis() -> i64 {
    crate::outbox::now_millis()
}
