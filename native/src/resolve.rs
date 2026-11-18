//! Content URI -> local file, for `rur render` output directories (story_render.rs), the desktop app's
//! asset worker (assets.rs) and the Node CLI (`rur-native assets fetch`, download.rs).
//!
//! `rbxasset://` comes from the local Studio install, `rbxassetid://N` (and `roblox.com/asset/?id=N`,
//! bare ids) from the asset cache (`<state dir>/assets/<id>.<ext>` when its download record matches
//! it, offline any such file; a cached decal / texture model resolves to the image it references),
//! `rbxthumb://` from its cached WebP rendition, `file://` from its file (the PNG snapshot the engine
//! emulator writes of an EditableImage a script shows). What is not local is downloaded (download.rs,
//! the desktop build; the web build only has its bundle). ViewportFrame images (`rur-viewport://`)
//! come from the 3D worker (viewports.rs, the desktop build).
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::SystemTime,
};

use serde_json::Value;

/// A resolved URI: the local file, or why there is none.
pub type Resolution = Result<PathBuf, String>;

pub struct Resolver {
    repo: PathBuf,
    assets: PathBuf,
    offline: bool,
    content_roots: Option<Vec<PathBuf>>,
    /// The snapshots of the ViewportFrame images being resolved, by URI
    viewports: Value,
    /// Whether ViewportFrame pictures are kept for later processes (one-shot renders); the desktop app
    /// draws them anew as they change and keeps none
    persist_viewports: bool,
    /// The 3D worker that draws ViewportFrames (viewports.rs)
    #[cfg(feature = "desktop")]
    worker: crate::viewports::ViewportWorker,
    /// Downloads, the account and the studio read (download.rs)
    #[cfg(feature = "desktop")]
    downloads: crate::download::Downloader,
}

/// The extensions a cache entry can have, in the order cli/assets.mjs cachedFile looked for them
pub(crate) const EXTENSIONS: [&str; 12] = [
    ".png", ".jpg", ".gif", ".webp", ".bmp", ".ttf", ".otf", ".ttc", ".json", ".rbxm", ".rbxmx", ".bin",
];

impl Resolver {
    pub fn new(repo: impl Into<PathBuf>, offline: bool) -> Self {
        let repo = repo.into();
        let assets = crate::state_dir(&repo).join("assets");
        Self {
            #[cfg(feature = "desktop")]
            downloads: crate::download::Downloader::new(&repo),
            #[cfg(feature = "desktop")]
            worker: crate::viewports::ViewportWorker::new(&repo, offline),
            repo,
            assets,
            offline,
            content_roots: None,
            viewports: Value::Null,
            persist_viewports: true,
        }
    }

    pub fn set_offline(&mut self, offline: bool) {
        #[cfg(feature = "desktop")]
        self.worker.set_offline(offline);
        self.offline = offline;
    }

    /// The desktop app's ViewportFrames: drawn anew as they change, kept by no later process
    pub fn draw_viewports_live(&mut self) {
        self.persist_viewports = false;
    }

    /// Forgets the ViewportFrame pictures of the slots no longer shown (the desktop app)
    pub fn retain_viewports(&mut self, slots: &[u64]) {
        #[cfg(feature = "desktop")]
        self.worker.retain(slots);
        #[cfg(not(feature = "desktop"))]
        let _ = slots;
    }

    /// The snapshots `uris`' ViewportFrame images are drawn from (the desktop app's job carries them)
    pub fn set_viewport_snapshots(&mut self, snapshots: Value) {
        self.viewports = snapshots;
    }

    /// How long one ViewportFrame picture may take before the 3D worker is stopped
    pub fn set_viewport_timeout(&mut self, timeout: std::time::Duration) {
        #[cfg(feature = "desktop")]
        self.worker.set_timeout(timeout);
        #[cfg(not(feature = "desktop"))]
        let _ = timeout;
    }

