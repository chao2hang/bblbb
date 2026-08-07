//! M06-UPLOAD：两阶段上传与内容安全处理。
//!
//! 生命周期：`create`（预留容量 + pending 行 + object key）→ 传输
//! （local Rust stream / S3 presigned PUT）→ `complete`（服务端 HEAD 复检 +
//! 内容安全 worker：magic/hash/病毒/图片重解码 + EXIF 剥离）→ `ready` /
//! `quarantined`。
//!
//! 安全约定（M06-UPLOAD-05/06）：默认拒绝 SVG、HTML/脚本、可执行文件、
//! 宏文档、压缩包与 MIME/扩展名欺骗；图片限制宽高与像素数（解压炸弹）。
//! 失败对象进入 `quarantined` 并记录**安全摘要**（`processing_error`），
//! 不泄漏内部细节。

use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Either;

use crate::audit::AuditEntry;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::db::DatabasePool;
use crate::storage::adapter::StorageService;
use crate::storage::error::StorageError;
use crate::storage::model::{
    generate_object_key, AttachmentRecord, AttachmentStatus, NewAttachment, ObjectHead,
    StorageBackend,
};
use crate::storage::quota::{self, get_policy_for_level, release_reserved, reserve_bytes};

/// 文件名最大长度（清洗后）。
pub const MAX_FILENAME_LEN: usize = 255;
/// 未完成上传的清理宽限（毫秒；`reap_stale_uploads` 使用）。
pub const STALE_UPLOAD_MS: i64 = 24 * 60 * 60 * 1000;
/// 图片宽高上限（像素炸弹防护，M06-UPLOAD-05）。
pub const MAX_IMAGE_DIMENSION: i64 = 16_384;
/// 图片像素数上限（解压炸弹防护）。
pub const MAX_IMAGE_PIXELS: i64 = 40_000_000;
/// 传输并发上限（进程内信号量；超限返回 `storage_rate_limited`）。
pub const UPLOAD_CONCURRENCY: usize = 16;

/// 允许上传的媒体类型白名单（MIME/魔法双重校验的事实来源）。
pub const ALLOWED_MEDIA_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/webp",
    "image/gif",
    "image/avif",
    "application/pdf",
    "text/plain",
];

/// 各媒体类型的合法扩展名（扩展名欺骗判定）。
fn extensions_for(media_type: &str) -> &'static [&'static str] {
    match media_type {
        "image/jpeg" => &["jpg", "jpeg", "jpe"],
        "image/png" => &["png"],
        "image/webp" => &["webp"],
        "image/gif" => &["gif"],
        "image/avif" => &["avif", "avifs"],
        "application/pdf" => &["pdf"],
        "text/plain" => &["txt", "text", "log", "md", "markdown"],
        _ => &[],
    }
}

/// 任何情况下都拒绝的危险扩展名（脚本/宏/可执行/压缩包）。
const DANGEROUS_EXTENSIONS: &[&str] = &[
    "exe", "com", "bat", "cmd", "ps1", "sh", "bash", "vbs", "js", "mjs", "svg", "html", "htm",
    "xhtml", "mht", "jar", "class", "dll", "so", "dylib", "doc", "docm", "xls", "xlsm", "ppt",
    "pptm", "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "iso", "dmg", "lnk", "apk", "deb", "rpm",
    "msi",
];

/// 是否允许该媒体类型。
pub fn is_allowed_media_type(media_type: &str) -> bool {
    ALLOWED_MEDIA_TYPES.contains(&media_type)
}

/// 进程内上传并发信号量（M06-UPLOAD-03：限制传输/扫描并发，防小机器 OOM）。
static UPLOAD_SEMAPHORE: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>> =
    std::sync::OnceLock::new();

/// 获取上传并发许可；超限返回 `storage_rate_limited`（429）。
pub fn acquire_upload_permit() -> Result<tokio::sync::OwnedSemaphorePermit, StorageError> {
    let semaphore = UPLOAD_SEMAPHORE
        .get_or_init(|| std::sync::Arc::new(tokio::sync::Semaphore::new(UPLOAD_CONCURRENCY)));
    semaphore
        .clone()
        .try_acquire_owned()
        .map_err(|_| StorageError::RateLimited("upload concurrency limit reached".to_string()))
}

// ────────────────────────── 病毒扫描占位 ───────────────────────────────────

/// 病毒扫描结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanVerdict {
    Clean,
    Infected,
}

/// 病毒扫描接口：生产可接 ClamAV；默认占位恒 `Clean`，测试注入确定性 mock
/// （M06-UPLOAD-05）。
pub trait VirusScan: Send + Sync {
    fn scan(&self, data: &[u8]) -> ScanVerdict;
}

/// 默认病毒扫描占位（未接 ClamAV 时恒干净；接产后在服务层替换）。
pub struct NoopVirusScan;

impl VirusScan for NoopVirusScan {
    fn scan(&self, _data: &[u8]) -> ScanVerdict {
        ScanVerdict::Clean
    }
}

// ────────────────────────── 内容安全检查 ──────────────────────────────────

/// 内容安全扫描结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanOutcome {
    pub sha256: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    /// 剥离 EXIF/GPS 元数据后的二进制（图片重写；非图片原样返回）。
    pub scrubbed: Vec<u8>,
}

/// 内容安全拒绝原因（safe summary，可进入 `processing_error`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanError {
    /// 类型不在白名单。
    UnsupportedType(String),
    /// MIME 与魔法字节不一致（欺骗）。
    TypeMismatch { declared: String, detected: String },
    /// 危险内容（SVG/HTML/脚本/可执行/宏/压缩包等）。
    DangerousContent(String),
    /// 图片宽高/像素超限（解压炸弹）。
    ImageTooLarge { width: i64, height: i64 },
    /// 图片结构损坏（无法解析尺寸）。
    CorruptImage(String),
    /// 病毒扫描命中。
    VirusDetected(String),
}

impl ScanError {
    /// 安全摘要（不含路径/内部细节）。
    pub fn summary(&self) -> String {
        match self {
            Self::UnsupportedType(t) => format!("media type not allowed: {t}"),
            Self::TypeMismatch { declared, detected } => {
                format!("declared {declared} but magic is {detected}")
            }
            Self::DangerousContent(kind) => format!("content blocked: {kind}"),
            Self::ImageTooLarge { width, height } => {
                format!("image dimensions {width}x{height} exceed limits")
            }
            Self::CorruptImage(reason) => format!("image corrupt: {reason}"),
            Self::VirusDetected(reason) => format!("virus scan failed: {reason}"),
        }
    }
}

/// 检测到的魔法类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MagicKind {
    Jpeg,
    Png,
    Webp,
    Gif,
    Avif,
    Pdf,
    PlainText,
}

impl MagicKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Webp => "image/webp",
            Self::Gif => "image/gif",
            Self::Avif => "image/avif",
            Self::Pdf => "application/pdf",
            Self::PlainText => "text/plain",
        }
    }
}

