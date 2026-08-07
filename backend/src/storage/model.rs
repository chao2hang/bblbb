//! M06-SCHEMA：附件、配额与下载授权相关的领域类型。
//!
//! 与 `migrations/*/0048_storage_download.sql` 三库同构；这里只放与存储
//! 内核直接相关的数据载体（附件记录、配额策略/计数、对象头），路由层与
//! 上传/配额/下载域 agent 复用之。

use serde::{Deserialize, Serialize};

/// 存储后端（attachments.storage_backend）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageBackend {
    /// 本地磁盘（根目录外存储，object key 不可猜，路径穿越阻断）。
    Local,
    /// S3 兼容对象存储（AWS S3 / MinIO / R2）。
    S3,
}

impl StorageBackend {
    pub const ALL: [StorageBackend; 2] = [StorageBackend::Local, StorageBackend::S3];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::S3 => "s3",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "local" => Some(Self::Local),
            "s3" => Some(Self::S3),
            _ => None,
        }
    }
}

impl std::fmt::Display for StorageBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 附件状态机（M06-SCHEMA-05）：
/// pending → processing → ready；任何阶段可 → quarantined（安全处理失败）；
/// ready/quarantined 可 → deleted（30 天保留后物理删除）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AttachmentStatus {
    Pending,
    Processing,
    Ready,
    Quarantined,
    Deleted,
}

impl AttachmentStatus {
    pub const ALL: [AttachmentStatus; 5] = [
        AttachmentStatus::Pending,
        AttachmentStatus::Processing,
        AttachmentStatus::Ready,
        AttachmentStatus::Quarantined,
        AttachmentStatus::Deleted,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Ready => "ready",
            Self::Quarantined => "quarantined",
            Self::Deleted => "deleted",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "processing" => Some(Self::Processing),
            "ready" => Some(Self::Ready),
            "quarantined" => Some(Self::Quarantined),
            "deleted" => Some(Self::Deleted),
            _ => None,
        }
    }

    /// 合法状态迁移（M06-SCHEMA-05 约束的服务端裁决）。
    pub fn can_transition(self, next: Self) -> bool {
        use AttachmentStatus::*;
        matches!(
            (self, next),
            (Pending, Processing)
                | (Pending, Ready)
                | (Pending, Quarantined)
                | (Processing, Ready)
                | (Processing, Quarantined)
                | (Pending, Deleted)
                | (Processing, Deleted)
                | (Ready, Quarantined)
                | (Ready, Deleted)
                | (Quarantined, Deleted)
        )
    }
}

/// `attachments` 行（M06-SCHEMA-01）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentRecord {
    pub id: String,
    pub owner_id: String,
    pub storage_backend: StorageBackend,
    pub storage_key: String,
    pub original_name: Option<String>,
    pub media_type: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub status: AttachmentStatus,
    pub quota_bytes_charged: i64,
    pub is_public: bool,
    pub ref_count: i64,
    pub processing_version: i32,
    pub processing_error: Option<String>,
    pub created_at: i64,
    pub deleted_at: Option<i64>,
}

/// 创建附件时的声明输入（M06-UPLOAD-01）。
#[derive(Debug, Clone)]
pub struct NewAttachment {
    pub owner_id: String,
    pub original_name: Option<String>,
    pub media_type: String,
    pub size_bytes: i64,
    pub is_public: bool,
}

/// 等级配额策略快照（quota_policy_revisions 行，M06-QUOTA-01）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaPolicy {
    pub level: i64,
    pub single_file_max_bytes: i64,
    pub total_bytes: i64,
    pub daily_upload_bytes: i64,
    pub retention_days: i64,
    pub policy_version: i64,
}

/// 用户配额计数（user_quota_counters 行，M06-QUOTA-03/04）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuotaCounters {
    pub bytes_reserved: i64,
    pub bytes_charged: i64,
    pub bytes_released: i64,
}

impl QuotaCounters {
    /// 当前计入已用容量（charged，不计保留中字节）。
    pub fn used_bytes(&self) -> i64 {
        self.bytes_charged
    }

    /// 已用 + 保留中 = 提交后最终占用（用于预留判断，M06-QUOTA-05）。
    pub fn committed_after_reserve(&self, reserve: i64) -> i64 {
        self.bytes_charged + self.bytes_reserved + reserve
    }
}

/// 对象头（M06-ADAPTER-01 head/complete 复检）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectHead {
    pub size_bytes: i64,
    pub content_type: Option<String>,
    pub etag: Option<String>,
    pub exists: bool,
}

/// 预签名 URL（M06-ADAPTER-06：短 TTL、仅作为传输通道，不做权限裁决）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresignedUrl {
    pub url: String,
    /// 过期时刻（Unix 毫秒）；前端仅在过期时调后端重签。
    pub expires_at: i64,
    /// 传输方式（presigned PUT/GET）。
    pub method: &'static str,
}

/// 生成不可猜 object key（M06-ADAPTER-02）：`u/<owner>/<uuidv7>/<safe>`。
/// `<safe>` 仅保留白名单字符；路径穿越/绝对路径/符号链接由适配器层阻断。
pub fn generate_object_key(owner_id: &str, original_name: Option<&str>) -> String {
    let safe = original_name
        .map(|n| {
            let cleaned: String = n
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            let mut s = cleaned.trim_matches(['.', '_', '-']).to_string();
            if s.len() > 64 {
                s.truncate(64);
            }
            if s.is_empty() {
                s = "object".to_string();
            }
            s
        })
        .unwrap_or_else(|| "object".to_string());
    format!("u/{owner_id}/{}/{safe}", uuid::Uuid::now_v7())
}

/// 判断 storage_key 是否安全（无 `..` 路径段、非绝对路径、无空段），
/// 供本地适配器与迁移清单复用。
pub fn is_safe_key(key: &str) -> bool {
    if key.is_empty()
        || key.starts_with('/')
        || key.contains("..")
        || key.split('/').any(|s| s.is_empty())
    {
        return false;
    }
    !key.contains(['\\', '\0'])
}