    /// The 3D worker's warnings since the last call (what it drew, but not as Studio would)
    pub fn take_viewport_warnings(&mut self) -> Vec<String> {
        #[cfg(feature = "desktop")]
        return self.worker.take_warnings();
        #[cfg(not(feature = "desktop"))]
        Vec::new()
    }

    /// Resolves every URI (in order, duplicates once): local answers first, then the downloads, then
    /// the ViewportFrame images from the 3D worker.
    pub fn resolve_all(&mut self, uris: &[String]) -> BTreeMap<String, Resolution> {
        let mut results = BTreeMap::new();
        let mut seen = HashSet::new();
        let (mut remote, mut viewports) = (Vec::new(), Vec::new());
        for uri in uris {
            if !seen.insert(uri.as_str()) {
                continue;
            }
            if is_viewport(uri) {
                viewports.push(uri.clone());
                continue;
            }
            match self.local(uri) {
                Some(result) => {
                    results.insert(uri.clone(), result);
                }
                None => remote.push(uri.clone()),
            }
        }
        results.extend(self.download(&remote));
        results.extend(self.viewport_images(&viewports));
        results
    }

    pub fn resolve_scene(&mut self, scene: &Value, uris: &[String]) -> BTreeMap<String, Resolution> {
        self.viewports = crate::assets::viewport_requests(scene["nodes"].as_array().into_iter().flatten(), uris);
        let result = self.resolve_all(uris);
        self.viewports = Value::Null;
        result
    }

    /// Downloads the cloud fonts a run reported missing (scene.diagnostics); true when any arrived,
    /// so the run is repeated with them. Offline, and in the web build, nothing is downloaded.
    pub fn fetch_fonts(&mut self, diagnostics: &Value) -> Result<bool, String> {
        if self.offline {
            return Ok(false);
        }
        #[cfg(feature = "desktop")]
        {
            Ok(self.downloads.fetch_fonts(diagnostics))
        }
        #[cfg(not(feature = "desktop"))]
        {
            let _ = diagnostics;
            Ok(false)
        }
    }

