//! Static frontend asset resolution, MIME mapping, and cache-control policy.

use std::fs;
use std::path::{Component, Path, PathBuf};

use rust_embed::RustEmbed;
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
        Ok(Some(build_asset(rel, bytes)))
    }
}

impl StaticAssetSource for DiskStaticAssets {
    fn resolve(&self, uri_path: &str) -> Result<Option<StaticAsset>, StaticAssetError> {
        resolve_with_reader(uri_path, |rel| self.read_checked_file(rel))
    }
}

#[derive(RustEmbed)]
#[folder = "../../web/dist"]
struct WebDistEmbed;

/// Embedded-bundle [`StaticAssetSource`] compiling `web/dist` into the server binary.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmbeddedStaticAssets;

impl EmbeddedStaticAssets {
    /// Iterates over all embedded relative asset paths (e.g. `"index.html"`, `"assets/app.js"`).
    pub fn asset_paths() -> impl Iterator<Item = std::borrow::Cow<'static, str>> {
        WebDistEmbed::iter()
    }
}

impl StaticAssetSource for EmbeddedStaticAssets {
    fn resolve(&self, uri_path: &str) -> Result<Option<StaticAsset>, StaticAssetError> {
        resolve_with_reader(uri_path, |rel| {
            Ok(WebDistEmbed::get(rel).map(|file| build_asset(rel, file.data.into_owned())))
        })
    }
}

/// No-op asset source for tests or environments that intentionally serve no static frontend.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopStaticAssets;

impl StaticAssetSource for NoopStaticAssets {
    fn resolve(&self, uri_path: &str) -> Result<Option<StaticAsset>, StaticAssetError> {
        let _ = normalize_relative_path(uri_path)?;
        Ok(None)
    }
}

fn build_asset(rel: &str, bytes: Vec<u8>) -> StaticAsset {
    StaticAsset {
        bytes,
        content_type: content_type_for_path(rel),
        cache_control: cache_control_for_path(rel),
    }
}

