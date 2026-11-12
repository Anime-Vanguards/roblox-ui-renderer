//! Content URI -> local file, for `rur render` output directories (story_render.rs).
//!
//! The rules are cli/assets.mjs resolveUri's: `rbxasset://` from the local Studio install,
//! `rbxassetid://N` (and `roblox.com/asset/?id=N`, bare ids) from the asset cache
//! (`<state dir>/assets/<id>.<ext>` when its download record matches it, offline any such file; a
//! cached decal / texture model resolves to the image it references), `rbxthumb://` from its cached
//! WebP rendition, `file://` from its file (the PNG snapshot the engine emulator writes of an
//! EditableImage a script shows). Everything this module answers is local and needs no helper; a URI
//! that needs a download (a cache file rur did not download included, or a cache entry only Node
//! migrates) goes to cli/native-assets.mjs, which owns authentication, downloads and the cache
//! layout. The error texts are resolveUri's, so a render's `missingImages` read the same either way.
use std::{
    collections::{BTreeMap, HashMap},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::SystemTime,
};

use serde_json::{Value, json};

/// A resolved URI: the local file, or why there is none.
pub type Resolution = Result<PathBuf, String>;

pub struct Resolver {
    repo: PathBuf,
    assets: PathBuf,
    offline: bool,
    /// No Node helper (a binary outside a checkout): only local content resolves, and what needs a
    /// download is reported as needing the rur CLI from a checkout
    local_only: bool,
    helper: Option<Helper>,
    content_roots: Option<Vec<PathBuf>>,
    viewports: Value,
    /// Missing cloud fonts already reported when `local_only`, so each is printed once
    reported_fonts: std::collections::HashSet<String>,
    /// Cache files that matched their download record, by the length and modification time they had then
    verified: HashMap<PathBuf, (u64, Option<SystemTime>)>,
}

struct Helper {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next: u64,
}

/// Adds why an uncached asset can't be downloaded here to the "offline and not cached" error.
fn with_helper_hint(error: String) -> String {
    if error == "offline and not cached" {
        format!("{error}; {}", crate::paths::HELPER_REQUIRED)
    } else {
        error
    }
}