    /// Whether private assets can be asked for, and how: "broker", "cookie", or None (public assets only)
    #[cfg(feature = "desktop")]
    pub fn account_access(&self) -> Option<&'static str> {
        self.downloads.account_access()
    }

    /// The downloader, for the `assets` command (`--no-studio`)
    #[cfg(feature = "desktop")]
    pub(crate) fn downloads_mut(&mut self) -> &mut crate::download::Downloader {
        &mut self.downloads
    }

    /// The URIs `local` could not answer: downloaded (download.rs), or, without a downloader (the web
    /// build, which never downloads), not cached
    fn download(&mut self, uris: &[String]) -> BTreeMap<String, Resolution> {
        if uris.is_empty() {
            return BTreeMap::new();
        }
        #[cfg(feature = "desktop")]
        {
            self.downloads.resolve_all(uris, self.offline).into_iter().map(|(uri, result)| (uri, result.map(|fetched| fetched.file))).collect()
        }
        #[cfg(not(feature = "desktop"))]
        {
            uris.iter().map(|uri| (uri.clone(), Err("offline and not cached".to_string()))).collect()
        }
    }

    /// ViewportFrame images (`rur-viewport://`), drawn by the 3D worker from their snapshots. A worker
    /// that can't be had fails every frame with the same reason, found once.
    fn viewport_images(&mut self, uris: &[String]) -> BTreeMap<String, Resolution> {
        let mut results = BTreeMap::new();
        #[cfg(feature = "desktop")]
        {
            let mut unavailable: Option<String> = None;
            for uri in uris {
                if let Some(reason) = &unavailable {
                    results.insert(uri.clone(), Err(reason.clone()));
                    continue;
                }
                let snapshot = self.viewports.get(uri.as_str()).cloned().unwrap_or(Value::Null);
                let result = self.worker.render(uri, &snapshot, self.persist_viewports);
                if let Err(error) = &result {
                    if crate::viewports::is_unavailable(error) {
                        unavailable = Some(error.clone());
                    }
                }
                results.insert(uri.clone(), result);
            }
        }
        #[cfg(not(feature = "desktop"))]
        for uri in uris {
            results.insert(uri.clone(), Err("ViewportFrames are not drawn in the browser build".into()));
        }
        results
    }

    /// The local answer for `uri`, or None when it needs a download (or the 3D worker).
    fn local(&mut self, uri: &str) -> Option<Resolution> {
        let trimmed = uri.trim();
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("rur-viewport://") {
            return None;
        }
        if lower.starts_with("rbxasset://") {
            let relative = &trimmed["rbxasset://".len()..];
            return Some(self.rbxasset(relative).ok_or_else(|| format!("not in the local Studio install: {uri}")));
        }
        if lower.starts_with("file://") {
            let path = file_path(trimmed);
            return Some(if path.is_file() { Ok(path) } else { Err(format!("no such file: {uri}")) });
        }
        if lower.starts_with("rbxthumb://") {
            let key = ThumbnailQuery::parse(&trimmed["rbxthumb://".len()..]).key();
            return match cached_file(&self.assets, &key) {
                // A cache entry holding the UnapprovedImage icon (a Blocked thumbnail cached before the placeholder
                // rule) is left to the downloader, which draws Studio's placeholder (download.rs fetch_thumbnail)
                Some(file) if unapproved_icon(&file) => None,
                Some(file) if !file.to_string_lossy().ends_with(".bin") => Some(Ok(file)),
                Some(_) => None,
                None if self.offline => Some(Err("offline and not cached".into())),
                None => None,
            };
        }
        match asset_id(trimmed) {
            Some(id) => self.asset(&id, true),
            None => Some(Err(format!("unsupported content uri: {uri}"))),
        }
    }

    /// A cached asset id (fetch_asset_id without the network): a model resolves to the texture it names.
    fn asset(&mut self, id: &str, allow_decal: bool) -> Option<Resolution> {
        let Some(file) = cached_file(&self.assets, id) else {
            return self.offline.then(|| Err("offline and not cached".into()));
        };
        // A pre-unwrap .bin entry is migrated by the downloader; a font is recorded in the font index there
        let name = file.to_string_lossy().to_ascii_lowercase();
        if name.ends_with(".bin") || [".ttf", ".otf", ".ttc"].iter().any(|ext| name.ends_with(ext)) {
            return None;
        }
        let bytes = std::fs::read(&file).ok()?;
        // A file rur did not download under this name is downloaded again
        if !self.offline && !downloaded(&file, &bytes) {
            return None;
        }
        if allow_decal && sniff(&bytes).kind == "model" {
            let text: String = bytes.iter().map(|&byte| byte as char).collect();
            let Some(inner) = model_texture(&text) else {
                return Some(Err(format!("asset {id} is a model without an image texture")));
            };
            if inner == id {
                return Some(Err(format!("asset {id} is a model without an image texture")));
            }
            return self.asset(&inner, false);
        }
        Some(Ok(file))
    }

    /// cli/assets.mjs resolveRbxasset: content/, ExtraContent/, PlatformContent/pc/ of the newest
    /// Studio install (RUR_ROBLOX_CONTENT's parent first)
    fn rbxasset(&mut self, relative: &str) -> Option<PathBuf> {
        let roots = self.content_roots.get_or_insert_with(content_roots);
        for root in roots.iter() {
            for base in ["content", "ExtraContent", "PlatformContent/pc"] {
                let file = root.join(base).join(relative);
                if file.exists() {
                    return Some(file);
                }
            }
        }
        None
    }
}

/// A ViewportFrame image (`rur-viewport://<slot>/<snapshot digest>`), drawn by the 3D worker
pub(crate) fn is_viewport(uri: &str) -> bool {
    uri.starts_with("rur-viewport://")
}

