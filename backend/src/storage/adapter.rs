//! M06-ADAPTER：Local / S3 存储适配器。
//!
//! 抽象接口覆盖 create/temp/upload/complete/read/range/delete/head/copy/list 与
//! multipart 生命周期（M06-ADAPTER-01/05），并给出两个实现：
//! - `LocalAdapter`：根目录外存储、不可猜 object key、路径穿越/绝对路径/符号
//!   链接阻断（M06-ADAPTER-02）；Rust stream 上传模式，不支持预签名。
//! - `S3Adapter`：AWS S3 / MinIO / R2 兼容，virtual-host/path-style、region
//!   auto、endpoint TLS 校验、预签名上传/下载与 multipart（M06-ADAPTER-03/04）。
//!
//! 错误分类见 [`StorageError`]（M06-ADAPTER-08/10）：供应商 403/404/409/429/
//! 5xx、超时、DNS/TLS 与部分上传映射为稳定 Problem code。

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use aws_sdk_s3::presigning::{PresignedRequest, PresigningConfig};
use aws_sdk_s3::primitives::ByteStream;

use crate::storage::error::StorageError;
use crate::storage::model::{is_safe_key, ObjectHead, PresignedUrl, StorageBackend};

/// S3 连接配置（仅由 Rust 配置层读取；前端只得到脱敏状态，M06-ADAPTER-03）。
#[derive(Debug, Clone)]
pub struct S3Config {
    pub bucket: String,
    /// 区域或 `auto`（R2）。
    pub region: String,
    /// 自定义 endpoint（MinIO/R2 网关）；为空使用 AWS 默认。
    pub endpoint: Option<String>,
    /// path-style 访问（兼容 MinIO/本地网关）。
    pub path_style: bool,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub session_token: Option<String>,
}

/// 存储服务配置。
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// 本地根目录（必须位于进程可写目录，禁止把仓库根作为存储根）。
    pub local_root: PathBuf,
    /// 可选 S3 后端；None 时全站使用 local。
    pub s3: Option<S3Config>,
}

impl StorageConfig {
    /// 默认后端：配置了 S3 则 s3，否则 local。
    pub fn default_backend(&self) -> StorageBackend {
        if self.s3.is_some() {
            StorageBackend::S3
        } else {
            StorageBackend::Local
        }
    }
}

/// 对象存储抽象接口（M06-ADAPTER-01）。
#[async_trait]
pub trait StorageAdapter: Send + Sync {
    fn backend(&self) -> StorageBackend;

    /// 是否支持预签名 URL（S3 支持；local 用 Rust stream 传输）。
    fn supports_presign(&self) -> bool {
        false
    }

    /// 对象头（head；用于 complete 复检与配额核对，M06-UPLOAD-04）。
    async fn head_object(&self, key: &str) -> Result<ObjectHead, StorageError>;

    /// 读取整个对象（内容安全 worker 流式处理用）。
    async fn read_object(&self, key: &str) -> Result<Vec<u8>, StorageError>;

    /// Range 读取（下载端点与封面预览用；`start`/`len` 已服务端校验）。
    async fn read_range(&self, key: &str, start: u64, len: u64) -> Result<Vec<u8>, StorageError>;

    /// 写入对象（Rust stream 模式 / 迁移复制用）。
    async fn write_object(
        &self,
        key: &str,
        data: &[u8],
        content_type: Option<&str>,
    ) -> Result<(), StorageError>;

    /// 删除对象（物理删除，仅在配额释放校验通过后调用）。
    async fn delete_object(&self, key: &str) -> Result<(), StorageError>;

    /// 复制对象（迁移/断点续传用）。
    async fn copy_object(&self, from_key: &str, to_key: &str) -> Result<(), StorageError>;

    /// 列出对象（迁移 manifest 与孤儿清理用）。
    async fn list_objects(&self, prefix: &str) -> Result<Vec<String>, StorageError>;

    /// 预签名上传（M06-ADAPTER-06：短 TTL，仅传输通道，不做权限裁决）。
    async fn presign_upload(
        &self,
        key: &str,
        content_type: &str,
        ttl_secs: u64,
    ) -> Result<PresignedUrl, StorageError>;

    /// 预签名下载（TTL 独立于附件生命周期，M06-QUOTA-08）。
    async fn presign_download(
        &self,
        key: &str,
        ttl_secs: u64,
    ) -> Result<PresignedUrl, StorageError>;