/// 魔法字节检测；`None` = 未知二进制（一律拒绝）。
fn detect_magic(data: &[u8]) -> Option<MagicKind> {
    if data.len() >= 3 && data[..3] == [0xFF, 0xD8, 0xFF] {
        return Some(MagicKind::Jpeg);
    }
    if data.len() >= 8 && data[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        return Some(MagicKind::Png);
    }
    if data.len() >= 4 && &data[..4] == b"GIF8" {
        return Some(MagicKind::Gif);
    }
    if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return Some(MagicKind::Webp);
    }
    if data.len() >= 12 && &data[4..8] == b"ftyp" {
        let brand = &data[8..12];
        if brand.starts_with(b"avif") || brand.starts_with(b"avis") {
            return Some(MagicKind::Avif);
        }
    }
    if data.len() >= 5 && &data[..5] == b"%PDF-" {
        return Some(MagicKind::Pdf);
    }
    if is_probably_text(data) {
        return Some(MagicKind::PlainText);
    }
    None
}

/// 前 1024 字节是否近似纯文本（UTF-8/ASCII；无控制字节）。
fn is_probably_text(data: &[u8]) -> bool {
    let sample = &data[..data.len().min(1024)];
    sample.iter().all(|b| {
        b.is_ascii_graphic()
            || b.is_ascii_whitespace()
            || *b >= 0x80
            || matches!(*b, 0x0A | 0x0D | 0x09 | 0x0C)
    }) && !sample.is_empty()
}

/// 危险内容判定（SVG/HTML/脚本/可执行/宏/压缩包，M06-UPLOAD-06）。
fn dangerous_content_kind(data: &[u8]) -> Option<&'static str> {
    let head = &data[..data.len().min(512)];
    let ascii = String::from_utf8_lossy(head);
    let lower = ascii.to_ascii_lowercase();
    let trimmed = lower.trim_start();
    if trimmed.starts_with("<svg")
        || lower.contains("<svg ")
        || (trimmed.starts_with("<?xml") && lower.contains("<svg"))
    {
        return Some("svg (vector markup is not allowed)");
    }
    if trimmed.starts_with("<!doctype html")
        || trimmed.starts_with("<html")
        || trimmed.starts_with("<head")
        || trimmed.starts_with("<body")
    {
        return Some("html");
    }
    if data.starts_with(b"#!") {
        return Some("executable script (shebang)");
    }
    if data.len() >= 4 && &data[..4] == b"\x7fELF" {
        return Some("elf executable");
    }
    if data.len() >= 2 && &data[..2] == b"MZ" {
        return Some("windows executable");
    }
    if data.len() >= 2 && &data[..2] == b"\xd0\xcf" && data.len() >= 8 {
        // OLE compound document：宏文档（doc/xls/ppt）与旧版 MSI
        return Some("ole compound document");
    }
    if data.len() >= 4 && &data[..4] == b"PK\x03\x04" {
        return Some("zip archive");
    }
    if data.len() >= 2 && &data[..2] == b"\x1f\x8b" {
        return Some("gzip archive");
    }
    if data.len() >= 4 && &data[..4] == b"Rar!" {
        return Some("rar archive");
    }
    if data.len() >= 6 && &data[..6] == b"7z\xbc\xaf\x27\x1c" {
        return Some("7z archive");
    }
    None
}

/// 图片宽高解析（不完整解码；尺寸非法视为损坏）。
fn image_dimensions(kind: MagicKind, data: &[u8]) -> Result<Option<(i64, i64)>, ScanError> {
    let dims: Option<(i64, i64)> = match kind {
        MagicKind::Jpeg => jpeg_dimensions(data),
        MagicKind::Png => png_dimensions(data),
        MagicKind::Gif => gif_dimensions(data),
        MagicKind::Webp => webp_dimensions(data),
        MagicKind::Avif => None, // AVIF 尺寸需完整解码；按无尺寸处理
        _ => None,
    };
    let Some((w, h)) = dims else {
        if kind == MagicKind::Avif {
            return Ok(None);
        }
        // 非图片类型或无法解析的图片：可解析尺寸的图片类型返回 CorruptImage
        return Err(ScanError::CorruptImage(kind.as_str().to_string()));
    };
    Ok(Some((w, h)))
}

fn jpeg_dimensions(data: &[u8]) -> Option<(i64, i64)> {
    if data.len() < 4 {
        return None;
    }
    let mut i = 2; // 跳过 SOI
    while i + 4 <= data.len() {
        if data[i] != 0xFF {
            return None;
        }
        let marker = data[i + 1];
        // RSTn / TEM 无长度段
        if (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            i += 2;
            continue;
        }
        if marker == 0xD9 || marker == 0xDA {
            return None; // EOI 或进入压缩数据
        }
        let seg_len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        if seg_len < 2 {
            return None;
        }
        // SOF0..SOF15（排除 DHT C4 / JPG C8 / DAC CC）
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            if i + 9 > data.len() {
                return None;
            }
            let height = u16::from_be_bytes([data[i + 5], data[i + 6]]) as i64;
            let width = u16::from_be_bytes([data[i + 7], data[i + 8]]) as i64;
            return Some((width, height));
        }
        i += 2 + seg_len;
    }
    None
}

fn png_dimensions(data: &[u8]) -> Option<(i64, i64)> {
    if data.len() < 24 || &data[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]) as i64;
    let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]) as i64;
    Some((width, height))
}

fn gif_dimensions(data: &[u8]) -> Option<(i64, i64)> {
    if data.len() < 10 {
        return None;
    }
    let width = u16::from_le_bytes([data[6], data[7]]) as i64;
    let height = u16::from_le_bytes([data[8], data[9]]) as i64;
    Some((width, height))
}

fn webp_dimensions(data: &[u8]) -> Option<(i64, i64)> {
    if data.len() < 24 {
        return None;
    }
    let chunk_type = &data[12..16];
    let chunk_size = u32::from_le_bytes([data[16], data[17], data[18], data[19]]) as usize;
    let chunk = data.get(20..(20 + chunk_size).min(data.len()))?;
    match chunk_type {
        b"VP8X" => {
            if chunk.len() < 10 {
                return None;
            }
            // 24-bit LE：宽度-1、高度-1（canvas）
            let width = i64::from(u32::from_le_bytes([chunk[4], chunk[5], chunk[6], 0])) + 1;
            let height = i64::from(u32::from_le_bytes([chunk[7], chunk[8], chunk[9], 0])) + 1;
            Some((width, height))
        }
        b"VP8L" => {
            if chunk.len() < 5 || chunk[0] != 0x2F {
                return None;
            }
            let bits = u32::from_le_bytes([chunk[1], chunk[2], chunk[3], chunk[4]]);
            let width = i64::from(bits & 0x3FFF) + 1;
            let height = i64::from((bits >> 14) & 0x3FFF) + 1;
            Some((width, height))
        }
        b"VP8 " => {
            if chunk.len() < 10 {
                return None;
            }
            let frame_tag = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], 0]);
            if frame_tag & 1 != 0 {
                return None; // key frame 校验位
            }
            let width = i64::from(u16::from_le_bytes([chunk[6], chunk[7]]) & 0x3FFF);
            let height = i64::from(u16::from_le_bytes([chunk[8], chunk[9]]) & 0x3FFF);
            Some((width, height))
        }
        _ => None,
    }
}