impl Drop for Helper {
    /// Asks the helper to quit and gives it a moment; a helper that does not exit is killed
    fn drop(&mut self) {
        let _ = writeln!(self.input, "{}", json!({"command":"quit"}));
        let _ = self.input.flush();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

const EXTENSIONS: [&str; 12] = [
    ".png", ".jpg", ".gif", ".webp", ".bmp", ".ttf", ".otf", ".ttc", ".json", ".rbxm", ".rbxmx", ".bin",
];

impl Resolver {
    pub fn new(repo: impl Into<PathBuf>, offline: bool) -> Self {
        let repo = repo.into();
        let assets = crate::state_dir(&repo).join("assets");
        let local_only = !crate::paths::has_asset_helper(&repo);
        Self { repo, assets, offline, local_only, helper: None, content_roots: None, viewports: Value::Null, reported_fonts: Default::default(), verified: HashMap::new() }
    }

    pub fn set_offline(&mut self, offline: bool) {
        if self.offline != offline {
            self.helper = None;
        }
        self.offline = offline;
    }

    /// Resolves every URI (in order, duplicates once): local answers first, the rest through the
    /// helper in one request.
    pub fn resolve_all(&mut self, uris: &[String]) -> BTreeMap<String, Resolution> {
        let mut results = BTreeMap::new();
        let mut remote = Vec::new();
        for uri in uris {
            if results.contains_key(uri) || remote.contains(uri) {
                continue;
            }
            match self.local(uri) {
                Some(result) => {
                    let result = if self.local_only { result.map_err(with_helper_hint) } else { result };
                    results.insert(uri.clone(), result);
                }
                None => remote.push(uri.clone()),
            }
        }
        if !remote.is_empty() {
            match self.helper_images(&remote) {
                Ok(answers) => {
                    for uri in remote {
                        let answer = answers.get(&uri).cloned().unwrap_or_else(|| Err("unresolved".into()));
                        results.insert(uri, answer);
                    }
                }
                Err(error) => {
                    for uri in remote {
                        results.insert(uri, Err(error.clone()));
                    }
                }
            }
        }
        results
    }

    pub fn resolve_scene(&mut self, scene: &Value, uris: &[String]) -> BTreeMap<String, Resolution> {
        self.viewports = crate::assets::viewport_requests(scene["nodes"].as_array().into_iter().flatten(), uris);
        let result = self.resolve_all(uris);
        self.viewports = Value::Null;
        result
    }

    pub fn model(&mut self, request: Value) -> Result<Value, String> {
        self.helper_call(json!({"command":"model","model":request}))
    }

    /// Downloads the cloud fonts a run reported missing (scene.diagnostics); true when any arrived,
    /// so the run is repeated with them (cli/rur.mjs fetchMissingFonts).
    pub fn fetch_fonts(&mut self, diagnostics: &Value) -> Result<bool, String> {
        if self.local_only {
            // Nothing to download with: say once per font that the helper is needed
            for font in diagnostics["missingFontAssets"].as_array().into_iter().flatten() {
                let key = font.to_string();
                if self.reported_fonts.insert(key.clone()) {
                    eprintln!("Missing font {key}: {}", crate::paths::HELPER_REQUIRED);
                }
            }
            return Ok(false);
        }
        if self.offline {
            return Ok(false);
        }
        let reply = self.helper_call(json!({"command":"fonts","diagnostics":diagnostics}))?;
        Ok(reply["fontsChanged"].as_bool().unwrap_or(false))
    }

    /// The local answer for `uri`, or None when only the helper can answer it.
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
            let key = thumbnail_key(&trimmed["rbxthumb://".len()..]);
            return match self.cached(&key) {
                // A cache entry holding the UnapprovedImage icon (a Blocked thumbnail cached before the placeholder
                // rule) is left to the helper, which draws Studio's placeholder (cli/assets.mjs fetchThumbnail)
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

    /// A cached asset id (fetchAssetId without the network): a model resolves to the texture it names.
    fn asset(&mut self, id: &str, allow_decal: bool) -> Option<Resolution> {
        let Some(file) = self.cached(id) else {
            return self.offline.then(|| Err("offline and not cached".into()));
        };
        // A pre-unwrap .bin entry is migrated by the helper; a font is recorded in the font index there
        let name = file.to_string_lossy().to_ascii_lowercase();
        if name.ends_with(".bin") || [".ttf", ".otf", ".ttc"].iter().any(|ext| name.ends_with(ext)) {
            return None;
        }
        let bytes = std::fs::read(&file).ok()?;
        // A file rur did not download under this name is downloaded again by the helper
        if !self.offline && !self.local_only && !self.downloaded(&file, &bytes) {
            return None;
        }
        if allow_decal && is_model(&bytes) {
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

    fn cached(&self, key: &str) -> Option<PathBuf> {
        EXTENSIONS.iter().map(|ext| self.assets.join(format!("{key}{ext}"))).find(|file| file.is_file())
    }

    /// cli/assets.mjs downloaded: whether `file` holds what rur downloaded under its name, its sha256
    /// and size in `assets/downloaded/<file name>.json`. Hashed once per file length and modification time.
    fn downloaded(&mut self, file: &Path, bytes: &[u8]) -> bool {
        let stamp = std::fs::metadata(file).map(|metadata| (metadata.len(), metadata.modified().ok())).ok();
        if stamp.is_some() && self.verified.get(file) == stamp.as_ref() {
            return true;
        }
        let Some(name) = file.file_name() else { return false };
        let record = self.assets.join("downloaded").join(format!("{}.json", name.to_string_lossy()));
        let Some(record) = std::fs::read(record).ok().and_then(|text| serde_json::from_slice::<Value>(&text).ok()) else {
            return false;
        };
        let matches = record["size"].as_u64() == Some(bytes.len() as u64)
            && record["sha256"].as_str() == Some(crate::engine::text::hinted::sha256_hex(bytes).as_str());
        if let (true, Some(stamp)) = (matches, stamp) {
            self.verified.insert(file.to_path_buf(), stamp);
        }
        matches
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

    fn helper_images(&mut self, uris: &[String]) -> Result<BTreeMap<String, Resolution>, String> {
        // persistViewports: one-shot renders reuse a snapshot's image across processes (cli/viewports.mjs)
        let reply = self.helper_call(json!({"images": uris, "viewports": self.viewports, "viewportSlots": uris.iter().filter_map(|uri| crate::assets::viewport_slot(uri)).collect::<Vec<_>>(), "persistViewports": true}))?;
        let mut answers = BTreeMap::new();
        for entry in reply["images"].as_array().into_iter().flatten() {
            let Some(uri) = entry["uri"].as_str() else { continue };
            let answer = match (entry["file"].as_str(), entry["error"].as_str()) {
                (Some(file), _) => Ok(PathBuf::from(file)),
                (None, Some(error)) => Err(error.to_string()),
                _ => Err("unresolved".into()),
            };
            answers.insert(uri.to_string(), answer);
        }
        Ok(answers)
    }

    fn helper_call(&mut self, mut request: Value) -> Result<Value, String> {
        if self.local_only {
            return Err(crate::paths::HELPER_REQUIRED.into());
        }
        if self.helper.is_none() {
            self.helper = Some(self.start_helper()?);
        }
        let helper = self.helper.as_mut().unwrap();
        helper.next += 1;
        request["id"] = json!(helper.next);
        let result = (|| {
            writeln!(helper.input, "{request}").map_err(|error| error.to_string())?;
            helper.input.flush().map_err(|error| error.to_string())?;
            let mut line = String::new();
            if helper.output.read_line(&mut line).map_err(|error| error.to_string())? == 0 {
                return Err("the asset helper exited".to_string());
            }
            serde_json::from_str::<Value>(&line).map_err(|error| error.to_string())
        })();
        match result {
            Ok(reply) if reply["error"].is_string() => Err(reply["error"].as_str().unwrap().to_string()),
            Ok(reply) => Ok(reply),
            Err(error) => {
                self.helper = None;
                Err(format!("asset helper: {error}"))
            }
        }
    }

    fn start_helper(&self) -> Result<Helper, String> {
        let node = std::env::var_os("RUR_NODE").unwrap_or_else(|| "node".into());
        let mut command = Command::new(node);
        command
            .arg(crate::paths::asset_helper(&self.repo)?)
            .current_dir(&self.repo)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if self.offline {
            command.arg("--offline");
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn().map_err(|error| format!("starting node for cli/native-assets.mjs: {error}"))?;
        let input = child.stdin.take().ok_or("asset helper has no stdin")?;
        let output = BufReader::new(child.stdout.take().ok_or("asset helper has no stdout")?);
        Ok(Helper { child, input, output, next: 0 })
    }
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
fn asset_id(uri: &str) -> Option<String> {
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

/// cli/assets.mjs fetchThumbnail's cache key: `thumb-<type>-<id>-<w>x<h>-webp`, non-word characters
/// as `_`
/// The UnapprovedImage icon's bytes (4,085, sha256 below): what the thumbnail service served for a Blocked
/// thumbnail before cli/assets.mjs drew Studio's placeholder in its place
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

fn thumbnail_key(query: &str) -> String {
    let mut params: BTreeMap<String, String> = BTreeMap::new();
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let decode = |text: &str| percent_decode(&text.replace('+', " "));
        params.entry(decode(key)).or_insert_with(|| decode(value));
    }
    let get = |key: &str, fallback: &str| params.get(key).cloned().unwrap_or_else(|| fallback.to_string());
    let key = format!("thumb-{}-{}-{}x{}-webp", get("type", "Asset"), params.get("id").cloned().unwrap_or_else(|| "null".into()), get("w", "150"), get("h", "150"));
    key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect()
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

/// cli/assets.mjs sniff: an rbxm / rbxmx model
fn is_model(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(64)]);
    let head = head.trim_start();
    head.starts_with("<roblox") || head.starts_with("<?xml")
}

/// cli/assets.mjs resolveModelTexture: /(?:rbxassetid:\/\/|[?&]id=)(\d+)/i
fn model_texture(text: &str) -> Option<String> {
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
mod tests {
    use super::*;

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
        assert_eq!(thumbnail_key("type=AvatarHeadShot&id=1&w=60&h=60"), "thumb-AvatarHeadShot-1-60x60-webp");
        assert_eq!(thumbnail_key("id=5"), "thumb-Asset-5-150x150-webp");
        assert_eq!(model_texture("<roblox> <url>http://www.roblox.com/asset/?id=42</url>"), Some("42".into()));
        assert_eq!(model_texture("x rbxassetid://7 y ?id=3"), Some("7".into()));
    }

    #[test]
    fn cached_assets_resolve_locally_only_as_rur_downloaded_them() {
        let assets = std::env::temp_dir().join(format!("rur-asset-records-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&assets);
        std::fs::create_dir_all(assets.join("downloaded")).unwrap();
        let resolver = |offline| Resolver { repo: assets.clone(), assets: assets.clone(), offline, local_only: false, helper: None, content_roots: None, viewports: Value::Null, reported_fonts: Default::default(), verified: HashMap::new() };
        let (mut online, mut offline) = (resolver(false), resolver(true));
        let file = assets.join("123.png");
        let real = [137u8, 80, 78, 71, 1];
        let record = json!({"sha256": crate::engine::text::hinted::sha256_hex(&real), "size": real.len()});
        // A file another tool wrote: the helper downloads it again; offline it is used
        std::fs::write(&file, real).unwrap();
        assert_eq!(online.local("rbxassetid://123"), None);
        let mut standalone = resolver(false);
        standalone.local_only = true;
        assert_eq!(standalone.local("rbxassetid://123"), Some(Ok(file.clone())));
        let missing = standalone.resolve_all(&["rbxassetid://456".into(), "rbxassetid://456".into(), "rur-viewport://1".into()]);
        assert_eq!(missing.len(), 2);
        assert!(missing.values().all(|result| result.as_ref().unwrap_err().contains(crate::paths::HELPER_REQUIRED)));
        let fonts = json!({"missingFontAssets": ["rbxassetid://789", "rbxassetid://789"]});
        assert!(!standalone.fetch_fonts(&fonts).unwrap());
        assert!(!standalone.fetch_fonts(&fonts).unwrap());
        assert_eq!(standalone.reported_fonts.len(), 1);
        assert_eq!(offline.local("rbxassetid://123"), Some(Ok(file.clone())));
        // rur's download (its record matches)
        std::fs::write(assets.join("downloaded").join("123.png.json"), record.to_string()).unwrap();
        assert_eq!(online.local("rbxassetid://123"), Some(Ok(file.clone())));
        // Changed since: other bytes under the name
        std::fs::write(&file, [137u8, 80, 78, 71, 2, 2]).unwrap();
        assert_eq!(online.local("rbxassetid://123"), None);
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