    /// multipart 上传：开始（M06-ADAPTER-05）。
    async fn begin_multipart(&self, key: &str, content_type: &str) -> Result<String, StorageError>;
    /// multipart 上传：上传一个 part（返回 ETag）。
    async fn upload_part(
        &self,
        key: &str,
        upload_id: &str,
        part_number: i32,
        data: &[u8],
    ) -> Result<String, StorageError>;
    /// multipart 上传：完成（parts 必须按 part_number 升序）。
    async fn complete_multipart(
        &self,
        key: &str,
        upload_id: &str,
        parts: &[(i32, String)],
    ) -> Result<(), StorageError>;
    /// multipart 上传：中止并清理孤儿（M06-ADAPTER-05 孤儿清理）。
    async fn abort_multipart(&self, key: &str, upload_id: &str) -> Result<(), StorageError>;
}

/// 校验 key 并对本地路径做穿越防护。
fn local_path(root: &Path, key: &str) -> Result<PathBuf, StorageError> {
    if !is_safe_key(key) {
        return Err(StorageError::Invalid(format!("unsafe object key: {key}")));
    }
    let path = root.join(key);
    // 符号链接阻断：父目录链上任何一层是符号链接都拒绝。
    let mut cursor = path.as_path();
    while let Ok(meta) = std::fs::symlink_metadata(cursor) {
        if meta.file_type().is_symlink() {
            return Err(StorageError::Forbidden(format!(
                "symlink in object path: {key}"
            )));
        }
        match cursor.parent() {
            Some(parent) => cursor = parent,
            None => break,
        }
    }
    Ok(path)
}

/// 本地磁盘适配器（Rust stream 模式；不支持预签名与 multipart）。
pub struct LocalAdapter {
    root: PathBuf,
}

impl LocalAdapter {
    pub fn new(root: PathBuf) -> Result<Self, StorageError> {
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[async_trait]
impl StorageAdapter for LocalAdapter {
    fn backend(&self) -> StorageBackend {
        StorageBackend::Local
    }

    async fn head_object(&self, key: &str) -> Result<ObjectHead, StorageError> {
        let path = local_path(&self.root, key)?;
        match std::fs::metadata(&path) {
            Ok(meta) => Ok(ObjectHead {
                size_bytes: meta.len() as i64,
                content_type: None,
                etag: None,
                exists: true,
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ObjectHead {
                size_bytes: 0,
                content_type: None,
                etag: None,
                exists: false,
            }),
            Err(e) => Err(e.into()),
        }
    }

    async fn read_object(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        let path = local_path(&self.root, key)?;
        tokio::fs::read(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound(format!("object {key}"))
            } else {
                StorageError::from(e)
            }
        })
    }

    async fn read_range(&self, key: &str, start: u64, len: u64) -> Result<Vec<u8>, StorageError> {
        let path = local_path(&self.root, key)?;
        let data = tokio::fs::read(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound(format!("object {key}"))
            } else {
                StorageError::from(e)
            }
        })?;
        let start = (start as usize).min(data.len());
        let end = (start.saturating_add(len as usize)).min(data.len());
        Ok(data[start..end].to_vec())
    }

    async fn write_object(
        &self,
        key: &str,
        data: &[u8],
        _content_type: Option<&str>,
    ) -> Result<(), StorageError> {
        let path = local_path(&self.root, key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&path, data).await?;
        Ok(())
    }

    async fn delete_object(&self, key: &str) -> Result<(), StorageError> {
        let path = local_path(&self.root, key)?;
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    async fn copy_object(&self, from_key: &str, to_key: &str) -> Result<(), StorageError> {
        let from = local_path(&self.root, from_key)?;
        let to = local_path(&self.root, to_key)?;
        // 同路径复制（如 local→local 迁移/回滚同 key）：no-op，
        // 避免 fs::copy 同源目标时先截断源文件。
        if from == to {
            return Ok(());
        }
        if let Some(parent) = to.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::copy(&from, &to).await?;
        Ok(())
    }

    async fn list_objects(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
        let base = local_path(&self.root, if prefix.is_empty() { "u" } else { prefix })?;
        let mut out = Vec::new();
        let mut stack = vec![base];
        while let Some(dir) = stack.pop() {
            let mut entries = tokio::fs::read_dir(&dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let meta = entry.metadata().await?;
                if meta.is_dir() {
                    stack.push(entry.path());
                } else if meta.is_file() {
                    let path = entry.path();
                    let rel = path.strip_prefix(&self.root).map_err(|_| {
                        StorageError::Internal("path outside local root".to_string())
                    })?;
                    out.push(rel.to_string_lossy().to_string());
                }
            }
        }
        Ok(out)
    }

    async fn presign_upload(
        &self,
        _key: &str,
        _content_type: &str,
        _ttl_secs: u64,
    ) -> Result<PresignedUrl, StorageError> {
        Err(StorageError::Unsupported(
            "local backend uses Rust stream upload".to_string(),
        ))
    }

    async fn presign_download(
        &self,
        _key: &str,
        _ttl_secs: u64,
    ) -> Result<PresignedUrl, StorageError> {
        Err(StorageError::Unsupported(
            "local backend streams through the download endpoint".to_string(),
        ))
    }

    async fn begin_multipart(
        &self,
        _key: &str,
        _content_type: &str,
    ) -> Result<String, StorageError> {
        Err(StorageError::Unsupported(
            "local backend uses Rust stream upload".to_string(),
        ))
    }

    async fn upload_part(
        &self,
        _key: &str,
        _upload_id: &str,
        _part_number: i32,
        _data: &[u8],
    ) -> Result<String, StorageError> {
        Err(StorageError::Unsupported(
            "local backend uses Rust stream upload".to_string(),
        ))
    }

    async fn complete_multipart(
        &self,
        _key: &str,
        _upload_id: &str,
        _parts: &[(i32, String)],
    ) -> Result<(), StorageError> {
        Err(StorageError::Unsupported(
            "local backend uses Rust stream upload".to_string(),
        ))
    }

    async fn abort_multipart(&self, _key: &str, _upload_id: &str) -> Result<(), StorageError> {
        Err(StorageError::Unsupported(
            "local backend uses Rust stream upload".to_string(),
        ))
    }
}

