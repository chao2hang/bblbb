//! 成就图标本地存储（管理后台配置；刻意不走 S3/附件域）。
//!
//! 设计（对应管理侧 icon 上传/删除端点，`routes/achievements.rs`）：
//! - 存储：`{storage_dir}/achievements/` 目录，文件名
//!   `icon-{code}-{sha256 前 16 hex}.{ext}`；DB `achievements.icon_path`
//!   只存该目录内的文件名（内容寻址：替换图片 = 新文件名，无陈旧缓存）。
//! - 与 M06 附件域完全解耦：不建 attachments 行、不占配额、不经
//!   `StorageService`（因此 S3 后端站点同样直写本地磁盘）——成就图标是
//!   运营配置的小型静态资源，随站点本地存储走，备份/迁移由部署方处理。
//! - 校验：魔数嗅探（png/jpeg/webp/gif）决定扩展名与响应 Content-Type，
//!   不信任请求头；大小上限 [`MAX_ICON_BYTES`]（含空文件拒绝）。
//! - 路径安全：code 在管理路由已校验为 `[a-z0-9_-]{1,64}`，ext 来自
//!   固定白名单，文件名不含任何外部输入路径段。

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// 成就图标单文件上限（2MB；远小于全局 10MB body limit，够 512px 徽章用）。
pub const MAX_ICON_BYTES: usize = 2 * 1024 * 1024;

/// 图标存储子目录名（位于 `config.storage_dir` 下）。
pub const ICON_SUBDIR: &str = "achievements";

/// 魔数嗅探结果：`(扩展名, 响应 Content-Type)`。
pub type SniffedImage = (&'static str, &'static str);

/// 按魔数嗅探图片类型（不信任 Content-Type 头）。
pub fn sniff_image(bytes: &[u8]) -> Option<SniffedImage> {
    if bytes.len() >= 8 && bytes[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        return Some(("png", "image/png"));
    }
    if bytes.len() >= 3 && bytes[..3] == [0xFF, 0xD8, 0xFF] {
        return Some(("jpg", "image/jpeg"));
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some(("webp", "image/webp"));
    }
    if bytes.len() >= 6 && (&bytes[..6] == b"GIF87a" || &bytes[..6] == b"GIF89a") {
        return Some(("gif", "image/gif"));
    }
    None
}

/// 扩展名 → 响应 Content-Type（读取端用；未知扩展返回 None）。
pub fn content_type_for_ext(ext: &str) -> Option<&'static str> {
    match ext {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

/// 图标存储目录：`{storage_dir}/achievements/`。
pub fn icon_dir(storage_dir: &Path) -> PathBuf {
    storage_dir.join(ICON_SUBDIR)
}

/// 内容寻址文件名：`icon-{code}-{sha256 前 16 hex}.{ext}`。
pub fn icon_filename(code: &str, bytes: &[u8], ext: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(code.as_bytes());
    hasher.update([0]);
    hasher.update(bytes);
    let digest = hex::encode(hasher.finalize());
    format!("icon-{code}-{}.{ext}", &digest[..16])
}

/// 从文件名提取内容哈希段（供 ETag）；解析失败返回 None。
pub fn hash_of_filename(filename: &str) -> Option<String> {
    let name = filename.strip_prefix("icon-")?;
    let stem = name.rsplit_once('.')?.0;
    let hash = stem.rsplit_once('-')?.1;
    if hash.len() == 16 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(hash.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 1, 2, 3];
    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 1];
    const GIF: &[u8] = b"GIF89a\x01\x00";
    const WEBP: &[u8] = b"RIFF\x00\x00\x00\x00WEBPVP8 ";

    #[test]
    fn sniff_detects_supported_images() {
        assert_eq!(sniff_image(PNG), Some(("png", "image/png")));
        assert_eq!(sniff_image(JPEG), Some(("jpg", "image/jpeg")));
        assert_eq!(sniff_image(GIF), Some(("gif", "image/gif")));
        assert_eq!(sniff_image(WEBP), Some(("webp", "image/webp")));
        assert_eq!(sniff_image(b"<html>"), None);
        assert_eq!(sniff_image(b""), None);
        // 前缀正确但长度不足 → 拒绝
        assert_eq!(sniff_image(&PNG[..7]), None);
    }

    #[test]
    fn filename_is_content_addressed_and_hash_roundtrips() {
        let a = icon_filename("first_post", PNG, "png");
        let b = icon_filename("first_post", PNG, "png");
        let c = icon_filename("first_post", JPEG, "jpg");
        assert_eq!(a, b, "同 code 同内容 → 同文件名");
        assert_ne!(a, c, "不同内容 → 不同文件名");
        assert!(a.starts_with("icon-first_post-") && a.ends_with(".png"));
        let hash = hash_of_filename(&a).unwrap();
        assert_eq!(hash.len(), 16);
        assert_eq!(hash_of_filename("nonsense"), None);
    }

    #[test]
    fn content_type_ext_roundtrip() {
        for (ext, ct) in [
            ("png", "image/png"),
            ("jpg", "image/jpeg"),
            ("webp", "image/webp"),
            ("gif", "image/gif"),
        ] {
            assert_eq!(content_type_for_ext(ext), Some(ct));
        }
        assert_eq!(content_type_for_ext("exe"), None);
    }
}