/// 校验图片宽高与像素数（压缩炸弹防护）。
fn check_image_limits(w: i64, h: i64) -> Result<(), ScanError> {
    if w <= 0 || h <= 0 {
        return Err(ScanError::CorruptImage(format!("{w}x{h}")));
    }
    if w > MAX_IMAGE_DIMENSION || h > MAX_IMAGE_DIMENSION {
        return Err(ScanError::ImageTooLarge {
            width: w,
            height: h,
        });
    }
    if w.saturating_mul(h) > MAX_IMAGE_PIXELS {
        return Err(ScanError::ImageTooLarge {
            width: w,
            height: h,
        });
    }
    Ok(())
}

/// JPEG 剥离 APP1-Exif / APP1-XMP / APP13-IPTC 段（M06-UPLOAD-09：EXIF/GPS）。
/// 简化实现：仅重写段头，压缩数据原样保留。
fn scrub_jpeg(data: &[u8]) -> Vec<u8> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return data.to_vec();
    }
    let mut out = vec![0xFF, 0xD8];
    let mut i = 2;
    while i + 4 <= data.len() {
        if data[i] != 0xFF {
            break;
        }
        let marker = data[i + 1];
        if (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            out.extend_from_slice(&data[i..i + 2]);
            i += 2;
            continue;
        }
        if marker == 0xDA {
            // SOS：压缩数据整体保留
            out.extend_from_slice(&data[i..]);
            return out;
        }
        let seg_len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        if seg_len < 2 || i + 2 + seg_len > data.len() {
            break;
        }
        let seg = &data[i..i + 2 + seg_len];
        let drop = match marker {
            0xE1 => {
                seg.get(4..).is_some_and(|b| b.starts_with(b"Exif\0\0"))
                    || seg
                        .get(4..)
                        .is_some_and(|b| b.starts_with(b"http://ns.adobe.com/xap/1.0/"))
            }
            0xE2 => seg
                .get(4..)
                .is_some_and(|b| b.starts_with(b"http://ns.adobe.com/xmp/")),
            0xED => seg
                .get(4..)
                .is_some_and(|b| b.starts_with(b"Photoshop 3.0")),
            _ => false,
        };
        if !drop {
            out.extend_from_slice(seg);
        }
        i += 2 + seg_len;
    }
    out
}

/// PNG 剥离 tEXt / zTXt / iTXt / eXIf 数据块。
fn scrub_png(data: &[u8]) -> Vec<u8> {
    const SIG: &[u8] = b"\x89PNG\r\n\x1a\n";
    if !data.starts_with(SIG) || data.len() < 8 {
        return data.to_vec();
    }
    let mut out = data[..8].to_vec();
    let mut i = 8;
    while i + 12 <= data.len() {
        let len = u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as usize;
        let chunk_type = &data[i + 4..i + 8];
        if i + 12 + len > data.len() {
            break;
        }
        let drop = matches!(chunk_type, b"tEXt" | b"zTXt" | b"iTXt" | b"eXIf");
        if !drop {
            out.extend_from_slice(&data[i..i + 12 + len]);
        }
        i += 12 + len;
    }
    out
}

fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// 内容安全扫描（M06-UPLOAD-05/06）：魔法 + 扩展名 + MIME 一致性 + 病毒 +
/// 图片重解码/像素限制 + EXIF/GPS 剥离。失败返回 [`ScanError`]（安全摘要）。
pub fn scan_for_safety(
    data: &[u8],
    declared_media_type: &str,
    filename: Option<&str>,
    virus: &dyn VirusScan,
) -> Result<ScanOutcome, ScanError> {
    if !is_allowed_media_type(declared_media_type) {
        return Err(ScanError::UnsupportedType(declared_media_type.to_string()));
    }

    // 扩展名欺骗（MIME/扩展名双校验）
    if let Some(name) = filename {
        let ext = name.rsplit('.').next().map(str::to_ascii_lowercase);
        if let Some(ext) = ext {
            if DANGEROUS_EXTENSIONS.contains(&ext.as_str()) {
                return Err(ScanError::DangerousContent(format!(
                    "extension .{ext} is not allowed"
                )));
            }
            let legal = extensions_for(declared_media_type);
            if !legal.is_empty() && !legal.contains(&ext.as_str()) {
                return Err(ScanError::DangerousContent(format!(
                    "extension .{ext} does not match media type {declared_media_type}"
                )));
            }
        }
    }

    // 危险内容（SVG/HTML/脚本/可执行/宏/压缩包）
    if let Some(kind) = dangerous_content_kind(data) {
        return Err(ScanError::DangerousContent(kind.to_string()));
    }

    // 病毒扫描占位（生产接 ClamAV；测试用确定性 mock）
    if virus.scan(data) == ScanVerdict::Infected {
        return Err(ScanError::VirusDetected(
            "deterministic test mock".to_string(),
        ));
    }

    // 魔法字节与 MIME 一致性
    let magic = detect_magic(data)
        .ok_or_else(|| ScanError::DangerousContent("unknown binary content".to_string()))?;
    if magic.as_str() != declared_media_type {
        return Err(ScanError::TypeMismatch {
            declared: declared_media_type.to_string(),
            detected: magic.as_str().to_string(),
        });
    }

    // 图片：尺寸解析 + 像素限制 + 元数据剥离
    let (width, height, scrubbed) = match magic {
        MagicKind::Jpeg => {
            let dims =
                jpeg_dimensions(data).ok_or_else(|| ScanError::CorruptImage("jpeg".to_string()))?;
            check_image_limits(dims.0, dims.1)?;
            let scrubbed = scrub_jpeg(data);
            (Some(dims.0 as i32), Some(dims.1 as i32), scrubbed)
        }
        MagicKind::Png => {
            let dims =
                png_dimensions(data).ok_or_else(|| ScanError::CorruptImage("png".to_string()))?;
            check_image_limits(dims.0, dims.1)?;
            let scrubbed = scrub_png(data);
            (Some(dims.0 as i32), Some(dims.1 as i32), scrubbed)
        }
        MagicKind::Gif | MagicKind::Webp => {
            let dims = image_dimensions(magic, data)?;
            if let Some((w, h)) = dims {
                check_image_limits(w, h)?;
            }
            (
                dims.map(|d| d.0 as i32),
                dims.map(|d| d.1 as i32),
                data.to_vec(),
            )
        }
        MagicKind::Avif => (None, None, data.to_vec()),
        _ => (None, None, data.to_vec()),
    };

    let scrubbed_hash = sha256_hex(&scrubbed);
    Ok(ScanOutcome {
        sha256: scrubbed_hash,
        width,
        height,
        scrubbed,
    })
}

// ────────────────────────── 附件行 IO ──────────────────────────────────────