/// The cache file of `key` (an asset id or a thumbnail key) in `assets`, by cli/assets.mjs's extension order
pub(crate) fn cached_file(assets: &Path, key: &str) -> Option<PathBuf> {
    EXTENSIONS.iter().map(|ext| assets.join(format!("{key}{ext}"))).find(|file| file.is_file())
}

/// The download record of a cache file: `assets/downloaded/<file name>.json`
pub(crate) fn record_file(file: &Path) -> PathBuf {
    let name = file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    file.parent().unwrap_or(Path::new(".")).join("downloaded").join(format!("{name}.json"))
}

/// Whether `file` holds what rur downloaded under its name: its sha256 and size in
/// `assets/downloaded/<file name>.json`. A file is hashed once per length and modification time (per
/// process: every resolver and download thread shares the answers).
///
/// What rur downloaded under each cache name is recorded because a file under an asset's name is not
/// that asset because it is there: the cache is a plain folder, and on 2026-09-30 agents filled it with
/// stand-ins for assets this renderer could not download (thumbnails.roblox.com 700 x 700 and 420 x 420
/// renditions cropped to their alpha bounds, and render directories' copies of them), which every later
/// render drew as the asset (a header plate 7% narrower for its height, its art out of place). A cached
/// file without a matching record is downloaded again before it is used and replaced when its bytes
/// differ; offline it is used as it is.
pub(crate) fn downloaded(file: &Path, bytes: &[u8]) -> bool {
    type Stamp = (u64, Option<SystemTime>);
    static VERIFIED: OnceLock<Mutex<HashMap<PathBuf, Stamp>>> = OnceLock::new();
    let verified = VERIFIED.get_or_init(Default::default);
    let stamp = std::fs::metadata(file).map(|metadata| (metadata.len(), metadata.modified().ok())).ok();
    if stamp.is_some() && verified.lock().ok().and_then(|map| map.get(file).cloned()) == stamp {
        return true;
    }
    let Some(record) = std::fs::read(record_file(file)).ok().and_then(|text| serde_json::from_slice::<Value>(&text).ok()) else {
        return false;
    };
    let matches = record["size"].as_u64() == Some(bytes.len() as u64)
        && record["sha256"].as_str() == Some(crate::engine::text::hinted::sha256_hex(bytes).as_str());
    if let (true, Some(stamp), Ok(mut map)) = (matches, stamp, verified.lock()) {
        map.insert(file.to_path_buf(), stamp);
    }
    matches
}

/// cli/assets.mjs filePathFromUri: a `file://` URI's path (`file:///C:/x` and `file://C:/x`, `%xx`
/// escapes decoded)
fn file_path(uri: &str) -> PathBuf {
    let rest = &uri["file://".len()..];
    let bytes = rest.as_bytes();
    // file:///C:/x: the slash before a drive letter is the URI's, not the path's
    let rest = if bytes.len() > 2 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':' { &rest[1..] } else { rest };
    PathBuf::from(percent_decode(rest))
}

/// cli/assets.mjs assetIdFromUri
pub(crate) fn asset_id(uri: &str) -> Option<String> {
    let uri = uri.trim();
    let lower = uri.to_ascii_lowercase();
    let digits = |text: &str| -> Option<String> {
        let id: String = text.chars().take_while(char::is_ascii_digit).collect();
        (!id.is_empty()).then_some(id)
    };
    if let Some(rest) = lower.strip_prefix("rbxassetid://") {
        return digits(rest);
    }
    // /roblox\.com\/asset\/?\?.*?\bid=(\d+)/i
    if let Some(at) = lower.find("roblox.com/asset") {
        let rest = &lower[at + "roblox.com/asset".len()..];
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        if let Some(query) = rest.strip_prefix('?') {
            let mut search = 0;
            while let Some(found) = query[search..].find("id=") {
                let start = search + found;
                let boundary = start == 0 || !query.as_bytes()[start - 1].is_ascii_alphanumeric() && query.as_bytes()[start - 1] != b'_';
                if boundary {
                    if let Some(id) = digits(&query[start + 3..]) {
                        return Some(id);
                    }
                }
                search = start + 3;
            }
        }
        return None;
    }
    (!uri.is_empty() && uri.bytes().all(|byte| byte.is_ascii_digit())).then(|| uri.to_string())
}

