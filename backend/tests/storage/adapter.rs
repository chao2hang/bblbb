//! M16-STORAGE-FAULTS-01/02/03：local/S3 Adapter contract 与故障注入。
//!
//! 覆盖（驱动 SHIPPED 代码 backend/src/storage/{adapter,model,error}.rs）：
//!   * key 安全（路径穿越/绝对路径/反斜杠/NUL）——`is_safe_key` 单测。
//!   * LocalAdapter 完整 contract：create/head/read/range/write/delete/copy/list；
//!     multipart 对 local 是 Unsupported（local 用 Rust stream 上传，契约明确）。
//!   * LocalAdapter 路径穿越/符号链接阻断。
//!   * S3 mock 服务器（进程内 TCP）注入 403/404/429/5xx → 稳定分类
//!     （StorageError::Forbidden/NotFound/RateLimited/Upstream + retryable 语义）。
//!   * S3 mock 的 multipart 生命周期（Initiate/UploadPart/Complete/Abort）。
//!   * 预签名 URL 生成与 TTL 过期语义（本地签名，无网络）。
//!
//! 真实 AWS S3/MinIO/R2 兼容矩阵为外部阻塞项（M16-STORAGE-FAULTS-01 `[!]`）。

use std::net::TcpListener as StdTcpListener;
use std::path::PathBuf;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use bblbb_backend::storage::adapter::{LocalAdapter, S3Adapter, S3Config, StorageAdapter};
use bblbb_backend::storage::error::StorageError;
use bblbb_backend::storage::model::{is_safe_key, PresignedUrl};

// ─── key 安全 ───────────────────────────────────────────────────────────────

#[test]
fn safe_key_accepts_normal_paths() {
    assert!(is_safe_key("u/owner1/abc/photo.jpg"));
    assert!(is_safe_key("a/b/c"));
}

#[test]
fn safe_key_rejects_traversal_absolute_and_empty_segments() {
    assert!(!is_safe_key(""));
    assert!(!is_safe_key("../etc/passwd"));
    assert!(!is_safe_key("a/../b"));
    assert!(!is_safe_key("/abs/path"));
    assert!(!is_safe_key("a//b"));
    assert!(!is_safe_key("a\\b"), "反斜杠拒绝");
    assert!(!is_safe_key("a\0b"), "NUL 拒绝");
}

// ─── LocalAdapter contract ──────────────────────────────────────────────────

async fn local() -> (LocalAdapter, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-adapter-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    // macOS /tmp 是符号链接；canonicalize 解析到真实路径，避免误伤。
    let dir = std::fs::canonicalize(&dir).unwrap();
    let root = dir.join("uploads");
    (LocalAdapter::new(root.clone()).unwrap(), dir)
}