/// 把 AWS SDK 错误分类为稳定存储错误（M06-ADAPTER-08）。
fn classify_sdk<E: std::fmt::Debug>(
    e: &aws_sdk_s3::error::SdkError<E>,
    operation: &str,
) -> StorageError {
    use aws_sdk_s3::error::SdkError;
    match e {
        SdkError::ServiceError(_) => {
            let status = e.raw_response().map(|r| r.status().as_u16()).unwrap_or(0);
            match status {
                401 => StorageError::Auth(format!("s3 {operation} auth failed")),
                403 => StorageError::Forbidden(format!("s3 {operation} forbidden")),
                404 => StorageError::NotFound(format!("s3 {operation} object not found")),
                409 => StorageError::Conflict(format!("s3 {operation} conflict")),
                429 => StorageError::RateLimited(format!("s3 {operation} rate limited")),
                500..=599 => StorageError::Upstream(format!("s3 {operation} upstream {status}")),
                _ => StorageError::Upstream(format!("s3 {operation} service error {status}")),
            }
        }
        SdkError::TimeoutError(_) => StorageError::Network(format!("s3 {operation} timeout")),
        SdkError::DispatchFailure(_) => {
            StorageError::Network(format!("s3 {operation} dispatch failure (dns/tls)"))
        }
        other => StorageError::Upstream(format!("s3 {operation} sdk error: {other:?}")),
    }
}

/// 预签名请求 → PresignedUrl；过期时间由 TTL 计算（签名请求不携带该字段，
/// 前端仅在过期时重签，不依赖此值做权限裁决）。
fn presigned_to_url(req: PresignedRequest, method: &'static str, ttl_secs: u64) -> PresignedUrl {
    let expires_at = crate::outbox::now_millis() + (ttl_secs * 1000) as i64;
    PresignedUrl {
        url: req.uri().to_string(),
        expires_at,
        method,
    }
}

/// S3 兼容适配器（AWS S3 / MinIO / R2）。
pub struct S3Adapter {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl S3Adapter {
    pub async fn new(cfg: &S3Config) -> Result<Self, StorageError> {
        let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(cfg.region.clone()));
        if let Some(endpoint) = &cfg.endpoint {
            loader = loader.endpoint_url(endpoint);
        }
        if let (Some(ak), Some(sk)) = (&cfg.access_key_id, &cfg.secret_access_key) {
            loader = loader.credentials_provider(
                aws_credential_types::provider::SharedCredentialsProvider::new(
                    aws_credential_types::Credentials::new(
                        ak,
                        sk,
                        cfg.session_token.clone(),
                        None,
                        "bblbb-storage",
                    ),
                ),
            );
        }
        let sdk_config = loader.load().await;