/// The UnapprovedImage icon's bytes (4,085, sha256 below): what the thumbnail service served for a Blocked
/// thumbnail before rur drew Studio's placeholder in its place
fn unapproved_icon(file: &Path) -> bool {
    use sha2::{Digest, Sha256};
    const SHA256: &str = "6eb7d741e290870a88e2cbbe6aaa55c0e6ff638de4daa01244dc2863ddfacbbf";
    if std::fs::metadata(file).map(|m| m.len()).ok() != Some(4085) {
        return false;
    }
    std::fs::read(file).is_ok_and(|bytes| {
        Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect::<String>() == SHA256
    })
}

/// An `rbxthumb://` query (URLSearchParams rules: `&`-separated, `+` and `%xx` decoded, the first value
/// of a name counts): `type` (default Asset), `id`, `w` and `h` (default 150)
pub(crate) struct ThumbnailQuery {
    pub kind: String,
    /// "null" when the URI names none, as cli/assets.mjs wrote it
    pub id: String,
    pub w: String,
    pub h: String,
}

impl ThumbnailQuery {
    pub fn parse(query: &str) -> Self {
        let mut params: BTreeMap<String, String> = BTreeMap::new();
        for pair in query.split('&').filter(|pair| !pair.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let decode = |text: &str| percent_decode(&text.replace('+', " "));
            params.entry(decode(key)).or_insert_with(|| decode(value));
        }
        let get = |key: &str, fallback: &str| params.get(key).filter(|value| !value.is_empty()).cloned().unwrap_or_else(|| fallback.to_string());
        Self { kind: get("type", "Asset"), id: params.get("id").cloned().unwrap_or_else(|| "null".into()), w: get("w", "150"), h: get("h", "150") }
    }

    /// The cache key: `thumb-<type>-<id>-<w>x<h>-webp`, non-word characters as `_` (the WebP rendition,
    /// stored as PNG; older PNG-rendition files under keys without `-webp` stay unused)
    pub fn key(&self) -> String {
        let key = format!("thumb-{}-{}-{}x{}-webp", self.kind, self.id, self.w, self.h);
        key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect()
    }
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let hex = |byte: u8| (byte as char).to_digit(16);
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 3 <= bytes.len() {
            if let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2])) {
                out.push((high * 16 + low) as u8);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// What a file's bytes are (cli/assets.mjs sniff): its cache extension, kind and MIME type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sniffed {
    pub ext: &'static str,
    pub kind: &'static str,
    pub mime: &'static str,
}

pub fn sniff(bytes: &[u8]) -> Sniffed {
    let is = |ext, kind, mime| Sniffed { ext, kind, mime };
    if bytes.len() >= 8 && bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return is(".png", "image", "image/png");
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return is(".jpg", "image", "image/jpeg");
    }
    if bytes.len() >= 6 && bytes.starts_with(b"GIF8") {
        return is(".gif", "image", "image/gif");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return is(".webp", "image", "image/webp");
    }
    if bytes.starts_with(b"BM") {
        return is(".bmp", "image", "image/bmp");
    }
    if bytes.len() >= 4 {
        match &bytes[..4] {
            [0, 1, 0, 0] | b"true" => return is(".ttf", "font", "font/ttf"),
            b"OTTO" => return is(".otf", "font", "font/otf"),
            b"ttcf" => return is(".ttc", "font", "font/collection"),
            _ => {}
        }
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(64)]);
    // (JavaScript's trimStart: white space and the byte order mark)
    let head = head.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    if head.starts_with('{') || head.starts_with('[') {
        return is(".json", "json", "application/json");
    }
    if head.starts_with("<roblox!") {
        return is(".rbxm", "model", "application/octet-stream");
    }
    if head.starts_with("<roblox") || head.starts_with("<?xml") {
        return is(".rbxmx", "model", "application/xml");
    }
    is(".bin", "unknown", "application/octet-stream")
}