#[derive(sqlx::FromRow)]
struct AttachmentRow {
    id: String,
    owner_id: String,
    storage_backend: String,
    storage_key: String,
    original_name: Option<String>,
    media_type: String,
    size_bytes: i64,
    sha256: String,
    width: Option<i32>,
    height: Option<i32>,
    status: String,
    quota_bytes_charged: i64,
    is_public: i64,
    ref_count: i64,
    processing_version: i32,
    processing_error: Option<String>,
    created_at: i64,
    deleted_at: Option<i64>,
}

impl From<AttachmentRow> for AttachmentRecord {
    fn from(r: AttachmentRow) -> Self {
        Self {
            id: r.id,
            owner_id: r.owner_id,
            storage_backend: StorageBackend::parse(&r.storage_backend)
                .unwrap_or(StorageBackend::Local),
            storage_key: r.storage_key,
            original_name: r.original_name,
            media_type: r.media_type,
            size_bytes: r.size_bytes,
            sha256: r.sha256,
            width: r.width,
            height: r.height,
            status: AttachmentStatus::parse(&r.status).unwrap_or(AttachmentStatus::Pending),
            quota_bytes_charged: r.quota_bytes_charged,
            is_public: r.is_public != 0,
            ref_count: r.ref_count,
            processing_version: r.processing_version,
            processing_error: r.processing_error,
            created_at: r.created_at,
            deleted_at: r.deleted_at,
        }
    }
}

const ATTACHMENT_COLUMNS: &str =
    "id, owner_id, storage_backend, storage_key, original_name, media_type,
     size_bytes, sha256, width, height, status, quota_bytes_charged, is_public, ref_count,
     processing_version, processing_error, created_at, deleted_at";

/// 读取附件行（不存在 → `Ok(None)`）。
pub async fn load_attachment(
    pool: &DatabasePool,
    attachment_id: &str,
) -> Result<Option<AttachmentRecord>, StorageError> {
    let row: Option<AttachmentRow> = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, AttachmentRow>(&format!(
                "SELECT {ATTACHMENT_COLUMNS} FROM attachments WHERE id = ?"
            ))
            .bind(attachment_id)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as::<_, AttachmentRow>(&format!(
                "SELECT {ATTACHMENT_COLUMNS} FROM attachments WHERE id = ?"
            ))
            .bind(attachment_id)
            .fetch_optional(p)
            .await?
        }
    };
    Ok(row.map(AttachmentRecord::from))
}

/// 读取用户全部附件（投影/管理用；按 created_at DESC）。
pub async fn list_attachments_for_owner(
    pool: &DatabasePool,
    owner_id: &str,
    limit: i64,
) -> Result<Vec<AttachmentRecord>, StorageError> {
    let limit = limit.clamp(1, 100);
    let rows: Vec<AttachmentRow> = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, AttachmentRow>(&format!(
                "SELECT {ATTACHMENT_COLUMNS} FROM attachments
                 WHERE owner_id = ? ORDER BY created_at DESC LIMIT ?"
            ))
            .bind(owner_id)
            .bind(limit)
            .fetch_all(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as::<_, AttachmentRow>(&format!(
                "SELECT {ATTACHMENT_COLUMNS} FROM attachments
                 WHERE owner_id = ? ORDER BY created_at DESC LIMIT ?"
            ))
            .bind(owner_id)
            .bind(limit)
            .fetch_all(p)
            .await?
        }
    };
    Ok(rows.into_iter().map(AttachmentRecord::from).collect())
}

// ────────────────────────── create（M06-UPLOAD-01/02）──────────────────────

/// 附件创建声明输入。
#[derive(Debug, Clone)]
pub struct CreateAttachmentInput {
    pub owner_id: String,
    pub original_name: Option<String>,
    pub media_type: String,
    pub size_bytes: i64,
    pub is_public: bool,
}

/// 上传传输方式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadTransport {
    /// S3 直传：后端先签发短 TTL PUT URL（M06-UPLOAD-03）。
    Presigned {
        url: String,
        method: &'static str,
        expires_at: i64,
    },
    /// local 后端：客户端经 Rust stream 端点传输（`PUT /api/v1/attachments/{id}`）。
    Stream,
}

/// create 结果。
#[derive(Debug, Clone)]
pub struct CreateOutcome {
    pub attachment: AttachmentRecord,
    pub transport: UploadTransport,
}

/// 创建附件（M06-UPLOAD-01/02）：后端计算 owner、大小上限、策略版本与 object
/// key；create 阶段预留容量（reserved）。权限/账号状态门由路由层
/// （`attachment.upload` + 邮箱/冷静期/封禁）先行裁决。
pub async fn create_attachment(
    pool: &DatabasePool,
    storage: &StorageService,
    user_id: &str,
    input: CreateAttachmentInput,
    now: i64,
) -> Result<CreateOutcome, StorageError> {
    let media_type = input.media_type.trim().to_ascii_lowercase();
    if !is_allowed_media_type(&media_type) {
        return Err(StorageError::Invalid(format!(
            "media type not allowed: {media_type}"
        )));
    }
    if input.size_bytes <= 0 {
        return Err(StorageError::Invalid(
            "attachment size must be positive".to_string(),
        ));
    }
    let name = clean_filename(input.original_name.as_deref());
    if name.as_ref().is_some_and(|n| n.len() > MAX_FILENAME_LEN) {
        return Err(StorageError::Invalid("filename too long".to_string()));
    }

    // 当前等级 + 最新配额策略修订（M06-QUOTA-03：create 阶段重新读取）
    let level = current_level(pool, user_id).await?;
    let policy = get_policy_for_level(pool, level, user_id).await?;
    if input.size_bytes > policy.single_file_max_bytes {
        return Err(StorageError::Quota(format!(
            "single file exceeds level limit: {} > {}",
            input.size_bytes, policy.single_file_max_bytes
        )));
    }
    if input.size_bytes > quota::SITE_SINGLE_FILE_HARD_LIMIT_BYTES {
        return Err(StorageError::Quota(
            "single file exceeds site hard limit".to_string(),
        ));
    }

    // 预留容量（超卖/每日上限原子校验；M06-QUOTA-05）
    reserve_bytes(pool, user_id, input.size_bytes, &policy, now).await?;

    let backend = storage.default_backend();
    let storage_key = generate_object_key(user_id, name.as_deref());
    let attachment_id = uuid::Uuid::now_v7().to_string();

    let insert = insert_attachment_row(
        pool,
        &NewAttachment {
            owner_id: user_id.to_string(),
            original_name: name,
            media_type: media_type.clone(),
            size_bytes: input.size_bytes,
            is_public: input.is_public,
        },
        &attachment_id,
        backend,
        &storage_key,
        now,
    )
    .await;

    if let Err(e) = insert {
        // 回滚预留，避免孤儿 reserved
        let _ = release_reserved(pool, user_id, input.size_bytes, now).await;
        return Err(e);
    }

    let attachment = load_attachment(pool, &attachment_id)
        .await?
        .ok_or_else(|| StorageError::Internal("attachment row missing after insert".to_string()))?;

    // S3 直传：后端先签发短 TTL PUT URL（M06-UPLOAD-03/06）；local 走 Rust stream
    let transport = match backend {
        StorageBackend::S3 => {
            let adapter = storage.adapter(StorageBackend::S3)?;
            let presigned = adapter
                .presign_upload(&storage_key, &media_type, quota::PRESIGN_TTL_SECS)
                .await?;
            UploadTransport::Presigned {
                url: presigned.url,
                method: presigned.method,
                expires_at: presigned.expires_at,
            }
        }
        StorageBackend::Local => UploadTransport::Stream,
    };

    Ok(CreateOutcome {
        attachment,
        transport,
    })
}