fn resolve_with_reader<F>(
    uri_path: &str,
    read_file: F,
) -> Result<Option<StaticAsset>, StaticAssetError>
where
    F: Fn(&str) -> Result<Option<StaticAsset>, StaticAssetError>,
{
    let rel = normalize_relative_path(uri_path)?;

    if rel == "api" || rel.starts_with("api/") || rel == "ws" || rel.starts_with("ws/") {
        return Ok(None);
    }

    if rel.is_empty() {
        return read_file("index.html");
    }

    if let Some(asset) = read_file(&rel)? {
        return Ok(Some(asset));
    }

    if rel == "assets" || rel.starts_with("assets/") {
        return Ok(None);
    }

    read_file("index.html")
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

fn is_vite_hash_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn has_vite_content_hash(rel: &str) -> bool {
    let Some(stem) = Path::new(rel).file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    if let Some((prefix, suffix)) = stem.rsplit_once('-') {
        if !prefix.is_empty() && suffix.len() >= 6 && suffix.bytes().all(is_vite_hash_char) {
            return true;
        }
    }
    // Rollup/Vite emits 8-character base64url hashes (`[A-Za-z0-9_-]{8}`) that may contain `-`.
    if stem.len() >= 10 {
        let sep_idx = stem.len() - 9;
        let hash = &stem[sep_idx + 1..];
        if stem.as_bytes()[sep_idx] == b'-'
            && hash.bytes().all(|b| is_vite_hash_char(b) || b == b'-')
            && hash
                .bytes()
                .any(|b| b.is_ascii_digit() || b.is_ascii_uppercase())
        {
            return true;
        }
    }
    false
}

fn cache_control_for_path(rel: &str) -> &'static str {
    let lower = rel.to_ascii_lowercase();
    let lower_path = Path::new(&lower);
    let is_manifest = matches!(
        lower_path.file_name().and_then(|n| n.to_str()),
        Some("manifest.json" | "manifest.webmanifest")
    );
    let is_html = matches!(
        lower_path.extension().and_then(|e| e.to_str()),
        Some("html" | "htm")
    );
    if is_html || is_manifest {
        "no-cache"
    } else if lower.starts_with("assets/") && has_vite_content_hash(rel) {
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

#[cfg(test)]
mod tests {
    use super::{EmbeddedStaticAssets, StaticAssetError, StaticAssetSource};

    #[test]
    fn embedded_assets_serve_index_hashed_assets_spa_fallback_and_block_traversal() {
        let assets = EmbeddedStaticAssets;

        // Root & /index.html -> text/html + no-cache
        for path in ["/", "/index.html"] {
            let asset = assets
                .resolve(path)
                .expect("resolve root")
                .expect("index.html must be embedded");
            assert_eq!(asset.content_type, "text/html; charset=utf-8");
            assert_eq!(asset.cache_control, "no-cache");
            assert!(
                String::from_utf8_lossy(&asset.bytes).contains("Majjj"),
                "embedded index.html should contain Majjj title"
            );
        }

        // Find at least one hashed JS asset and one hashed CSS asset from embedded bundle.
        let mut embedded_paths = EmbeddedStaticAssets::asset_paths();
        let js_asset = embedded_paths
            .find(|p| p.starts_with("assets/") && p.ends_with(".js"))
            .expect("embedded bundle must contain hashed JS asset");
        let resolved_js = assets
            .resolve(&format!("/{js_asset}"))
            .expect("resolve js")
            .expect("js asset exists");
        assert_eq!(
            resolved_js.content_type,
            "application/javascript; charset=utf-8"
        );
        assert_eq!(
            resolved_js.cache_control,
            "public, max-age=31536000, immutable"
        );
        assert!(!resolved_js.bytes.is_empty());

        let css_asset = EmbeddedStaticAssets::asset_paths()
            .find(|p| p.starts_with("assets/") && p.ends_with(".css"))
            .expect("embedded bundle must contain hashed CSS asset");
        let resolved_css = assets
            .resolve(&format!("/{css_asset}"))
            .expect("resolve css")
            .expect("css asset exists");
        assert_eq!(resolved_css.content_type, "text/css; charset=utf-8");
        assert_eq!(
            resolved_css.cache_control,
            "public, max-age=31536000, immutable"
        );

        // /api/* and /ws return None (not SPA fallback)
        assert!(assets.resolve("/api/health").unwrap().is_none());
        assert!(assets.resolve("/api/unknown").unwrap().is_none());
        assert!(assets.resolve("/ws").unwrap().is_none());

        // Missing asset under /assets/ returns None
        assert!(assets
            .resolve("/assets/missing-999999.js")
            .unwrap()
            .is_none());

        // SPA fallback for client routes
        let spa = assets
            .resolve("/lobby/seat")
            .expect("spa resolve")
            .expect("spa fallback returns index.html");
        assert_eq!(spa.cache_control, "no-cache");
        assert_eq!(spa.content_type, "text/html; charset=utf-8");

        // Traversal / backslash / null byte rejected
        for evil in [
            "/../Cargo.toml",
            "/%2e%2e/Cargo.toml",
            "/assets/%2e%2e/index.html",
            "/index.html%00.js",
            "/assets\\index.js",
        ] {
            assert!(
                matches!(assets.resolve(evil), Err(StaticAssetError::InvalidPath)),
                "expected InvalidPath for {evil}"
            );
        }
    }

    #[test]
    fn embedded_assets_serve_pwa_manifest_and_icons() {
        let assets = EmbeddedStaticAssets;
        let manifest = assets
            .resolve("/manifest.webmanifest")
            .expect("resolve manifest")
            .expect("manifest.webmanifest must be embedded");
        assert_eq!(
            manifest.content_type,
            "application/manifest+json; charset=utf-8"
        );
        assert_eq!(manifest.cache_control, "no-cache");

        let text = String::from_utf8(manifest.bytes).expect("manifest must be valid UTF-8");
        assert!(text.contains("\"standalone\""));
        let json: serde_json::Value =
            serde_json::from_str(&text).expect("manifest must be valid JSON");
        assert_eq!(json["display"], "standalone");
        assert_eq!(json["background_color"], "#102026");
        assert_eq!(json["theme_color"], "#102026");

        let icons = json["icons"]
            .as_array()
            .expect("manifest must list icons array");
        assert!(!icons.is_empty(), "manifest must include at least one icon");
        for icon in icons {
            let src = icon["src"].as_str().expect("icon src string");
            let resolved_icon = assets
                .resolve(src)
                .expect("resolve icon")
                .expect("icon file must be embedded in web/dist");
            assert_eq!(resolved_icon.content_type, "image/png");
            assert!(!resolved_icon.bytes.is_empty());
        }
    }
}