/// cli/assets.mjs resolveModelTexture: /(?:rbxassetid:\/\/|[?&]id=)(\d+)/i
pub(crate) fn model_texture(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let mut best: Option<(usize, String)> = None;
    for pattern in ["rbxassetid://", "?id=", "&id="] {
        let mut search = 0;
        while let Some(found) = lower[search..].find(pattern) {
            let start = search + found;
            let digits: String = lower[start + pattern.len()..].chars().take_while(char::is_ascii_digit).collect();
            if !digits.is_empty() {
                if best.as_ref().is_none_or(|(at, _)| start < *at) {
                    best = Some((start, digits));
                }
                break;
            }
            search = start + 1;
        }
    }
    best.map(|(_, id)| id)
}

/// The Studio install fonts and built-in textures come from (the best of [`content_roots`])
pub fn studio_content() -> Option<PathBuf> {
    content_roots().into_iter().next()
}

/// cli/assets.mjs studioContentRoots
fn content_roots() -> Vec<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).or_else(|| {
        std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join("AppData").join("Local"))
    });
    content_roots_with(
        std::env::var_os("RUR_ROBLOX_CONTENT").map(PathBuf::from),
        || roblox_install::RobloxStudio::locate().ok().map(|studio| studio.content_path().to_path_buf()),
        local,
        crate::paths::is_standalone(),
    )
}