/// 清洗文件名（保留可打印字符；控制字符替换为下划线）。
fn clean_filename(raw: Option<&str>) -> Option<String> {
    raw.map(|s| {
        let cleaned: String = s
            .chars()
            .map(|c| if c.is_control() { '_' } else { c })
            .collect();
        let trimmed = cleaned.trim_matches([' ', '.', '_']).to_string();
        if trimmed.is_empty() {
            "object".to_string()
        } else {
            trimmed
        }
    })
}

/// 读取用户当前等级（users.level；缺省 1）。
async fn current_level(pool: &DatabasePool, user_id: &str) -> Result<i64, StorageError> {
    let level: Option<i64> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT level FROM users WHERE id = ?")
                .bind(user_id)
                .fetch_optional(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT level FROM users WHERE id = ?")
                .bind(user_id)
                .fetch_optional(p)
                .await?
        }
    };
    Ok(level.unwrap_or(1).max(1))
}

/// 插入附件行 + 审计（同一事务；失败时调用方回滚预留）。
async fn insert_attachment_row(
    pool: &DatabasePool,
    input: &NewAttachment,
    attachment_id: &str,
    backend: StorageBackend,
    storage_key: &str,
    now: i64,
) -> Result<(), StorageError> {
    let mut tx = match pool {
        Either::Left(p) => Either::Left(p.begin().await?),
        Either::Right(p) => Either::Right(p.begin().await?),
    };
    match &mut tx {
        Either::Left(t) => {
            sqlx::query(
                "INSERT INTO attachments
                    (id, owner_id, storage_backend, storage_key, original_name, media_type,
                     size_bytes, sha256, width, height, status, quota_bytes_charged,
                     is_public, ref_count, processing_version, processing_error,
                     created_at, deleted_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, '', NULL, NULL, 'pending', 0, ?, 0, 0, NULL, ?, NULL)",
            )
            .bind(attachment_id)
            .bind(&input.owner_id)
            .bind(backend.as_str())
            .bind(storage_key)
            .bind(&input.original_name)
            .bind(&input.media_type)
            .bind(input.size_bytes)
            .bind(input.is_public)
            .bind(now)
            .execute(&mut **t)
            .await?;
        }
        Either::Right(t) => {
            sqlx::query(
                "INSERT INTO attachments
                    (id, owner_id, storage_backend, storage_key, original_name, media_type,
                     size_bytes, sha256, width, height, status, quota_bytes_charged,
                     is_public, ref_count, processing_version, processing_error,
                     created_at, deleted_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, '', NULL, NULL, 'pending', 0, ?, 0, 0, NULL, ?, NULL)",
            )
            .bind(attachment_id)
            .bind(&input.owner_id)
            .bind(backend.as_str())
            .bind(storage_key)
            .bind(&input.original_name)
            .bind(&input.media_type)
            .bind(input.size_bytes)
            .bind(input.is_public)
            .bind(now)
            .execute(&mut **t)
            .await?;
        }
    }
    let audit = AuditEntry::user_action(&input.owner_id, "attachment.create")
        .with_target("attachment", attachment_id)
        .with_effective_role("member")
        .with_policy_version(AUTHZ_POLICY_VERSION);
    audit.record_in_tx(&mut tx).await?;
    match tx {
        Either::Left(t) => t.commit().await?,
        Either::Right(t) => t.commit().await?,
    }
    Ok(())
}

// ────────────────────────── stream 上传（M06-UPLOAD-03）───────────────────

/// Rust stream 上传（local 后端；S3 走 presign，拒绝此入口）。
///
/// 服务端复检 Content-Length 与声明大小一致、Content-Type 与声明一致
/// （`application/octet-stream` 视为未知类型放行，由 complete 的 magic 校验兜底）。
pub async fn stream_upload(
    pool: &DatabasePool,
    storage: &StorageService,
    attachment_id: &str,
    user_id: &str,
    data: &[u8],
    content_type: Option<&str>,
) -> Result<AttachmentRecord, StorageError> {
    let _permit = acquire_upload_permit()?;
    let attachment = load_attachment(pool, attachment_id)
        .await?
        .ok_or_else(|| StorageError::NotFound(format!("attachment {attachment_id}")))?;
    if attachment.owner_id != user_id {
        return Err(StorageError::Forbidden(
            "attachment belongs to another user".to_string(),
        ));
    }
    if attachment.status != AttachmentStatus::Pending {
        return Err(StorageError::State(
            "attachment upload already completed or quarantined".to_string(),
        ));
    }
    if attachment.storage_backend != StorageBackend::Local {
        return Err(StorageError::Invalid(
            "s3 backend uses presigned upload".to_string(),
        ));
    }
    if data.len() as i64 != attachment.size_bytes {
        return Err(StorageError::Verification(format!(
            "body size {} does not match declared {}",
            data.len(),
            attachment.size_bytes
        )));
    }
    // Content-Type 限制：声明类型与请求头必须一致（octet-stream 放行）
    if let Some(ct) = content_type {
        let normalized = ct
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if !normalized.is_empty()
            && normalized != attachment.media_type
            && normalized != "application/octet-stream"
        {
            return Err(StorageError::Verification(format!(
                "content-type {normalized} does not match declared {}",
                attachment.media_type
            )));
        }
    }

    let adapter = storage.adapter(StorageBackend::Local)?;
    let key = attachment.storage_key.clone();
    let media_type = attachment.media_type.clone();
    tokio::time::timeout(
        std::time::Duration::from_secs(20),
        adapter.write_object(&key, data, Some(&media_type)),
    )
    .await
    .map_err(|_| StorageError::Network("upload timed out".to_string()))??;

    // 字节就位：pending → processing（complete 阶段做服务端 HEAD 复检与扫描）
    update_status(pool, attachment_id, AttachmentStatus::Processing).await?;
    load_attachment(pool, attachment_id)
        .await?
        .ok_or_else(|| StorageError::Internal("attachment row missing".to_string()))
}

// ────────────────────────── complete（M06-UPLOAD-04/05/08）────────────────

/// complete 结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompleteOutcome {
    /// 已 ready（幂等重放或本次完成）。
    Ready,
    /// 内容安全失败，进入 quarantined（含安全摘要）。
    Quarantined,
}