#[tokio::test]
async fn local_adapter_full_contract() {
    let (adapter, dir) = local().await;

    // write → head
    adapter
        .write_object("u/a/obj1", b"hello storage", Some("text/plain"))
        .await
        .unwrap();
    let head = adapter.head_object("u/a/obj1").await.unwrap();
    assert!(head.exists);
    assert_eq!(head.size_bytes, 13);

    // read 全量
    assert_eq!(
        adapter.read_object("u/a/obj1").await.unwrap(),
        b"hello storage"
    );

    // range
    assert_eq!(
        adapter.read_range("u/a/obj1", 6, 5).await.unwrap(),
        b"stora"
    );

    // copy + list
    adapter
        .copy_object("u/a/obj1", "u/b/obj1-copy")
        .await
        .unwrap();
    let listed = adapter.list_objects("u").await.unwrap();
    assert!(listed.contains(&"u/a/obj1".to_string()));
    assert!(listed.contains(&"u/b/obj1-copy".to_string()));

    // delete 后 head 不存在
    adapter.delete_object("u/a/obj1").await.unwrap();
    assert!(!adapter.head_object("u/a/obj1").await.unwrap().exists);

    // 不存在的对象 read → NotFound
    match adapter.read_object("u/nope").await {
        Err(StorageError::NotFound(_)) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }

    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn local_adapter_rejects_traversal_and_symlink() {
    let (adapter, dir) = local().await;

    // 路径穿越 key 拒绝。
    match adapter.write_object("../escape", b"x", None).await {
        Err(StorageError::Invalid(_)) => {}
        other => panic!("expected Invalid for traversal, got {other:?}"),
    }

    // 符号链接阻断：root 内建指向外部文件的 symlink。
    let outside = dir.join("secret-outside");
    std::fs::write(&outside, b"secret").unwrap();
    let root = dir.join("uploads");
    let link_path = root.join("u").join("link");
    std::fs::create_dir_all(link_path.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&outside, &link_path).unwrap();

    match adapter.read_object("u/link").await {
        Err(StorageError::Forbidden(_)) => {}
        other => panic!("expected Forbidden for symlink, got {other:?}"),
    }

    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn local_adapter_multipart_is_unsupported_by_contract() {
    // local 后端用 Rust stream 上传；multipart 为 S3 专用（contract 明确）。
    let (adapter, dir) = local().await;
    match adapter
        .begin_multipart("u/m/mp", "application/octet-stream")
        .await
    {
        Err(StorageError::Unsupported(_)) => {}
        other => panic!("expected Unsupported for local multipart, got {other:?}"),
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

// ─── S3 mock 服务器 ─────────────────────────────────────────────────────────

enum Scenario {
    /// 所有操作返回固定故障状态码 + S3 XML 错误体。
    Fault(u16),
    /// 完整 multipart 生命周期（Initiate/UploadPart/Complete/Abort）。
    Multipart,
}

fn s3_error_body(code: u16) -> (&'static str, &'static str, &'static str) {
    match code {
        403 => (
            "403",
            "Forbidden",
            "<?xml version=\"1.0\"?><Error><Code>AccessDenied</Code><Message>Access Denied</Message></Error>",
        ),
        404 => (
            "404",
            "Not Found",
            "<?xml version=\"1.0\"?><Error><Code>NoSuchKey</Code><Message>Not Found</Message></Error>",
        ),
        429 => (
            "429",
            "Too Many Requests",
            "<?xml version=\"1.0\"?><Error><Code>SlowDown</Code><Message>Slow down</Message></Error>",
        ),
        500 => (
            "500",
            "Internal Server Error",
            "<?xml version=\"1.0\"?><Error><Code>InternalError</Code><Message>Internal</Message></Error>",
        ),
        _ => ("200", "OK", ""),
    }
}

async fn spawn_mock_s3(scenario: Scenario) -> (String, tokio::sync::mpsc::Receiver<String>) {
    let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let listener = tokio::net::TcpListener::from_std(listener).unwrap();
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = match listener.accept().await {
                Ok(pair) => pair,
                Err(_) => break,
            };
            let tx = tx.clone();
            let scenario = match &scenario {
                Scenario::Fault(code) => Scenario::Fault(*code),
                Scenario::Multipart => Scenario::Multipart,
            };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = match socket.read(&mut buf).await {
                    Ok(n) if n > 0 => n,
                    _ => return,
                };
                let raw = String::from_utf8_lossy(&buf[..n]);
                let request_line = raw.lines().next().unwrap_or("").to_string();
                let _ = tx.send(request_line.clone()).await;

                let parts: Vec<&str> = request_line.split_whitespace().collect();
                let method = parts.first().copied().unwrap_or("");
                let target = parts.get(1).copied().unwrap_or("/");

                let (code, reason, body) = match &scenario {
                    Scenario::Fault(c) => {
                        let (code, reason, body) = s3_error_body(*c);
                        (code.to_string(), reason.to_string(), body.to_string())
                    }
                    Scenario::Multipart => {
                        if target.contains("uploads")
                            && method == "POST"
                            && !target.contains("uploadId")
                        {
                            // InitiateMultipartUpload
                            (
                                "200".to_string(),
                                "OK".to_string(),
                                "<?xml version=\"1.0\"?><InitiateMultipartUploadResult><UploadId>mock-upload-1</UploadId></InitiateMultipartUploadResult>".to_string(),
                            )
                        } else if target.contains("partNumber") && method == "PUT" {
                            // UploadPart
                            ("200".to_string(), "OK".to_string(), "".to_string())
                        } else if target.contains("uploadId") && method == "POST" {
                            // CompleteMultipartUpload
                            (
                                "200".to_string(),
                                "OK".to_string(),
                                "<?xml version=\"1.0\"?><CompleteMultipartUploadResult><Location>mock</Location><ETag>\"final-etag\"</ETag></CompleteMultipartUploadResult>".to_string(),
                            )
                        } else if target.contains("uploadId") && method == "DELETE" {
                            // AbortMultipartUpload
                            ("204".to_string(), "No Content".to_string(), "".to_string())
                        } else {
                            // GET/HEAD 对象读取与其余操作
                            ("200".to_string(), "OK".to_string(), "".to_string())
                        }
                    }
                };

                let mut resp = format!(
                    "HTTP/1.1 {code} {reason}\r\nContent-Length: {}\r\nETag: \"mock-etag\"\r\nContent-Type: text/plain\r\n\r\n{body}",
                    body.len()
                );
                if method == "HEAD" {
                    // HEAD 无 body。
                    resp = format!(
                        "HTTP/1.1 {code} {reason}\r\nContent-Length: 13\r\nContent-Type: text/plain\r\nETag: \"mock-etag\"\r\n\r\n"
                    );
                }
                let _ = socket.write_all(resp.as_bytes()).await;
                let _ = socket.shutdown().await;
            });
        }
    });
    (format!("http://{addr}"), rx)
}