/// The folders that hold Studio's `content` folder, best first:
/// 1. the one `RUR_ROBLOX_CONTENT` names,
/// 2. the Windows installs under `local`, newest first,
/// 3. release builds only: the one the Studio locator finds.
///
/// The environment and the locator are parameters so tests can pass their own.
///
/// The locator is the one the font lookup uses: it knows Studio's macOS application bundle as well
/// as Windows installs. Checkout builds keep their lookup and its order, so they never ask it.
fn content_roots_with(
    content: Option<PathBuf>,
    locate_studio: impl FnOnce() -> Option<PathBuf>,
    local: Option<PathBuf>,
    standalone: bool,
) -> Vec<PathBuf> {
    let parent_of = |content_folder: &Path| content_folder.parent().map(Path::to_path_buf);
    let mut roots = Vec::new();
    if let Some(root) = content.as_deref().and_then(parent_of) {
        roots.push(root);
    }
    if let Some(local) = local {
        if let Ok(entries) = std::fs::read_dir(local.join("Roblox").join("Versions")) {
            let mut installs: Vec<(std::time::SystemTime, PathBuf)> = entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|dir| dir.join("RobloxStudioBeta.exe").exists())
                .filter_map(|dir| Some((std::fs::metadata(&dir).ok()?.modified().ok()?, dir)))
                .collect();
            installs.sort_by(|a, b| b.0.cmp(&a.0));
            roots.extend(installs.into_iter().map(|(_, dir)| dir));
        }
    }
    // The locator comes after the Versions lookup so that on Windows a release binary picks the same
    // textures as a checkout build, which never asks it. It stays for the machines the Versions
    // lookup cannot see: Linux, macOS and Windows without a Versions folder.
    if standalone {
        if let Some(root) = locate_studio().as_deref().and_then(parent_of) {
            if !roots.contains(&root) {
                roots.push(root);
            }
        }
    }
    roots
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    /// A resolver over `assets` (its own state dir); desktop builds download through `transport`
    #[cfg(feature = "desktop")]
    pub(crate) fn resolver_with(assets: &Path, offline: bool, transport: std::sync::Arc<dyn crate::download::Transport>) -> Resolver {
        Resolver {
            downloads: crate::download::Downloader::for_tests(assets, transport),
            repo: assets.to_path_buf(),
            assets: assets.to_path_buf(),
            worker: crate::viewports::ViewportWorker::new(assets, offline),
            offline,
            content_roots: None,
            viewports: Value::Null,
            persist_viewports: true,
        }
    }

    #[test]
    fn standalone_images_use_the_studio_locator_after_explicit_content() {
        // The locator finds a content folder in each platform's layout
        for (content, root) in [
            ("C:/Roblox/Versions/version-example/content", "C:/Roblox/Versions/version-example"),
            ("/Applications/RobloxStudio.app/Contents/Resources/content", "/Applications/RobloxStudio.app/Contents/Resources"),
            ("/opt/studio/content", "/opt/studio"),
        ] {
            let locate = || Some(PathBuf::from(content));
            assert_eq!(content_roots_with(None, locate, None, true), vec![PathBuf::from(root)]);
            // An explicit content folder comes first
            assert_eq!(
                content_roots_with(Some(PathBuf::from("override/content")), locate, None, true),
                vec![PathBuf::from("override"), PathBuf::from(root)],
            );
        }
        assert_eq!(content_roots_with(None, || None, None, true), Vec::<PathBuf>::new());
        // A checkout build never asks the locator
        assert!(content_roots_with(None, || panic!("checkout must not query the locator"), None, false).is_empty());
    }

    #[test]
    fn asset_ids_and_thumbnail_keys_follow_cli_assets() {
        assert_eq!(asset_id("rbxassetid://123"), Some("123".into()));
        assert_eq!(asset_id("RBXASSETID://77abc"), Some("77".into()));
        assert_eq!(asset_id("http://www.roblox.com/asset/?id=456"), Some("456".into()));
        assert_eq!(asset_id("https://www.roblox.com/asset?version=1&id=789"), Some("789".into()));
        assert_eq!(asset_id("321"), Some("321".into()));
        assert_eq!(asset_id("rbxgameasset://x"), None);
        let key = |query: &str| ThumbnailQuery::parse(query).key();
        assert_eq!(key("type=AvatarHeadShot&id=1&w=60&h=60"), "thumb-AvatarHeadShot-1-60x60-webp");
        assert_eq!(key("id=5"), "thumb-Asset-5-150x150-webp");
        // An empty value takes the default, as `params.get(name) || default` did; no id is "null"
        assert_eq!(key("type=&w=&id=5"), "thumb-Asset-5-150x150-webp");
        assert_eq!(key("type=Avatar"), "thumb-Avatar-null-150x150-webp");
        assert_eq!(model_texture("<roblox> <url>http://www.roblox.com/asset/?id=42</url>"), Some("42".into()));
        assert_eq!(model_texture("x rbxassetid://7 y ?id=3"), Some("7".into()));
    }

    #[test]
    fn sniff_follows_cli_assets() {
        let kinds = |bytes: &[u8]| (sniff(bytes).ext, sniff(bytes).kind);
        assert_eq!(kinds(&[0x89, b'P', b'N', b'G', 13, 10, 26, 10]), (".png", "image"));
        assert_eq!(kinds(&[0x89, b'P', b'N', b'G']), (".bin", "unknown"), "a PNG needs its 8-byte signature");
        assert_eq!(kinds(&[0xff, 0xd8, 0xff, 0]), (".jpg", "image"));
        assert_eq!(kinds(b"GIF89a"), (".gif", "image"));
        assert_eq!(kinds(b"RIFF\0\0\0\0WEBPVP8 "), (".webp", "image"));
        assert_eq!(kinds(b"BM.."), (".bmp", "image"));
        assert_eq!(kinds(&[0, 1, 0, 0, 0]), (".ttf", "font"));
        assert_eq!(kinds(b"OTTO"), (".otf", "font"));
        assert_eq!(kinds(b"ttcf"), (".ttc", "font"));
        assert_eq!(kinds(b"\xef\xbb\xbf  {\"faces\":[]}"), (".json", "json"));
        assert_eq!(kinds(b"<roblox!\x89\xff"), (".rbxm", "model"));
        assert_eq!(kinds(b"<roblox version=\"4\">"), (".rbxmx", "model"));
        assert_eq!(kinds(b"<?xml version"), (".rbxmx", "model"));
        assert_eq!(kinds(b"plain"), (".bin", "unknown"));
    }

    /// The same checks with any download answered by a transport that fails: no network either way
    #[cfg(feature = "desktop")]
    #[test]
    fn cached_assets_resolve_locally_only_as_rur_downloaded_them() {
        use crate::download::tests::Fake;
        let assets = std::env::temp_dir().join(format!("rur-asset-records-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&assets);
        std::fs::create_dir_all(assets.join("downloaded")).unwrap();
        let fake = Fake::new(|_| Err("no network in this test".into()));
        let (mut online, mut offline) = (resolver_with(&assets, false, fake.clone()), resolver_with(&assets, true, fake.clone()));
        let file = assets.join("123.png");
        let real = [137u8, 80, 78, 71, 1];
        let record = json!({"sha256": crate::engine::text::hinted::sha256_hex(&real), "size": real.len()});
        // A file another tool wrote: downloaded again; offline it is used
        std::fs::write(&file, real).unwrap();
        assert_eq!(online.local("rbxassetid://123"), None);
        assert_eq!(offline.local("rbxassetid://123"), Some(Ok(file.clone())));
        // Downloads are native; a ViewportFrame image without its snapshot has nothing to draw (no worker starts)
        let missing = online.resolve_all(&["rbxassetid://456".into(), "rbxassetid://456".into(), "rur-viewport://1/x".into()]);
        assert_eq!(missing.len(), 2);
        assert_eq!(missing["rur-viewport://1/x"].as_ref().unwrap_err(), "Missing ViewportFrame snapshot");
        assert!(missing["rbxassetid://456"].is_err());
        assert_eq!(fake.urls(), ["https://assetdelivery.roblox.com/v1/assetId/456"], "the one missing id was asked for, once");
        // rur's download (its record matches)
        std::fs::write(assets.join("downloaded").join("123.png.json"), record.to_string()).unwrap();
        assert_eq!(online.local("rbxassetid://123"), Some(Ok(file.clone())));
        // Changed since: other bytes under the name
        std::fs::write(&file, [137u8, 80, 78, 71, 2, 2]).unwrap();
        assert_eq!(online.local("rbxassetid://123"), None);
        // Offline: no font is fetched and nothing is asked
        let fonts = json!({"missingFontAssets": ["rbxassetid://789"]});
        assert!(!offline.fetch_fonts(&fonts).unwrap());
        assert_eq!(fake.urls().len(), 1);
        let _ = std::fs::remove_dir_all(&assets);
    }

    #[test]
    fn file_uris_resolve_to_their_files() {
        assert_eq!(file_path("file:///C:/x%20y/a.png"), PathBuf::from("C:/x y/a.png"));
        assert_eq!(file_path("file://C:/Users/a b/c.png"), PathBuf::from("C:/Users/a b/c.png"));
        assert_eq!(file_path("file:///tmp/a.png"), PathBuf::from("/tmp/a.png"));
        let file = std::env::temp_dir().join(format!("rur-file-uri-{}.png", std::process::id()));
        std::fs::write(&file, [137u8, 80, 78, 71]).unwrap();
        let mut resolver = Resolver::new(std::env::temp_dir(), true);
        let path = file.display().to_string().replace('\\', "/");
        assert_eq!(resolver.local(&format!("file://{path}")), Some(Ok(PathBuf::from(&path))));
        assert_eq!(resolver.local(&format!("file:///{}", path.replace(' ', "%20"))), Some(Ok(PathBuf::from(&path))));
        assert!(matches!(resolver.local("file:///nowhere/at/all.png"), Some(Err(message)) if message.starts_with("no such file")));
        let missing = resolver.resolve_all(&["file:///nowhere/at/all.png".into()]);
        assert!(!missing.values().next().unwrap().as_ref().unwrap_err().contains(crate::paths::HELPER_REQUIRED));
        let _ = std::fs::remove_file(&file);
    }
}