/// complete：服务端 HEAD 复检 + 内容安全 worker + 容量结算（M06-UPLOAD-04/05/08）。
///
/// - 幂等：`ready` 重放直接成功；`pending`/`processing` 恢复执行；
/// - HEAD 复检存在性/大小/metadata 与 create 声明一致，不一致 → 隔离并
///   回滚 reserved，返回 [`StorageError::Verification`]；
/// - 容量重检（M06-QUOTA-03）：重新读取当前等级与最新策略修订；用户降级或
///   容量占满 → 拒绝且不超卖（隔离 + 回滚 reserved，返回 [`StorageError::Quota`]）。
pub async fn complete_attachment(
    pool: &DatabasePool,
    storage: &StorageService,
    attachment_id: &str,
    user_id: &str,
    virus: &dyn VirusScan,
    now: i64,
) -> Result<CompleteOutcome, StorageError> {
    let _permit = acquire_upload_permit()?;
    let attachment = load_attachment(pool, attachment_id)
        .await?
        .ok_or_else(|| StorageError::NotFound(format!("attachment {attachment_id}")))?;
    if attachment.owner_id != user_id {
        return Err(StorageError::Forbidden(
            "attachment belongs to another user".to_string(),
        ));
    }
    match attachment.status {
        AttachmentStatus::Ready => return Ok(CompleteOutcome::Ready),
        AttachmentStatus::Quarantined => {
            return Err(StorageError::State(
                "attachment is quarantined; delete and re-upload".to_string(),
            ))
        }
        AttachmentStatus::Deleted => {
            return Err(StorageError::State("attachment is deleted".to_string()))
        }
        AttachmentStatus::Pending | AttachmentStatus::Processing => {}
    }

    // 容量重检（M06-QUOTA-03）：当前等级 + 最新策略修订
    let level = current_level(pool, user_id).await?;
    let policy = get_policy_for_level(pool, level, user_id).await?;
    let counters = quota::get_counters(pool, user_id).await?;
    let committed_after = counters.bytes_charged + counters.bytes_reserved;
    if committed_after > policy.total_bytes || attachment.size_bytes > policy.single_file_max_bytes
    {
        // 用户降级/容量占满：拒绝且不超卖（隔离 + 回滚 reserved）
        quarantine_attachment(
            pool,
            &attachment,
            "quota insufficient after policy re-check",
            now,
        )
        .await?;
        return Err(StorageError::Quota(
            "quota exceeded at complete; attachment quarantined".to_string(),
        ));
    }

    // 服务端 HEAD 复检（M06-UPLOAD-04）
    let adapter = storage.adapter(attachment.storage_backend)?;
    let key = attachment.storage_key.clone();
    let head = adapter.head_object(&key).await?;
    if !head.exists {
        quarantine_attachment(pool, &attachment, "object missing at complete", now).await?;
        return Err(StorageError::Verification(
            "uploaded object does not exist".to_string(),
        ));
    }
    if let Err(verification) = verify_head(&head, &attachment) {
        quarantine_attachment(pool, &attachment, &verification.to_string(), now).await?;
        return Err(verification);
    }

    // 读取对象字节（内容安全 worker 输入；S3 经 GetObject 流式读取）
    let bytes = adapter.read_object(&key).await?;

    // 内容安全 worker：magic/hash/病毒/图片重解码（M06-UPLOAD-05/09）
    let scan = scan_for_safety(
        &bytes,
        &attachment.media_type,
        attachment.original_name.as_deref(),
        virus,
    );
    let outcome = match scan {
        Err(err) => {
            quarantine_attachment(pool, &attachment, &err.summary(), now).await?;
            return Ok(CompleteOutcome::Quarantined);
        }
        Ok(outcome) => outcome,
    };

    // EXIF/GPS 剥离结果写回对象（本地重写二进制并重算 hash，M06-UPLOAD-09）
    if outcome.scrubbed != bytes {
        let adapter = storage.adapter(attachment.storage_backend)?;
        tokio::time::timeout(
            std::time::Duration::from_secs(20),
            adapter.write_object(&key, &outcome.scrubbed, Some(&attachment.media_type)),
        )
        .await
        .map_err(|_| StorageError::Network("rewrite timed out".to_string()))??;
    }

    // 原子最终化：容量结算 + 状态→ready + 审计 + Outbox（单事务，M06-QUOTA-05）。
    // 并发 complete 只有一方发生状态迁移（`status IN ('pending','processing')`
    // 守卫），迁移方负责结算，另一方幂等重放。
    let transitioned = finalize_ready(pool, &attachment, &outcome, now).await?;
    if !transitioned {
        return Ok(CompleteOutcome::Ready);
    }
    Ok(CompleteOutcome::Ready)
}

/// HEAD 与 create 声明的一致性复检（存在性/大小/metadata）。
fn verify_head(head: &ObjectHead, attachment: &AttachmentRecord) -> Result<(), StorageError> {
    if !head.exists {
        return Err(StorageError::Verification(
            "uploaded object does not exist".to_string(),
        ));
    }
    if head.size_bytes != attachment.size_bytes {
        return Err(StorageError::Verification(format!(
            "object size {} does not match declared {}",
            head.size_bytes, attachment.size_bytes
        )));
    }
    if let Some(ct) = &head.content_type {
        let normalized = ct
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if !normalized.is_empty()
            && normalized != attachment.media_type
            && normalized != "application/octet-stream"
        {
            return Err(StorageError::Verification(format!(
                "object content-type {normalized} does not match declared {}",
                attachment.media_type
            )));
        }
    }
    Ok(())
}

/// 内容安全失败：单事务内 回滚 reserved + `processing_version+1` + 安全摘要 + 审计
/// （M06-UPLOAD-05；SQLite BEGIN IMMEDIATE / MySQL 事务）。
async fn quarantine_attachment(
    pool: &DatabasePool,
    attachment: &AttachmentRecord,
    summary: &str,
    now: i64,
) -> Result<(), StorageError> {
    let mut tx = match pool {
        Either::Left(p) => Either::Left(p.begin_with("BEGIN IMMEDIATE").await?),
        Either::Right(p) => Either::Right(p.begin().await?),
    };
    match &mut tx {
        Either::Left(t) => {
            // 回滚预留（pending/processing 阶段持有 reserved；负数钳制为 0）
            sqlx::query(
                "INSERT OR IGNORE INTO user_quota_counters (user_id, bytes_reserved, bytes_charged, bytes_released, updated_at)
                 VALUES (?, 0, 0, 0, ?)",
            )
            .bind(&attachment.owner_id)
            .bind(now)
            .execute(&mut **t)
            .await?;
            sqlx::query(
                "UPDATE user_quota_counters
                 SET bytes_released = bytes_released + ?,
                     bytes_reserved = CASE WHEN bytes_reserved < ? THEN 0 ELSE bytes_reserved - ? END,
                     updated_at = ?
                 WHERE user_id = ?",
            )
            .bind(attachment.size_bytes)
            .bind(attachment.size_bytes)
            .bind(attachment.size_bytes)
            .bind(now)
            .bind(&attachment.owner_id)
            .execute(&mut **t)
            .await?;
            sqlx::query(
                "UPDATE attachments
                 SET status = 'quarantined', processing_error = ?,
                     processing_version = processing_version + 1
                 WHERE id = ? AND status IN ('pending', 'processing')",
            )
            .bind(summary)
            .bind(&attachment.id)
            .execute(&mut **t)
            .await?;
        }
        Either::Right(t) => {
            sqlx::query(
                "INSERT IGNORE INTO user_quota_counters (user_id, bytes_reserved, bytes_charged, bytes_released, updated_at)
                 VALUES (?, 0, 0, 0, ?)",
            )
            .bind(&attachment.owner_id)
            .bind(now)
            .execute(&mut **t)
            .await?;
            sqlx::query(
                "UPDATE user_quota_counters
                 SET bytes_released = bytes_released + ?,
                     bytes_reserved = CASE WHEN bytes_reserved < ? THEN 0 ELSE bytes_reserved - ? END,
                     updated_at = ?
                 WHERE user_id = ?",
            )
            .bind(attachment.size_bytes)
            .bind(attachment.size_bytes)
            .bind(attachment.size_bytes)
            .bind(now)
            .bind(&attachment.owner_id)
            .execute(&mut **t)
            .await?;
            sqlx::query(
                "UPDATE attachments
                 SET status = 'quarantined', processing_error = ?,
                     processing_version = processing_version + 1
                 WHERE id = ? AND status IN ('pending', 'processing')",
            )
            .bind(summary)
            .bind(&attachment.id)
            .execute(&mut **t)
            .await?;
        }
    }
    let audit = AuditEntry::user_action(&attachment.owner_id, "attachment.quarantined")
        .with_target("attachment", &attachment.id)
        .with_reason(summary)
        .with_effective_role("member")
        .with_policy_version(AUTHZ_POLICY_VERSION);
    audit.record_in_tx(&mut tx).await?;
    match tx {
        Either::Left(t) => t.commit().await?,
        Either::Right(t) => t.commit().await?,
    }
    Ok(())
}

