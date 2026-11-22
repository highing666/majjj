//! Static frontend asset resolution, MIME mapping, and cache-control policy.

use std::fs;
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

/// Resolved static HTTP asset ready to be written into an HTTP response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticAsset {
    /// Raw file contents.
    pub bytes: Vec<u8>,
    /// MIME `Content-Type` header value.
    pub content_type: &'static str,
    /// `Cache-Control` header value.
    pub cache_control: &'static str,
}

/// Errors produced when resolving or validating static asset requests.
#[derive(Debug, Error)]
pub enum StaticAssetError {
    /// Request path contained traversal segments, backslashes, null bytes, or invalid encoding.
    #[error("invalid static asset path")]
    InvalidPath,
    /// Unexpected filesystem failure while reading an asset.
    #[error("static asset I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Abstraction seam for serving frontend assets from disk (Slice 2) or embedded bundles (Slice 5).
pub trait StaticAssetSource: Send + Sync {
    /// Resolves an HTTP request URI path (e.g. `"/"`, `"/assets/app-123.js"`, `"/lobby"`).
    ///
    /// Returns `Ok(None)` when the asset does not exist and no SPA fallback applies.
    ///
    /// # Errors
    ///
    /// Returns [`StaticAssetError::InvalidPath`] on traversal or malformed paths, or
    /// [`StaticAssetError::Io`] on underlying filesystem read errors.
    fn resolve(&self, uri_path: &str) -> Result<Option<StaticAsset>, StaticAssetError>;
}

/// Filesystem-backed [`StaticAssetSource`] serving a built `web/dist` directory.
#[derive(Debug, Clone)]
pub struct DiskStaticAssets {
    root: PathBuf,
}

impl DiskStaticAssets {
    /// Creates a new disk asset source rooted at `root`.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if `root` cannot be canonicalized or is not a directory.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let canonical = fs::canonicalize(root)?;
        if !canonical.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "static_dir must be a directory",
            ));
        }
        Ok(Self { root: canonical })
    }

    fn read_checked_file(&self, rel: &str) -> Result<Option<StaticAsset>, StaticAssetError> {
        let candidate = self.root.join(rel);
        let Ok(metadata) = fs::metadata(&candidate) else {
            return Ok(None);
        };
        if !metadata.is_file() {
            return Ok(None);
        }
        let canonical = fs::canonicalize(&candidate)?;
        if !canonical.starts_with(&self.root) {
            return Err(StaticAssetError::InvalidPath);
        }
        let bytes = fs::read(&canonical)?;
        let content_type = content_type_for_path(rel);
        let cache_control = cache_control_for_path(rel);
        Ok(Some(StaticAsset {
            bytes,
            content_type,
            cache_control,
        }))
    }
}

impl StaticAssetSource for DiskStaticAssets {
    fn resolve(&self, uri_path: &str) -> Result<Option<StaticAsset>, StaticAssetError> {
        let rel = normalize_relative_path(uri_path)?;

        if rel == "api" || rel.starts_with("api/") || rel == "ws" || rel.starts_with("ws/") {
            return Ok(None);
        }

        if rel.is_empty() {
            return self.read_checked_file("index.html");
        }

        if let Some(asset) = self.read_checked_file(&rel)? {
            return Ok(Some(asset));
        }

        if rel == "assets" || rel.starts_with("assets/") {
            return Ok(None);
        }

        self.read_checked_file("index.html")
    }
}

/// No-op asset source used when `--static-dir` is not configured.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopStaticAssets;

impl StaticAssetSource for NoopStaticAssets {
    fn resolve(&self, uri_path: &str) -> Result<Option<StaticAsset>, StaticAssetError> {
        let _ = normalize_relative_path(uri_path)?;
        Ok(None)
    }
}

fn normalize_relative_path(uri_path: &str) -> Result<String, StaticAssetError> {
    let raw_no_query = uri_path.split(['?', '#']).next().unwrap_or("");
    if contains_unsafe_raw_markers(raw_no_query) {
        return Err(StaticAssetError::InvalidPath);
    }

    let decoded = percent_decode_utf8(raw_no_query)?;
    if decoded.contains('\0') || decoded.contains('\\') {
        return Err(StaticAssetError::InvalidPath);
    }

    let trimmed = decoded.trim_start_matches('/');
    let mut parts = Vec::new();
    for comp in Path::new(trimmed).components() {
        match comp {
            Component::Normal(seg) => {
                let s = seg.to_str().ok_or(StaticAssetError::InvalidPath)?;
                if s == ".." || s == "." {
                    return Err(StaticAssetError::InvalidPath);
                }
                parts.push(s.to_owned());
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(StaticAssetError::InvalidPath);
            }
        }
    }

    Ok(parts.join("/"))
}

fn contains_unsafe_raw_markers(raw: &str) -> bool {
    raw.contains('\0') || raw.contains('\\') || raw.split('/').any(|seg| seg == "..")
}

fn percent_decode_utf8(input: &str) -> Result<String, StaticAssetError> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(StaticAssetError::InvalidPath);
            }
            let hi = hex_val(bytes[i + 1]).ok_or(StaticAssetError::InvalidPath)?;
            let lo = hex_val(bytes[i + 2]).ok_or(StaticAssetError::InvalidPath)?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| StaticAssetError::InvalidPath)
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn cache_control_for_path(rel: &str) -> &'static str {
    let lower = rel.to_ascii_lowercase();
    if lower.ends_with(".html")
        || lower.ends_with(".htm")
        || lower == "manifest.json"
        || lower == "manifest.webmanifest"
    {
        "no-cache"
    } else if lower.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    }
}

fn content_type_for_path(rel: &str) -> &'static str {
    let ext = Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("webmanifest") => "application/manifest+json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("m4a") => "audio/mp4",
        Some("mp3") => "audio/mpeg",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    }
}