async fn s3_with(scenario: Scenario) -> (S3Adapter, tokio::sync::mpsc::Receiver<String>) {
    let (endpoint, rx) = spawn_mock_s3(scenario).await;
    let cfg = S3Config {
        bucket: "bblbb-test".to_string(),
        region: "us-east-1".to_string(),
        endpoint: Some(endpoint),
        path_style: true,
        access_key_id: Some("AKIATEST123456".to_string()),
        secret_access_key: Some("test-secret".to_string()),
        session_token: None,
    };
    let adapter = S3Adapter::new(&cfg).await.unwrap();
    (adapter, rx)
}

#[tokio::test]
async fn s3_fault_403_maps_to_forbidden() {
    let (adapter, mut rx) = s3_with(Scenario::Fault(403)).await;
    match adapter.head_object("u/a/obj").await {
        Err(StorageError::Forbidden(_)) => {}
        other => panic!("expected Forbidden for S3 403, got {other:?}"),
    }
    assert!(rx.try_recv().is_ok(), "mock 服务器必须收到请求");
}

#[tokio::test]
async fn s3_fault_404_maps_to_not_found() {
    let (adapter, _rx) = s3_with(Scenario::Fault(404)).await;
    match adapter.head_object("u/a/obj").await {
        Err(StorageError::NotFound(_)) => {}
        other => panic!("expected NotFound for S3 404, got {other:?}"),
    }
}

#[tokio::test]
async fn s3_fault_429_maps_to_rate_limited_and_retryable() {
    let (adapter, _rx) = s3_with(Scenario::Fault(429)).await;
    let err = adapter.head_object("u/a/obj").await.unwrap_err();
    assert_eq!(
        err.code(),
        "storage_rate_limited",
        "429 → storage_rate_limited"
    );
    assert!(err.is_retryable(), "429 必须可重试");
}

#[tokio::test]
async fn s3_fault_5xx_maps_to_upstream_and_retryable() {
    let (adapter, _rx) = s3_with(Scenario::Fault(500)).await;
    let err = adapter.head_object("u/a/obj").await.unwrap_err();
    assert_eq!(
        err.code(),
        "storage_upstream_error",
        "5xx → storage_upstream_error"
    );
    assert!(err.is_retryable(), "5xx 必须可重试");
}

#[tokio::test]
async fn s3_head_returns_object_metadata() {
    let (adapter, _rx) = s3_with(Scenario::Multipart).await;
    let head = adapter.head_object("u/a/obj").await.unwrap();
    assert!(head.exists);
    assert_eq!(head.size_bytes, 13);
    let etag = head.etag.unwrap_or_default();
    assert!(etag.contains("mock-etag"), "ETag 透传：{etag}");
}