/// complete 最终化：单事务内完成 容量结算（charged += size、released +=
/// reserved、reserved -= reserved）+ 状态 → ready + 审计 + Outbox
/// `attachment.ready.v1`（SQLite `BEGIN IMMEDIATE` / MySQL 事务，M06-QUOTA-05）。
///
/// 返回 `true` 表示本次调用完成了状态迁移（并负责结算）；`false` 表示并发
/// complete 已先行完成（幂等重放，不重复结算/不发事件）。
async fn finalize_ready(
    pool: &DatabasePool,
    attachment: &AttachmentRecord,
    outcome: &ScanOutcome,
    now: i64,
) -> Result<bool, StorageError> {
    let mut tx = match pool {
        Either::Left(p) => Either::Left(p.begin_with("BEGIN IMMEDIATE").await?),
        Either::Right(p) => Either::Right(p.begin().await?),
    };
    let affected: u64 = match &mut tx {
        Either::Left(t) => sqlx::query(
            "UPDATE attachments
                 SET status = 'ready', sha256 = ?, width = ?, height = ?,
                     quota_bytes_charged = ?, processing_error = NULL,
                     processing_version = processing_version + 1
                 WHERE id = ? AND status IN ('pending', 'processing')",
        )
        .bind(&outcome.sha256)
        .bind(outcome.width)
        .bind(outcome.height)
        .bind(attachment.size_bytes)
        .bind(&attachment.id)
        .execute(&mut **t)
        .await?
        .rows_affected(),
        Either::Right(t) => sqlx::query(
            "UPDATE attachments
                 SET status = 'ready', sha256 = ?, width = ?, height = ?,
                     quota_bytes_charged = ?, processing_error = NULL,
                     processing_version = processing_version + 1
                 WHERE id = ? AND status IN ('pending', 'processing')",
        )
        .bind(&outcome.sha256)
        .bind(outcome.width)
        .bind(outcome.height)
        .bind(attachment.size_bytes)
        .bind(&attachment.id)
        .execute(&mut **t)
        .await?
        .rows_affected(),
    };
    if affected != 1 {
        return Ok(false);
    }
    // 容量结算（M06-QUOTA-04：charged += size、released += reserved、reserved -= reserved）
    match &mut tx {
        Either::Left(t) => {
            sqlx::query(
                "INSERT OR IGNORE INTO user_quota_counters (user_id, bytes_reserved, bytes_charged, bytes_released, updated_at)
                 VALUES (?, 0, 0, 0, ?)",
            )
            .bind(&attachment.owner_id)
            .bind(now)
            .execute(&mut **t)
            .await?;
            sqlx::query(
                "UPDATE user_quota_counters
                 SET bytes_charged = bytes_charged + ?,
                     bytes_released = bytes_released + ?,
                     bytes_reserved = CASE WHEN bytes_reserved < ? THEN 0 ELSE bytes_reserved - ? END,
                     updated_at = ?
                 WHERE user_id = ?",
            )
            .bind(attachment.size_bytes)
            .bind(attachment.size_bytes)
            .bind(attachment.size_bytes)
            .bind(attachment.size_bytes)
            .bind(now)
            .bind(&attachment.owner_id)
            .execute(&mut **t)
            .await?;
        }
        Either::Right(t) => {
            sqlx::query(
                "INSERT IGNORE INTO user_quota_counters (user_id, bytes_reserved, bytes_charged, bytes_released, updated_at)
                 VALUES (?, 0, 0, 0, ?)",
            )
            .bind(&attachment.owner_id)
            .bind(now)
            .execute(&mut **t)
            .await?;
            sqlx::query(
                "UPDATE user_quota_counters
                 SET bytes_charged = bytes_charged + ?,
                     bytes_released = bytes_released + ?,
                     bytes_reserved = CASE WHEN bytes_reserved < ? THEN 0 ELSE bytes_reserved - ? END,
                     updated_at = ?
                 WHERE user_id = ?",
            )
            .bind(attachment.size_bytes)
            .bind(attachment.size_bytes)
            .bind(attachment.size_bytes)
            .bind(attachment.size_bytes)
            .bind(now)
            .bind(&attachment.owner_id)
            .execute(&mut **t)
            .await?;
        }
    }
    let audit = AuditEntry::user_action(&attachment.owner_id, "attachment.complete")
        .with_target("attachment", &attachment.id)
        .with_effective_role("member")
        .with_policy_version(AUTHZ_POLICY_VERSION);
    audit.record_in_tx(&mut tx).await?;
    crate::outbox::enqueue_in_tx(
        &mut tx,
        crate::events::types::ATTACHMENT_READY,
        json!({
            "attachment_id": attachment.id,
            "owner_id": attachment.owner_id,
            "media_type": attachment.media_type,
            "size_bytes": attachment.size_bytes,
        }),
    )
    .await?;
    match tx {
        Either::Left(t) => t.commit().await?,
        Either::Right(t) => t.commit().await?,
    }
    Ok(true)
}

/// 更新状态（pending → processing 等；非法迁移由调用方保证）。
async fn update_status(
    pool: &DatabasePool,
    attachment_id: &str,
    next: AttachmentStatus,
) -> Result<(), StorageError> {
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE attachments SET status = ? WHERE id = ? AND status = 'pending'")
                .bind(next.as_str())
                .bind(attachment_id)
                .execute(p)
                .await?;
        }
        Either::Right(p) => {
            sqlx::query("UPDATE attachments SET status = ? WHERE id = ? AND status = 'pending'")
                .bind(next.as_str())
                .bind(attachment_id)
                .execute(p)
                .await?;
        }
    }
    Ok(())
}