        let mut sb = aws_sdk_s3::config::Builder::from(&sdk_config);
        if cfg.path_style {
            sb = sb.force_path_style(true);
        }
        let client = aws_sdk_s3::Client::from_conf(sb.build());
        Ok(Self {
            client,
            bucket: cfg.bucket.clone(),
        })
    }
}

#[async_trait]
impl StorageAdapter for S3Adapter {
    fn backend(&self) -> StorageBackend {
        StorageBackend::S3
    }

    fn supports_presign(&self) -> bool {
        true
    }

    async fn head_object(&self, key: &str) -> Result<ObjectHead, StorageError> {
        let resp = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| classify_sdk(&e, "head"))?;
        Ok(ObjectHead {
            size_bytes: resp.content_length().unwrap_or(0) as i64,
            content_type: resp.content_type().map(|s| s.to_string()),
            etag: resp.e_tag().map(|s| s.to_string()),
            exists: true,
        })
    }

    async fn read_object(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| classify_sdk(&e, "get"))?;
        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| StorageError::Network(format!("s3 get body failed: {e}")))?;
        Ok(bytes.into_bytes().to_vec())
    }

    async fn read_range(&self, key: &str, start: u64, len: u64) -> Result<Vec<u8>, StorageError> {
        let range = format!("bytes={start}-{}", start + len.saturating_sub(1));
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .range(range)
            .send()
            .await
            .map_err(|e| classify_sdk(&e, "get-range"))?;
        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| StorageError::Network(format!("s3 get-range body failed: {e}")))?;
        Ok(bytes.into_bytes().to_vec())
    }

    async fn write_object(
        &self,
        key: &str,
        data: &[u8],
        content_type: Option<&str>,
    ) -> Result<(), StorageError> {
        let mut req = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(ByteStream::from(data.to_vec()));
        if let Some(ct) = content_type {
            req = req.content_type(ct);
        }
        req.send().await.map_err(|e| classify_sdk(&e, "put"))?;
        Ok(())
    }

    async fn delete_object(&self, key: &str) -> Result<(), StorageError> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| classify_sdk(&e, "delete"))?;
        Ok(())
    }

    async fn copy_object(&self, from_key: &str, to_key: &str) -> Result<(), StorageError> {
        let source = format!("{}/{}", self.bucket, from_key);
        self.client
            .copy_object()
            .bucket(&self.bucket)
            .key(to_key)
            .copy_source(source)
            .send()
            .await
            .map_err(|e| classify_sdk(&e, "copy"))?;
        Ok(())
    }

    async fn list_objects(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut req = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(prefix);
            if let Some(t) = &token {
                req = req.continuation_token(t);
            }
            let resp = req.send().await.map_err(|e| classify_sdk(&e, "list"))?;
            for obj in resp.contents() {
                if let Some(k) = obj.key() {
                    out.push(k.to_string());
                }
            }
            if resp.is_truncated().unwrap_or(false) {
                token = resp.next_continuation_token().map(|s| s.to_string());
            } else {
                break;
            }
        }
        Ok(out)
    }

    async fn presign_upload(
        &self,
        key: &str,
        content_type: &str,
        ttl_secs: u64,
    ) -> Result<PresignedUrl, StorageError> {
        let cfg = PresigningConfig::expires_in(std::time::Duration::from_secs(ttl_secs))
            .map_err(|e| StorageError::Invalid(format!("presign ttl: {e}")))?;
        let req = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .presigned(cfg)
            .await
            .map_err(|e| classify_sdk(&e, "presign-put"))?;
        Ok(presigned_to_url(req, "PUT", ttl_secs))
    }

    async fn presign_download(
        &self,
        key: &str,
        ttl_secs: u64,
    ) -> Result<PresignedUrl, StorageError> {
        let cfg = PresigningConfig::expires_in(std::time::Duration::from_secs(ttl_secs))
            .map_err(|e| StorageError::Invalid(format!("presign ttl: {e}")))?;
        let req = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(cfg)
            .await
            .map_err(|e| classify_sdk(&e, "presign-get"))?;
        Ok(presigned_to_url(req, "GET", ttl_secs))
    }

    async fn begin_multipart(&self, key: &str, content_type: &str) -> Result<String, StorageError> {
        let resp = self
            .client
            .create_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| classify_sdk(&e, "create-multipart"))?;
        resp.upload_id()
            .map(|s| s.to_string())
            .ok_or_else(|| StorageError::Upstream("multipart missing upload_id".to_string()))
    }

    async fn upload_part(
        &self,
        key: &str,
        upload_id: &str,
        part_number: i32,
        data: &[u8],
    ) -> Result<String, StorageError> {
        let resp = self
            .client
            .upload_part()
            .bucket(&self.bucket)
            .key(key)
            .upload_id(upload_id)
            .part_number(part_number)
            .body(ByteStream::from(data.to_vec()))
            .send()
            .await
            .map_err(|e| classify_sdk(&e, "upload-part"))?;
        resp.e_tag()
            .map(|s| s.to_string())
            .ok_or_else(|| StorageError::Upstream("upload-part missing etag".to_string()))
    }

    async fn complete_multipart(
        &self,
        key: &str,
        upload_id: &str,
        parts: &[(i32, String)],
    ) -> Result<(), StorageError> {
        let mut sorted = parts.to_vec();
        sorted.sort_by_key(|(n, _)| *n);
        if sorted.is_empty() || sorted.len() > 10_000 {
            return Err(StorageError::PartialUpload(
                "multipart parts count out of range".to_string(),
            ));
        }
        let completed: Vec<aws_sdk_s3::types::CompletedPart> = sorted
            .iter()
            .map(|(n, etag)| {
                aws_sdk_s3::types::CompletedPart::builder()
                    .part_number(*n)
                    .e_tag(etag)
                    .build()
            })
            .collect();
        let mut seq = 0;
        for (n, _) in &sorted {
            seq += 1;
            if *n != seq {
                return Err(StorageError::PartialUpload(
                    "multipart parts must be contiguous from 1".to_string(),
                ));
            }
        }
        let upload = aws_sdk_s3::types::CompletedMultipartUpload::builder()
            .set_parts(Some(completed))
            .build();
        self.client
            .complete_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .upload_id(upload_id)
            .multipart_upload(upload)
            .send()
            .await
            .map_err(|e| classify_sdk(&e, "complete-multipart"))?;
        Ok(())
    }

    async fn abort_multipart(&self, key: &str, upload_id: &str) -> Result<(), StorageError> {
        self.client
            .abort_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .upload_id(upload_id)
            .send()
            .await
            .map_err(|e| classify_sdk(&e, "abort-multipart"))?;
        Ok(())
    }
}