#[tokio::test]
async fn s3_multipart_lifecycle_via_mock() {
    let (adapter, mut rx) = s3_with(Scenario::Multipart).await;
    let key = "u/m/mp1";

    let upload_id = adapter
        .begin_multipart(key, "application/octet-stream")
        .await
        .unwrap();
    assert_eq!(upload_id, "mock-upload-1");

    let etag1 = adapter
        .upload_part(key, &upload_id, 1, b"part-one")
        .await
        .unwrap();
    let etag2 = adapter
        .upload_part(key, &upload_id, 2, b"part-two")
        .await
        .unwrap();

    // 升序完成。
    adapter
        .complete_multipart(key, &upload_id, &[(1, etag1), (2, etag2)])
        .await
        .unwrap();

    // 中止。
    let upload_id2 = adapter
        .begin_multipart("u/m/mp2", "application/octet-stream")
        .await
        .unwrap();
    adapter
        .upload_part("u/m/mp2", &upload_id2, 1, b"orphan")
        .await
        .unwrap();
    adapter
        .abort_multipart("u/m/mp2", &upload_id2)
        .await
        .unwrap();

    // mock 收到过 Initiate/UploadPart/Complete/Abort 请求。
    let mut saw = Vec::new();
    while let Ok(line) = rx.try_recv() {
        saw.push(line);
    }
    assert!(
        saw.iter().any(|l| l.contains("uploads")),
        "Initiate 请求: {saw:?}"
    );
    assert!(
        saw.iter().any(|l| l.contains("partNumber")),
        "UploadPart 请求: {saw:?}"
    );
    assert!(
        saw.iter()
            .any(|l| l.contains("uploadId") && l.contains("POST")),
        "Complete 请求: {saw:?}"
    );
}

#[test]
fn storage_error_classification_matrix_covers_retry_dead() {
    // 分类矩阵：403/404/429/5xx/超时/DNS-TLS/部分上传 → 稳定码 + 是否可重试。
    let cases: Vec<(StorageError, &str, bool)> = vec![
        (
            StorageError::Forbidden("x".into()),
            "storage_forbidden",
            false,
        ),
        (StorageError::Auth("x".into()), "storage_auth_failed", false),
        (StorageError::NotFound("x".into()), "not_found", false),
        (StorageError::Conflict("x".into()), "storage_conflict", true),
        (
            StorageError::RateLimited("x".into()),
            "storage_rate_limited",
            true,
        ),
        (
            StorageError::Upstream("x".into()),
            "storage_upstream_error",
            true,
        ),
        (
            StorageError::Network("x".into()),
            "storage_network_error",
            true,
        ),
        (
            StorageError::PartialUpload("x".into()),
            "storage_partial_upload",
            false,
        ),
        (
            StorageError::Verification("x".into()),
            "storage_verification_failed",
            false,
        ),
        (
            StorageError::Mismatch("x".into()),
            "storage_hash_mismatch",
            false,
        ),
        (StorageError::Quota("x".into()), "quota_exceeded", false),
        (
            StorageError::State("x".into()),
            "storage_state_error",
            false,
        ),
    ];
    for (err, expected_code, expected_retry) in cases {
        assert_eq!(err.code(), expected_code, "分类码稳定");
        assert_eq!(
            err.is_retryable(),
            expected_retry,
            "{expected_code} 重试语义"
        );
    }
}

// ─── 预签名 URL（本地签名，无网络）───────────────────────────────────────────

#[tokio::test]
async fn s3_presign_download_expiry_and_re_sign_semantics() {
    let (adapter, _rx) = s3_with(Scenario::Multipart).await;
    let now_ms = bblbb_backend::outbox::now_millis();

    let url: PresignedUrl = adapter.presign_download("u/a/obj", 60).await.unwrap();
    assert!(url.url.starts_with("http"), "预签名 URL 必须可访问");
    assert!(url.expires_at > now_ms, "expires_at 在未来");
    assert!(url.expires_at <= now_ms + 60_000 + 5_000, "TTL 约 60s");
    assert_eq!(url.method, "GET");

    // 重签：新的 expires_at 独立于对象生命周期（每次重签 TTL 前移）。
    let url2: PresignedUrl = adapter.presign_download("u/a/obj", 60).await.unwrap();
    assert!(url2.expires_at >= url.expires_at, "重签不缩短有效期");
}

#[tokio::test]
async fn s3_presign_upload_method_is_put() {
    let (adapter, _rx) = s3_with(Scenario::Multipart).await;
    let url: PresignedUrl = adapter
        .presign_upload("u/a/upload", "image/png", 120)
        .await
        .unwrap();
    assert_eq!(url.method, "PUT");
    assert!(
        url.url.contains("X-Amz-Signature"),
        "必须携带 SigV4 签名参数"
    );
}

#[test]
fn presigned_url_expiry_model() {
    let now = bblbb_backend::outbox::now_millis();
    let url = PresignedUrl {
        url: "https://mock.example/bucket/key".to_string(),
        expires_at: now + 60_000,
        method: "GET",
    };
    assert!(url.expires_at > now, "TTL 边界前有效");
    assert!(url.expires_at - now <= 60_000, "TTL 不超预算");
}