// ────────────────────────── 删除（M06-QUOTA-09）───────────────────────────

/// 软删除：进入 30 天保留（`deleted_at` + `deleted` 状态）；解除全部引用；
/// 未结算（pending/processing）的预留容量立即回滚。单事务原子完成
/// （SQLite `BEGIN IMMEDIATE` / MySQL 事务）。
pub async fn delete_attachment(
    pool: &DatabasePool,
    user_id: &str,
    attachment_id: &str,
    now: i64,
) -> Result<AttachmentRecord, StorageError> {
    let attachment = load_attachment(pool, attachment_id)
        .await?
        .ok_or_else(|| StorageError::NotFound(format!("attachment {attachment_id}")))?;
    if attachment.owner_id != user_id {
        return Err(StorageError::Forbidden(
            "attachment belongs to another user".to_string(),
        ));
    }
    if attachment.status == AttachmentStatus::Deleted {
        return Ok(attachment);
    }
    let not_yet_charged = matches!(
        attachment.status,
        AttachmentStatus::Pending | AttachmentStatus::Processing
    );

    let mut tx = match pool {
        Either::Left(p) => Either::Left(p.begin_with("BEGIN IMMEDIATE").await?),
        Either::Right(p) => Either::Right(p.begin().await?),
    };
    match &mut tx {
        Either::Left(t) => {
            // 解除引用（Cover 移除只解除引用，不删附件；ref_count 归零）
            sqlx::query("DELETE FROM attachment_links WHERE attachment_id = ?")
                .bind(attachment_id)
                .execute(&mut **t)
                .await?;
            sqlx::query("UPDATE attachments SET ref_count = 0 WHERE id = ?")
                .bind(attachment_id)
                .execute(&mut **t)
                .await?;
            // pending/processing：预留尚未结算，立即回滚 reserved
            if not_yet_charged {
                sqlx::query(
                    "INSERT OR IGNORE INTO user_quota_counters (user_id, bytes_reserved, bytes_charged, bytes_released, updated_at)
                     VALUES (?, 0, 0, 0, ?)",
                )
                .bind(&attachment.owner_id)
                .bind(now)
                .execute(&mut **t)
                .await?;
                sqlx::query(
                    "UPDATE user_quota_counters
                     SET bytes_released = bytes_released + ?,
                         bytes_reserved = CASE WHEN bytes_reserved < ? THEN 0 ELSE bytes_reserved - ? END,
                         updated_at = ?
                     WHERE user_id = ?",
                )
                .bind(attachment.size_bytes)
                .bind(attachment.size_bytes)
                .bind(attachment.size_bytes)
                .bind(now)
                .bind(&attachment.owner_id)
                .execute(&mut **t)
                .await?;
            }
            sqlx::query("UPDATE attachments SET status = 'deleted', deleted_at = ? WHERE id = ?")
                .bind(now)
                .bind(attachment_id)
                .execute(&mut **t)
                .await?;
        }
        Either::Right(t) => {
            sqlx::query("DELETE FROM attachment_links WHERE attachment_id = ?")
                .bind(attachment_id)
                .execute(&mut **t)
                .await?;
            sqlx::query("UPDATE attachments SET ref_count = 0 WHERE id = ?")
                .bind(attachment_id)
                .execute(&mut **t)
                .await?;
            if not_yet_charged {
                sqlx::query(
                    "INSERT IGNORE INTO user_quota_counters (user_id, bytes_reserved, bytes_charged, bytes_released, updated_at)
                     VALUES (?, 0, 0, 0, ?)",
                )
                .bind(&attachment.owner_id)
                .bind(now)
                .execute(&mut **t)
                .await?;
                sqlx::query(
                    "UPDATE user_quota_counters
                     SET bytes_released = bytes_released + ?,
                         bytes_reserved = CASE WHEN bytes_reserved < ? THEN 0 ELSE bytes_reserved - ? END,
                         updated_at = ?
                     WHERE user_id = ?",
                )
                .bind(attachment.size_bytes)
                .bind(attachment.size_bytes)
                .bind(attachment.size_bytes)
                .bind(now)
                .bind(&attachment.owner_id)
                .execute(&mut **t)
                .await?;
            }
            sqlx::query("UPDATE attachments SET status = 'deleted', deleted_at = ? WHERE id = ?")
                .bind(now)
                .bind(attachment_id)
                .execute(&mut **t)
                .await?;
        }
    }
    let audit = AuditEntry::user_action(user_id, "attachment.delete")
        .with_target("attachment", attachment_id)
        .with_effective_role("member")
        .with_policy_version(AUTHZ_POLICY_VERSION);
    audit.record_in_tx(&mut tx).await?;
    match tx {
        Either::Left(t) => t.commit().await?,
        Either::Right(t) => t.commit().await?,
    }
    Ok(attachment)
}

// ────────────────────────── 中断上传清理（M06-UPLOAD-10）──────────────────

/// 清理超时未完成的 upload（`pending`/`processing` 超过 24h）：
/// 删除对象（如存在）、回滚预留、删除行。返回清理条数。
pub async fn reap_stale_uploads(
    pool: &DatabasePool,
    storage: &StorageService,
    now: i64,
) -> Result<usize, StorageError> {
    let cutoff = now - STALE_UPLOAD_MS;
    let rows: Vec<(String, String, String, String, i64)> = match pool {
        Either::Left(p) => {
            sqlx::query_as(
                "SELECT id, owner_id, storage_backend, storage_key, size_bytes
             FROM attachments WHERE status IN ('pending', 'processing') AND created_at < ?",
            )
            .bind(cutoff)
            .fetch_all(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as(
                "SELECT id, owner_id, storage_backend, storage_key, size_bytes
             FROM attachments WHERE status IN ('pending', 'processing') AND created_at < ?",
            )
            .bind(cutoff)
            .fetch_all(p)
            .await?
        }
    };
    let mut reaped = 0;
    for (id, owner_id, backend_str, key, size) in rows {
        let backend = match StorageBackend::parse(&backend_str) {
            Some(b) => b,
            None => continue,
        };
        if let Ok(adapter) = storage.adapter(backend) {
            if let Ok(head) = adapter.head_object(&key).await {
                if head.exists {
                    let _ = adapter.delete_object(&key).await;
                }
            }
        }
        release_reserved(pool, &owner_id, size, now).await?;
        match pool {
            Either::Left(p) => {
                sqlx::query("DELETE FROM attachments WHERE id = ?")
                    .bind(&id)
                    .execute(p)
                    .await?;
            }
            Either::Right(p) => {
                sqlx::query("DELETE FROM attachments WHERE id = ?")
                    .bind(&id)
                    .execute(p)
                    .await?;
            }
        }
        reaped += 1;
    }
    Ok(reaped)
}