/// 存储服务门面：按后端分发适配器，路由与域服务经此调用。
pub struct StorageService {
    local: LocalAdapter,
    s3: Option<S3Adapter>,
    default_backend: StorageBackend,
}

impl StorageService {
    /// 仅本地后端（同步构造；测试与纯本地部署用，免 S3 配置）。
    pub fn local_only(root: PathBuf) -> Result<Self, StorageError> {
        let local = LocalAdapter::new(root)?;
        Ok(Self {
            local,
            s3: None,
            default_backend: StorageBackend::Local,
        })
    }

    pub async fn new(cfg: &StorageConfig) -> Result<Self, StorageError> {
        let local = LocalAdapter::new(cfg.local_root.clone())?;
        let s3 = match &cfg.s3 {
            Some(s3) => Some(S3Adapter::new(s3).await?),
            None => None,
        };
        let default_backend = cfg.default_backend();
        Ok(Self {
            local,
            s3,
            default_backend,
        })
    }

    pub fn default_backend(&self) -> StorageBackend {
        self.default_backend
    }

    pub fn adapter(&self, backend: StorageBackend) -> Result<&dyn StorageAdapter, StorageError> {
        match backend {
            StorageBackend::Local => Ok(&self.local),
            StorageBackend::S3 => self
                .s3
                .as_ref()
                .map(|a| a as &dyn StorageAdapter)
                .ok_or_else(|| StorageError::Invalid("s3 backend not configured".to_string())),
        }
    }

    pub fn local_root(&self) -> &Path {
        self.local.root()
    }
}
